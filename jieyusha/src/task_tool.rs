use std::sync::Arc;
use std::collections::HashMap;
use async_trait::async_trait;
use uuid::Uuid;
use crate::agent;
use crate::Registry;
use crate::messages::*;
use crate::query::query;
use crate::error::{JieyushaError, Result};
use crate::tool::{Tool, ToolMessage, ToolUseContext};

pub struct TaskTool;

#[async_trait]
impl Tool for TaskTool {
    fn name(&self) -> &str {
        "Task"
    }

    fn input_json_schema(&self) -> &str {
        r#"
        {
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task."
                },
                "prompt": {
                    "type": "string",
                    "description": "The task for the agent to perform."
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of specialized agent to use for this task."
                }
            },
            "required": ["description", "prompt", "subagent_type"]
        }
        "#
    }

    fn description(&self) -> &str {
        "Launch a new task"
    }

    async fn prompt(&self) -> String {
        let agent_descriptions: Vec<String> = Registry::instance().get_all_agents()
            .into_iter()
            .map(|agent| {
                let tools_list = agent.tools.join(", ");
                format!("- {}: {} (Tools: {})", agent.agent_type, agent.description, tools_list)
            })
            .collect();

        let agent_descriptions_text = agent_descriptions.join("\n");
        log::info!("Sub Agents for TaskTool: {}", agent_descriptions_text);

        format!(
            r#"启动一个新代理以自主处理复杂的多步骤任务。

可用代理类型及其可访问的工具：
   {}

使用该工具时，必须指定 'subagent_type' 参数来选择要使用的代理类型。

何时使用代理工具：
- 当收行自定义斜杠命令的指示时。使用代理工具，并将整个斜杠命令调用作为提示内容。斜杠命令可以接受参数。例如：Task(description="检查文件", prompt="/check-file path/to/file.py")

何时不应使用代理工具：
- 与上理描述无关的其他任务

使用说明：
1. 代完成任务后，会向您返回单条消息。代理返回的结果对用户不可见。如需向用户展示结果，您应向用户发送一条包含结果简要摘要的文本消息。
2. 每代理调用都是无状态的。您无法向代理发送额外消息，代理也无法在其最终报告之外与您通信。因此，您的提示应包含高度详细的任务描述供代理自主执行，并明确指定代理应在其最终且唯一的消息中向您返回哪些信息。
3. 通常应信任代理的输出
4. 若理描述中提到应主动使用该代理，则应尽量在用户未主动要求的情况下率先使用。请自行判断。
        "#, agent_descriptions_text
        )
    }

    async fn call(&self, input_data: &serde_json::Value, context: &mut ToolUseContext) -> Result<ToolMessage> {
        let agent_type = match input_data.get("subagent_type").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return Err(JieyushaError::ToolError("missing or non-string subagent_type".to_string())),
        };

        let prompt = match input_data.get("prompt").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return Err(JieyushaError::ToolError("missing or non-string prompt".to_string())),
        };

        let agent_config = match Registry::instance().get_agent(agent_type) {
            Some(config) => config,
            None => {
                let avaliable_types = Registry::instance().get_all_agent_types();
                return Ok(ToolMessage::new_error(
                    format!("Agent type {} not found. Available types: {:?}", agent_type, avaliable_types),
                    context.tool_use_id.clone(),
                ));
            } 
        };

        let effective_prompt = format!("{}\n{}", agent_config.system_prompt, prompt);
        let messages = vec![Message::User(UserMessage::new(effective_prompt))];
        // Global prompt for all agents.

        let agent_id = format!("{}-{}", agent_type, Uuid::new_v4().to_string());
        log::info!("{}: Create a new sub-agent {}", context.agent_id, agent_id);
        let task_prompt = agent::get_agent_prompt();
        let mut tool_use_contenxt = ToolUseContext {
            model: None,
            tools: self.get_task_tools(agent_config.tools), 
            agent_id: agent_id.clone(),
            abort_signal: false,
            tool_use_id: Uuid::new_v4().to_string(),
        };

        let assistant = query(
            &messages,
            &vec![task_prompt],
            &mut tool_use_contenxt,
            HashMap::new(),
        ).await?;

        log::info!("{}: Sub-agent {} finished", context.agent_id, agent_id);
        Ok(ToolMessage::new_content(assistant.content.clone(), context.tool_use_id.clone()))
    }
}

impl TaskTool {
    fn get_task_tools(&self, tool_names: Vec<String>) -> Vec<Arc<dyn Tool>> {
        let mut tools = vec![];
        for name in tool_names {
            if name != self.name() {
                if let Some(tool) = Registry::instance().get_tool(&name) {
                    tools.push(tool);
                }
            }
        }

        tools
    }

}