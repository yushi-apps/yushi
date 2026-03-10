use std::sync::Arc;
use std::pin::Pin;
use futures::Stream;
use async_trait::async_trait;
use crate::messages::{Message, AssistantMessage, ToolMessage, ProgressMessage};

pub struct ToolResult {
    pub stream: Pin<Box<dyn Stream<Item = Message> + Send>>,
    /// 是否需要持久化结果到文件
    pub requires_persistence: bool,
}

impl ToolResult {
    pub fn new(stream: Pin<Box<dyn Stream<Item = Message> + Send>>) -> Self {
        Self { stream, requires_persistence: false }
    }

    /// 创建需要持久化的结果
    pub fn persistent(stream: Pin<Box<dyn Stream<Item = Message> + Send>>) -> Self {
        Self { stream, requires_persistence: true }
    }

    pub fn error(error: impl Into<String>, tool_use_id: impl Into<String>) -> Self {
        let message = ToolMessage::from_error(error, tool_use_id); 
        let stream = futures::stream::iter(vec![Message::Tool(message)]);
        Self::new(Box::pin(stream))
    }

    pub fn progress(content: &str, tool_use_id: &str) -> ToolResult {
        let message =    ProgressMessage {
            r#type: "progress".to_string(),
            content: AssistantMessage::new(content),
            tools: None,
            tool_use_id: Some(tool_use_id.to_string()),
        };
        println!("new progress message");
        let stream = futures::stream::iter(vec![Message::Progress(message)]);
        Self::new(Box::pin(stream))
    }

    pub fn result(content: &str, tool_use_id: &str) -> ToolResult {
        let message = ToolMessage::new_content(content, tool_use_id);
        let stream = futures::stream::iter(vec![Message::Tool(message)]);
        Self::new(Box::pin(stream))
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
    async fn call(&self, input_data: &serde_json::Value, context: &ToolUseContext) -> ToolResult;
}