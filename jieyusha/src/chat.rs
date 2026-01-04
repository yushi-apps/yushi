use std::time::Instant;
use std::collections::HashMap;
use log;
use tracing::instrument;
use crate::Registry;
use crate::Result;
use crate::ToolUseContext;
use crate::query::query;
use crate::messages::*;

#[instrument(skip_all)]
pub async fn chat(user_input: &str, session_id: &str) -> Result<String> {
    let mut tool_use_context = ToolUseContext {
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

    let assistant =  query(
        &vec![user_message],
        &vec![system_prompt],
        &mut tool_use_context,
        HashMap::new(),
    ).await?;
    
    log::info!("{}: Assistant Response: {}", tool_use_context.agent_id, assistant.content);
    Ok(assistant.content)
}