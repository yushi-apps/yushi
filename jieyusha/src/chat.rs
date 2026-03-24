//! Chat 模块 - 提供对话功能
//!
//! Session 管理由 App 层负责，chat 模块仅负责执行对话

use std::path::PathBuf;
use std::pin::Pin;
use std::time::Instant;
use std::collections::HashMap;
use log;
use tracing::instrument;
use crate::Registry;
use crate::Result;
use crate::ToolUseContext;
use crate::query::query;
use crate::messages::*;
use crate::memory;
use futures::stream::{Stream, StreamExt};

#[instrument(skip_all)]
pub async fn chat(user_input: &str, agent_id: &str) -> Result<String> {
    let root_path = Registry::instance().root_path();
    let root_path_buf = if root_path.is_empty() {
        None
    } else {
        Some(PathBuf::from(&root_path))
    };
    
    let tool_use_context = ToolUseContext {
        model: None,
        tools: Registry::instance().get_all_tools(),
        agent_id: agent_id.to_string(),
        abort_signal: false,
        tool_use_id: "".to_string(),
        root_path: root_path_buf.clone(),
    };

    log::info!("Agent({agent_id}) started");

    let user_message = Message::User(UserMessage::new(user_input)); 
    let start = Instant::now();
    let system_prompt = Registry::instance().get_system_prompt();
    
    // 初始化 memory 并创建 intent 差量
    if let Some(ref rp) = root_path_buf {
        if let Err(e) = memory::init(rp) {
            log::warn!("Failed to init memory: {}", e);
        }
        if let Err(e) = memory::create_intent_delta(rp, user_input) {
            log::error!("Failed to create intent delta: {}", e);
        }
    }
    
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

/// 流式聊天
/// 
/// # 参数
/// - `user_input`: 用户输入
/// - `root_path`: App::root_path() 返回的根目录
/// 
/// # 返回
/// 返回消息流
pub fn chat_stream(
    user_input: &str, 
    root_path: PathBuf,
) -> Pin<Box<dyn Stream<Item = Message> + Send>> {
    // 初始化 memory 系统
    if let Err(e) = memory::init(&root_path) {
        log::warn!("Failed to init memory: {}", e);
    }
    
    // 创建 intent 差量
    if let Err(e) = memory::create_intent_delta(&root_path, user_input) {
        log::error!("Failed to create intent delta: {}", e);
    }
    
    let agent_id = format!("yushi-{}", uuid::Uuid::new_v4());
    
    let tool_use_context = ToolUseContext {
        model: None,
        tools: Registry::instance().get_all_tools(),
        agent_id: agent_id.clone(),
        abort_signal: false,
        tool_use_id: "".to_string(),
        root_path: Some(root_path.clone()),
    };

    log::info!("Agent({}) started", agent_id);

    let user_message = Message::User(UserMessage::new(user_input));
    let system_prompt = Registry::instance().get_system_prompt();

    // 构建 context map
    let query_context = HashMap::new();

    query(
        vec![user_message],
        vec![system_prompt],
        tool_use_context,
        query_context,
    )
}
