use std::sync::Arc;
use futures::Stream;
use async_trait::async_trait;
use crate::error::Result;
use crate::messages::Message;

#[derive(Debug, Clone)]
pub struct ToolMessage {
    pub r#type: String,
    pub content: String,
    pub is_error: bool,
    pub tool_use_id: String,
}

pub type ToolResult = Box<dyn Stream<Item = Message> + Unpin + Send>;

impl ToolMessage {
    pub fn new_error(error: impl Into<String>, tool_use_id: impl Into<String>) -> Self {
        Self {
            r#type: "tool_result".to_string(),
            content: error.into(),
            is_error: true,
            tool_use_id: tool_use_id.into(),
        }
    }

    pub fn new_content(content: impl Into<String>, tool_use_id: impl Into<String>) -> Self {
        Self {
            r#type: "tool_result".to_string(),
            content: content.into(),
            is_error: false,
            tool_use_id: tool_use_id.into(),
        }
    }

    pub fn error_result(error: &str, tool_use_id: &str) -> ToolResult {
        let message = ToolMessage::new_error(error, tool_use_id); 
        let stream = futures::stream::iter(vec![Message::Tool(message)]);
        Box::new(stream)
    }

    pub fn content_result(content: &str, tool_use_id: &str) -> ToolResult {
        let message = ToolMessage::new_content(content, tool_use_id);
        let stream = futures::stream::iter(vec![Message::Tool(message)]);
        Box::new(stream)
    }
}

#[derive(Clone)]
pub struct ToolUseContext {
    pub model: Option<String>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub agent_id: String,  // name-uuid
    pub abort_signal: bool, // Todo use proper abort signal
    pub tool_use_id: String,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn input_json_schema(&self) -> &str;
    fn description(&self) -> &str;
    async fn prompt(&self) -> String;
    //async fn call(&self, input_data: &serde_json::Value, context: &ToolUseContext) -> Result<ToolMessage>;
    async fn call(&self, input_data: &serde_json::Value, context: &ToolUseContext) -> ToolResult;
}