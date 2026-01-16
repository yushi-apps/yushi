use log;
use std::pin::Pin;
use std::future::Future;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::instrument;
use crate::Registry;
use crate::messages::*;
use crate::llm::{LlmApiType, LlmProvider, ChatCompletionsProvider};
use crate::error::{JieyushaError, Result};
use crate::tool::{Tool, ToolMessage, ToolUseContext};

#[instrument(skip_all)]
pub fn query<'a>(
    messages: &'a Vec<Message>,
    system_prompt: &'a Vec<String>,
    tool_use_context: &'a mut ToolUseContext,
    context: HashMap<String, String>
) -> Pin<Box<dyn Future<Output = Result<AssistantMessage>> + Send + 'a>> {
        Box::pin(async move {
        let tools = tool_use_context.tools.clone();

        log::info!(
            "{}: Query LLM with context: {{
                Tools: {},
            }}", 
            tool_use_context.agent_id, 
            tools.iter().map(|t| t.name()).collect::<Vec<&str>>().join(", ")
        );

        let assistant_message = query_llm(
            messages,
            system_prompt,
            &tools,
            tool_use_context,
        ).await?;

        if tool_use_context.abort_signal {
            return Ok(AssistantMessage::new("Operation interrupted"));
        }

        if let Some(tool_uses) = &assistant_message.tool_uses {
            let mut tool_results: Vec<Message> = Vec::new();

            for tool_use in tool_uses {
                tool_use_context.tool_use_id = tool_use.id.clone();

                log::info!(
                    "{}: Call Tool: {{                    
                        tool_name: {}                  
                        tool_use_id: {}                   
                        tool_arguments: {}                
                    }}",
                    tool_use_context.agent_id,
                    tool_use.name,
                    tool_use.id,
                    tool_use.arguments
                );

                let tool_message = run_tool(tool_use, &assistant_message, tool_use_context).await;

                log::info!(
                    "{}: Tool Result: {{
                        tool_name: {}
                        tool_use_id: {}
                        tool_result: {}
                    }}",
                    tool_use_context.agent_id,
                    tool_use.name,
                    tool_message.tool_use_id,
                    tool_message.content
                );

                tool_results.push(Message::Tool(tool_message));
            }

            if tool_use_context.abort_signal {
                return Ok(AssistantMessage::new("Tool operation interrupted"));
            }

            let mut all_messages = messages.clone();
            all_messages.push(Message::Assistant(assistant_message));
            if !tool_results.is_empty() {
                all_messages.extend(tool_results.clone());
            }

            //log::info!(
            //        "{}: Context Overview:{{
            //            Tools: {},
            //        }}", 
            //        tool_use_context.agent_id, 
            //        tool_use_context.tools.iter().map(|t| t.name()).collect::<Vec<&str>>().join(", ")
            //    );

            return query(
                &all_messages,
                system_prompt,
                tool_use_context,
                context
            ).await;
        }

        log::info!("No tools to use so return assistant message: {assistant_message:?}");
        Ok(assistant_message)
    })
}


#[instrument(level="info", skip_all)]
async fn query_llm(
    messages: &Vec<Message>,
    system_prompt: &Vec<String>,
    tools: &Vec<Arc<dyn Tool>>,
    tool_use_context: &mut ToolUseContext,
) -> Result<AssistantMessage> { 
    let name = tool_use_context.model.as_ref().cloned().unwrap_or("main".to_string());
    let model = Registry::instance().get_model_profile(&name).unwrap();

    let request = UnifiedRequest {
        model: model.clone(),
        system_prompt: system_prompt.clone(),
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

    

    return response
}

async fn run_tool(
    tool_use: &ToolUse,
    _assistant_message: &AssistantMessage,
    tool_use_context: &mut ToolUseContext,
) -> ToolMessage {
    let tool = match Registry::instance().get_tool(&tool_use.name) {
        Some(t) => t,
        None => {
            return ToolMessage::new_error(
                format!("Tool {} not found", tool_use.name),
                tool_use_context.tool_use_id.clone(),
            );
        }
    };
    
    let input_data = match serde_json::from_str::<serde_json::Value>(&tool_use.arguments) {
        Ok(data) => data,
        Err(e) => {
            return ToolMessage::new_error(
                format!("Failed to parse \"{}\" call arguments: {}", tool_use.name, e),
                tool_use_context.tool_use_id.clone(),
            );
        }
    };

    // Todo validate input data
    match tool.call(&input_data, tool_use_context).await {
        Ok(message) => message,
        Err(e) => {
            return ToolMessage::new_error(
                format!("Tool '{}' execute failed: {}", tool_use.name, e),
                tool_use_context.tool_use_id.clone(),
            );
        }
    }
}