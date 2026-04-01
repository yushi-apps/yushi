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

/// 自动化任务执行上下文
/// 
/// 用于传递自动化 session 的执行信息
pub struct AutomationContext {
    /// 规则名称
    pub rule_name: String,
    /// 规则描述
    pub rule_description: Option<String>,
    /// 触发来源 (clock / event / manual)
    pub trigger_source: String,
}

/// 自动化任务专用流式聊天
/// 
/// 与普通 chat_stream 的区别：
/// 1. 过滤掉 Rule 工具，防止 LLM 修改规则配置
/// 2. 使用执行模式的 system prompt，明确告知 LLM "直接执行"
/// 3. 注入执行上下文（规则名称、描述、触发来源）
/// 
/// # 参数
/// - `user_input`: 待执行的任务内容（规则的 message 字段）
/// - `root_path`: App::root_path() 返回的根目录
/// - `automation_ctx`: 自动化执行上下文
/// 
/// # 返回
/// 返回消息流
pub fn chat_stream_automation(
    user_input: &str, 
    root_path: PathBuf,
    automation_ctx: AutomationContext,
) -> Pin<Box<dyn Stream<Item = Message> + Send>> {
    // 初始化 memory 系统
    if let Err(e) = memory::init(&root_path) {
        log::warn!("Failed to init memory: {}", e);
    }
    
    // 创建 intent 差量
    if let Err(e) = memory::create_intent_delta(&root_path, user_input) {
        log::error!("Failed to create intent delta: {}", e);
    }
    
    let agent_id = format!("automation-{}", uuid::Uuid::new_v4());
    
    // 过滤掉 Rule 工具，防止 LLM 修改规则配置
    let all_tools = Registry::instance().get_all_tools();
    let filtered_tools: Vec<_> = all_tools
        .into_iter()
        .filter(|tool| tool.name() != "Rule")
        .collect();
    
    let tool_use_context = ToolUseContext {
        model: None,
        tools: filtered_tools,
        agent_id: agent_id.clone(),
        abort_signal: false,
        tool_use_id: "".to_string(),
        root_path: Some(root_path.clone()),
    };

    log::info!("Automation Agent({}) started for rule: {}", agent_id, automation_ctx.rule_name);

    let user_message = Message::User(UserMessage::new(user_input));
    
    // 构建执行模式的 system prompt
    let base_prompt = Registry::instance().get_system_prompt();
    let execution_prompt = build_execution_mode_prompt(&automation_ctx, user_input);
    let system_prompt = format!("{}\n\n{}", base_prompt, execution_prompt);

    // 构建 context map
    let query_context = HashMap::new();

    query(
        vec![user_message],
        vec![system_prompt],
        tool_use_context,
        query_context,
    )
}

/// 构建执行模式的 System Prompt
/// 
/// 根据 message 类型（脚本型/意图型）生成不同的执行指导
fn build_execution_mode_prompt(ctx: &AutomationContext, message: &str) -> String {
    // 检测 message 类型
    let is_script = is_script_message(message);
    
    let mut prompt = String::new();
    
    prompt.push_str("# 自动化任务执行器\n\n");
    prompt.push_str("你是一个自动化任务执行器，负责执行预定义的任务。\n\n");
    
    // 执行上下文
    prompt.push_str("## 执行上下文\n\n");
    prompt.push_str(&format!("- 规则名称：{}\n", ctx.rule_name));
    if let Some(ref desc) = ctx.rule_description {
        prompt.push_str(&format!("- 规则描述：{}\n", desc));
    }
    prompt.push_str(&format!("- 触发来源：{}\n\n", ctx.trigger_source));
    
    // 执行要求
    prompt.push_str("## 执行要求\n\n");
    prompt.push_str("1. **直接执行**：理解任务内容后立即执行，不要询问\"你想让我做什么\"\n");
    prompt.push_str("2. **工具调用**：使用可用工具完成任务\n");
    prompt.push_str("3. **禁止配置**：不要创建、修改或删除自动化规则\n");
    prompt.push_str("4. **简洁汇报**：执行完成后简要说明结果\n\n");
    
    // 根据类型添加具体指导
    if is_script {
        prompt.push_str("## 任务类型：脚本执行\n\n");
        prompt.push_str("以下是待执行的脚本/命令序列，请按要求执行：\n");
        prompt.push_str("1. 按脚本内容逐步执行\n");
        prompt.push_str("2. 如需环境变量或参数，使用合理的默认值\n");
        prompt.push_str("3. 执行结果按脚本输出的格式呈现\n");
    } else {
        prompt.push_str("## 任务类型：意图执行\n\n");
        prompt.push_str("以下是任务目标描述，请理解后执行：\n");
        prompt.push_str("1. 分析任务目标\n");
        prompt.push_str("2. 规划执行步骤\n");
        prompt.push_str("3. 调用工具完成各步骤\n");
        prompt.push_str("4. 汇报执行结果\n");
    }
    
    prompt
}

/// 检测 message 是否为脚本类型
/// 
/// 脚本类型特征：
/// - 以 shebang 开头 (#!/bin/bash, #!/usr/bin/env 等)
/// - 明显的代码结构（大量命令行、代码关键字）
fn is_script_message(message: &str) -> bool {
    let trimmed = message.trim();
    
    // 检测 shebang
    if trimmed.starts_with("#!") {
        return true;
    }
    
    // 检测常见的脚本特征
    let script_indicators = [
        "#!/bin/",
        "#!/usr/bin/",
        "$ ",  // shell 提示符
        "curl ",
        "wget ",
        "echo ",
        "if [ ",
        "for ",
        "while ",
        "exit ",
        "function ",
        "() {",
        "```bash",
        "```sh",
        "```python",
        "```javascript",
    ];
    
    for indicator in script_indicators {
        if trimmed.contains(indicator) {
            // 排除假阳性：包含脚本但主要是描述性文字
            let lines: Vec<&str> = trimmed.lines().collect();
            let indicator_lines = lines.iter().filter(|line| line.contains(indicator)).count();
            if indicator_lines > lines.len() / 3 {
                return true;
            }
        }
    }
    
    false
}
