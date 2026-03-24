//! Query 模块 - LLM 请求构建与执行
//!
//! 根据设计方案，使用 Action 历史替代 ToolMessage 机制

use log;
use std::pin::Pin;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::instrument;
use futures::stream::{Stream, StreamExt};
use crate::Registry;
use crate::messages::*;
use crate::llm::{LlmApiType, LlmProvider, ChatCompletionsProvider};
use crate::error::{JieyushaError, Result};
use crate::tool::{Tool, ToolUseContext};
use crate::memory;
use crate::agent;

#[instrument(level="info", skip_all)]
async fn query_llm(
    messages: &Vec<Message>,
    system_prompt: &Vec<String>,
    tools: &Vec<Arc<dyn Tool>>,
    tool_use_context: &ToolUseContext,
) -> Result<AssistantMessage> { 
    let name = tool_use_context.model.as_ref().cloned().unwrap_or("main".to_string());
    let model = Registry::instance().get_model_profile(&name).unwrap();

    // 构建 system prompt：包含结构化 Memory XML
    let mut full_system_prompt = system_prompt.join("\n");
    
    // 如果有 root_path，添加结构化 Memory XML（不含 action 历史，action 历史通过 messages 传递）
    if let Some(ref root_path) = tool_use_context.root_path {
        if let Ok(merged_xml) = memory::get_structural_memory_xml(root_path) {
            full_system_prompt.push_str("\n\n");
            full_system_prompt.push_str(&merged_xml);
        }
    }

    let request = UnifiedRequest {
        model: model.clone(),
        system_prompt: vec![full_system_prompt],
        messages: messages.clone(),
        tools: Some(tools.clone()),
        stream: false,
    };

    let response = match LlmApiType::determine_api_type(&model) {
        LlmApiType::ChatCompletions => { 
            ChatCompletionsProvider::request(request).await
        },
        _ => {
            return Err(JieyushaError::LlmError("Unsupported API type".to_string()));
        }
    };

    response
}

pub fn query<'a>(
    messages: Vec<Message>,
    system_prompt: Vec<String>,
    tool_use_context: ToolUseContext,
    context: HashMap<String, String>
) -> Pin<Box<dyn Stream<Item = Message> + Send>> {
    let tools = tool_use_context.tools.clone();
    log::info!(
        "{}: Query LLM with context: {{
            Tools: {},
        }}", 
        tool_use_context.agent_id, 
        tools.iter().map(|t| t.name()).collect::<Vec<&str>>().join(", ")
    );

    Box::pin(async_stream::stream! {
        // 如果有 root_path，从 history 差量文件动态重建消息链
        let effective_messages = if let Some(ref root_path) = tool_use_context.root_path {
            match memory::build_messages_from_history(root_path) {
                Ok(msgs) => msgs,
                Err(e) => {
                    log::warn!("Failed to build messages from history: {}, using provided messages", e);
                    messages.clone()
                }
            }
        } else {
            messages.clone()
        };

        let assistant_message = match query_llm(
            &effective_messages,
            &system_prompt,
            &tools,
            &tool_use_context,
        ).await {
            Ok(msg) => msg,
            Err(e) => {
                yield Message::Assistant(AssistantMessage::new(&format!("LLM query error: {}", e)));
                return;
            }
        };

        if tool_use_context.abort_signal {
            yield Message::Assistant(assistant_message);
            return;
        }

        // 保存 thought action
        if let Some(ref root_path) = tool_use_context.root_path {
            if !assistant_message.content.is_empty() {
                if let Err(e) = memory::create_thought_delta(root_path, &assistant_message.content) {
                    log::error!("Failed to save thought delta: {}", e);
                }
            }
        }

        yield Message::Assistant(assistant_message.clone());

        if let Some(tool_uses) = &assistant_message.tool_uses {
            for tool_use in tool_uses {
                let mut current_tool_use_context = tool_use_context.clone();
                current_tool_use_context.tool_use_id = tool_use.id.clone();
                
                let agent_id = tool_use_context.agent_id.clone();
                log::info!(
                    "{}: Call Tool: {{                    
                        tool_name: {}                  
                        tool_use_id: {}                   
                        tool_arguments: {}                
                    }}",
                    agent_id,
                    tool_use.name,
                    tool_use.id,
                    tool_use.arguments
                );

                let tool_stream = run_tools_stream(tool_use, &assistant_message, current_tool_use_context.clone());
                futures::pin_mut!(tool_stream);
                while let Some(message) = tool_stream.next().await {
                    if let Message::Tool(tool_message) = &message {
                        log::info!(
                            "{}: Tool Result: {{
                                tool_name: {}
                                tool_use_id: {}
                                tool_result: {}
                            }}",
                            agent_id,
                            tool_use.name,
                            tool_message.tool_use_id,
                            tool_message.content
                        );

                        // 保存 toolcall delta（工具结果通过 Memory XML 传递，不再进入消息链）
                        if let Some(ref root_path) = tool_use_context.root_path {
                            if let Err(e) = memory::create_toolcall_delta(root_path, tool_use, tool_message) {
                                log::error!("Failed to save toolcall delta: {}", e);
                            }
                        }
                        // 注意：不再收集 ToolMessage 到消息列表
                    } else {
                        // Progress 等消息继续 yield
                        yield message.clone();
                    }
                }

                // 检查是否需要调用 Schedule Agent
                if let Some(ref root_path) = tool_use_context.root_path {
                    let history_dir = root_path.join("history");
                    if memory::should_generate_summary(&history_dir) {
                        log::info!("{}: History exceeds 10 actions, calling Schedule Agent", agent_id);
                        
                        // 调用 Schedule Agent
                        if let Err(e) = call_schedule_agent(root_path).await {
                            log::error!("Failed to call Schedule Agent: {}", e);
                        }
                    }
                }

                // 消息将从 history 差量文件动态重建，无需手动维护消息链
                
                let recursive_stream = query(
                    vec![],  // 占位，会被 history 重建覆盖
                    system_prompt.clone(),
                    current_tool_use_context.clone(),
                    context.clone()
                );

                futures::pin_mut!(recursive_stream);
                while let Some(msg) = recursive_stream.next().await {
                    yield msg;
                }
            }
        }
    })
}

fn run_tools_stream(
    tool_use: &ToolUse,
    _assistant_message: &AssistantMessage,
    tool_use_context: ToolUseContext,
) -> impl Stream<Item = Message> {
    async_stream::stream! {
        let tool = match Registry::instance().get_tool(&tool_use.name) {
            Some(t) => t,
            None => {
                yield Message::Tool(ToolMessage::from_error(
                    format!("Tool {} not found", tool_use.name),
                    tool_use_context.tool_use_id.clone(),
                ));
                return;
            }
        };   

        let input_data = match serde_json::from_str::<serde_json::Value>(&tool_use.arguments) {
            Ok(data) => data,
            Err(e) => {
                yield Message::Tool(ToolMessage::from_error(
                    format!("Failed to parse \"{}\" call arguments: {}", tool_use.name, e),
                    tool_use_context.tool_use_id.clone(),
                ));
                return;
            }
        };

        let tool_result = tool.call(&input_data, &tool_use_context).await;
        let mut tool_stream = tool_result.stream;
        while let Some(message) = tool_stream.next().await {
            yield message;
        }
    }
}

/// 调用 Schedule Agent 进行上下文整理
async fn call_schedule_agent(root_path: &std::path::Path) -> Result<()> {
    // 获取 schedule agent 配置
    let agent_config = Registry::instance().get_agent("schedule")
        .ok_or_else(|| JieyushaError::ConfigError("Schedule agent not found".to_string()))?;
    
    // 获取当前进度和历史动作
    let current_progress = memory::get_current_progress_xml(root_path)?;
    let history_actions = memory::get_history_actions_xml(root_path)?;
    
    // 构建 prompt
    let prompt = format!(
        r#"请根据以下信息进行上下文整理：

<current_progress>
{}
</current_progress>

<history-actions>
{}
</history-actions>

请直接输出 <current-progress> XML 内容。"#,
        current_progress,
        history_actions
    );
    
    // 构建完整的 prompt（包含 agent 的 system prompt）
    let effective_prompt = format!("{}\n{}", agent_config.system_prompt, prompt);
    
    // 获取模型配置
    let model = Registry::instance().get_model_profile("main")
        .ok_or_else(|| JieyushaError::ConfigError("Main model not found".to_string()))?;
    
    // 构建 LLM 请求
    let request = UnifiedRequest {
        model: model.clone(),
        system_prompt: vec![agent::get_agent_prompt()],
        messages: vec![Message::User(UserMessage::new(effective_prompt))],
        tools: None,  // Schedule agent 不需要工具
        stream: false,
    };
    
    // 调用 LLM
    let response = match LlmApiType::determine_api_type(&model) {
        LlmApiType::ChatCompletions => ChatCompletionsProvider::request(request).await?,
        _ => return Err(JieyushaError::LlmError("Unsupported API type".to_string())),
    };
    
    // 解析返回的 current-progress XML
    let content = response.content;
    
    // 提取 <current-progress> 内容
    let progress_xml = if let Some(start) = content.find("<current-progress>") {
        if let Some(end) = content.find("</current-progress>") {
            content[start..end + 19].to_string()
        } else {
            log::warn!("Schedule Agent response missing closing tag");
            return Err(JieyushaError::ConfigError("Invalid current-progress XML".to_string()));
        }
    } else {
        log::warn!("Schedule Agent response missing current-progress tag");
        return Err(JieyushaError::ConfigError("Missing current-progress in response".to_string()));
    };
    
    // 保存 Schedule Agent 结果
    memory::save_schedule_result(root_path, &progress_xml)?;
    
    // 清理历史动作文件
    memory::clear_history_actions(root_path)?;
    
    log::info!("Schedule Agent completed, current-progress updated");
    
    Ok(())
}
