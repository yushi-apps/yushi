use std::time::Instant;
use std::collections::HashMap;
use log;
use tracing::instrument;
use crate::Registry;
use crate::Result;
use crate::ToolUseContext;
use crate::query::query;
use crate::messages::*;
use futures::stream::{Stream, StreamExt};

#[instrument(skip_all)]
pub async fn chat(user_input: &str, session_id: &str) -> Result<String> {
    let tool_use_context = ToolUseContext {
        model: None, 
        tools: Registry::instance().get_all_tools(),
        agent_id: format!("main-{}", session_id),
        abort_signal: false,
        tool_use_id: "".to_string(),
    };

    log::info!("Session({session_id}) started");

    let user_message = Message::User(UserMessage::new(user_input)); 
    let start = Instant::now();
    let system_prompt = Registry::instance().get_system_prompt();
    let elapsed = start.elapsed();
    log::info!("System Prompt Build ({}ms)", elapsed.as_millis());
    log::info!("{}: Processing query {:?}", tool_use_context.agent_id, user_input);

    let agent_id = tool_use_context.agent_id.clone();
    let mut response_stream = query(
        vec![user_message],
        vec![system_prompt],
        tool_use_context,
        HashMap::new(),
    );

    let mut full_response = String::new();
    while let Some(message) = response_stream.next().await {
        if let Message::Assistant(assistant_msg) = message {
            full_response.push_str(&assistant_msg.content);
        }
    }

    log::info!("{}: Assistant Response: {}", agent_id, full_response);
    Ok(full_response)
}

pub fn chat_stream(user_input: &str, session_id: String) -> impl Stream<Item = Message> {
    let tool_use_context = ToolUseContext {
        model: None,
        tools: Registry::instance().get_all_tools(),
        agent_id: format!("main-{}", session_id),
        abort_signal: false,
        tool_use_id: "".to_string(),
    };

    log::info!("Session({session_id}) started");

    let user_message = Message::User(UserMessage::new(user_input));
    let system_prompt = Registry::instance().get_system_prompt();

    query(
        vec![user_message],
        vec![system_prompt],
        tool_use_context,
        HashMap::new(),
    )
}