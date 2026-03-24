//! Memory 模块 - Agent 记忆结构与差量管理
//
//! 基于可逆计算原理，实现结构化的、可演化的上下文系统

use std::path::{Path, PathBuf};
use std::fs;

use uuid::Uuid;
use chrono::Utc;

use tuo::{YNode, YNodeMerge};

use crate::Registry;
use crate::SkillTool;
use crate::error::{JieyushaError, Result};
use crate::messages::{Message, UserMessage, AssistantMessage, ToolUse, ToolMessage};

// ============================================================================
// XML 工具函数
// ============================================================================

/// XML 转义
fn escape_xml(s: &str) -> String {
    s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&apos;")
}

/// 生成 ISO 8601 格式时间戳
fn timestamp() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

// ============================================================================
// 差量文件管理
// ============================================================================

/// 扫描 history 目录，返回按数字排序的文件列表
fn scan_history_files(history_dir: &Path) -> Vec<(u32, String)> {
    let mut files: Vec<(u32, String)> = Vec::new();
    
    if !history_dir.exists() {
        return files;
    }
    
    if let Ok(entries) = fs::read_dir(history_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // 只处理 N_xxx.xml 格式的文件
            if name.ends_with(".xml") && name != "current.xml" {
                if let Some(num_str) = name.split('_').next() {
                    if let Ok(num) = num_str.parse::<u32>() {
                        files.push((num, name));
                    }
                }
            }
        }
    }
    
    files.sort_by_key(|(num, _)| *num);
    files
}

/// 获取下一个序号
fn get_next_number(history_dir: &Path) -> u32 {
    let files = scan_history_files(history_dir);
    files.last().map(|(num, _)| num + 1).unwrap_or(0)
}

/// 获取最新文件名
fn get_latest_file(history_dir: &Path) -> Option<String> {
    let files = scan_history_files(history_dir);
    files.last().map(|(_, name)| name.clone())
}

// ============================================================================
// Memory 差量文件生成
// ============================================================================

/// 生成初始 workspace 文件（首次创建）
///
/// 包含完整的 system-prompt、available-skills、available-tools、available-agents
/// 只在没有 workspace 文件时调用
pub fn create_workspace_delta(root_path: &Path) -> Result<PathBuf> {
    let history_dir = root_path.join("history");
    fs::create_dir_all(&history_dir)?;
    
    // 检查是否已有 workspace 文件
    let files = scan_history_files(&history_dir);
    let has_workspace = files.iter().any(|(_, name)| name.contains("_workspace"));
    
    if has_workspace {
        // 已有 workspace，返回最新的
        for (_, name) in files.iter().rev() {
            if name.contains("_workspace") {
                return Ok(history_dir.join(name));
            }
        }
    }
    
    // 首次创建完整 workspace
    let yushi_md_path = root_path.join("YUSHI.md");
    let system_prompt = fs::read_to_string(&yushi_md_path).unwrap_or_default();
    
    let skills_dir = root_path.join("skills");
    let skills_xml = if skills_dir.exists() {
        SkillTool::load_skills(&skills_dir)
    } else {
        String::new()
    };
    
    let agents_dir = root_path.join("agents");
    let agents_xml = load_agents(&agents_dir);
    
    let tools_xml = generate_tools_xml();
    
    let id = Uuid::new_v4().to_string();
    let created_at = timestamp();
    
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
    xml.push_str(&format!(
        r#"<Memory id="{}" created-at="{}">
"#,
        id, created_at
    ));
    
    // system-prompt
    if !system_prompt.is_empty() {
        xml.push_str(&format!("    <system-prompt>{}</system-prompt>\n", escape_xml(&system_prompt)));
    }
    
    // available-skills
    if !skills_xml.is_empty() {
        xml.push_str(&format!("    {}\n", skills_xml));
    }
    
    // available-tools
    if !tools_xml.is_empty() {
        xml.push_str(&tools_xml);
    }
    
    // available-agents
    if !agents_xml.is_empty() {
        xml.push_str(&agents_xml);
    }
    
    xml.push_str("</Memory>\n");
    
    let path = history_dir.join("0_workspace.xml");
    fs::write(&path, xml)?;
    log::info!("Created initial workspace: {:?}", path);
    
    Ok(path)
}

/// 更新 workspace 配置（增量更新）
///
/// 当 tools/skills/agents 等配置变更时调用
/// 生成增量文件，只包含变更部分
pub fn update_workspace_delta(root_path: &Path, change_type: WorkspaceChange) -> Result<PathBuf> {
    let history_dir = root_path.join("history");
    fs::create_dir_all(&history_dir)?;
    
    // 获取最新的 workspace 文件作为继承目标
    let files = scan_history_files(&history_dir);
    let extends_file = files.iter()
        .filter(|(_, name)| name.contains("_workspace"))
        .last()
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| "0_workspace.xml".to_string());
    
    let next_num = get_next_number(&history_dir);
    
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
    xml.push_str(&format!(
        r#"<Memory xmlns:x="xdsl.xdef" x:extends="{}">
"#,
        extends_file
    ));
    
    // 根据变更类型生成增量内容
    match change_type {
        WorkspaceChange::ToolAdded { name, description } => {
            xml.push_str(&format!(
                r#"    <available-tools>
        <tool name="{}" description="{}" x:override="append" />
    </available-tools>
"#,
                escape_xml(&name),
                escape_xml(&description)
            ));
        }
        WorkspaceChange::SkillAdded { name, description } => {
            xml.push_str(&format!(
                r#"    <available-skills>
        <skill name="{}" description="{}" x:override="append" />
    </available-skills>
"#,
                escape_xml(&name),
                escape_xml(&description)
            ));
        }
        WorkspaceChange::AgentAdded { name, description } => {
            xml.push_str(&format!(
                r#"    <available-agents>
        <agent name="{}" description="{}" x:override="append" />
    </available-agents>
"#,
                escape_xml(&name),
                escape_xml(&description)
            ));
        }
        WorkspaceChange::SystemPromptUpdated { content } => {
            xml.push_str(&format!(
                "    <system-prompt>{}</system-prompt>\n",
                escape_xml(&content)
            ));
        }
    }
    
    xml.push_str("</Memory>\n");
    
    let filename = format!("{}_workspace.xml", next_num);
    let path = history_dir.join(&filename);
    
    fs::write(&path, xml)?;
    log::info!("Created workspace delta: {:?}", path);
    
    Ok(path)
}

/// Workspace 变更类型
pub enum WorkspaceChange {
    ToolAdded { name: String, description: String },
    SkillAdded { name: String, description: String },
    AgentAdded { name: String, description: String },
    SystemPromptUpdated { content: String },
}

/// 生成 intent 差量文件
///
/// 保存用户输入的意图
pub fn create_intent_delta(root_path: &Path, intent: &str) -> Result<PathBuf> {
    let history_dir = root_path.join("history");
    fs::create_dir_all(&history_dir)?;
    
    let next_num = get_next_number(&history_dir);
    let extends_file = get_latest_file(&history_dir)
        .unwrap_or_else(|| "0_workspace.xml".to_string());
    
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
    xml.push_str(&format!(
        r#"<Memory xmlns:x="xdsl.xdef" x:extends="{}">
"#,
        extends_file
    ));
    
    // user-intent
    xml.push_str(&format!("    <user-intent>{}</user-intent>\n", escape_xml(intent)));

    xml.push_str("</Memory>\n");
    
    let filename = format!("{}_intent.xml", next_num);
    let path = history_dir.join(&filename);
    
    fs::write(&path, xml)?;
    log::info!("Created intent delta: {:?}", path);
    
    Ok(path)
}

/// 生成 thought 差量文件
///
/// 保存 LLM 的思考内容
pub fn create_thought_delta(root_path: &Path, content: &str) -> Result<PathBuf> {
    let history_dir = root_path.join("history");
    fs::create_dir_all(&history_dir)?;
    
    let next_num = get_next_number(&history_dir);
    let extends_file = get_latest_file(&history_dir)
        .unwrap_or_else(|| "0_workspace.xml".to_string());
    
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
    xml.push_str(&format!(
        r#"<Memory xmlns:x="xdsl.xdef" x:extends="{}">
"#,
        extends_file
    ));
    
    // thought action（按设计方案格式）
    xml.push_str(&format!(
        r#"    <thought id="{}" content="{}" />
"#,
        next_num,
        escape_xml(content)
    ));
    
    xml.push_str("</Memory>\n");
    
    let filename = format!("{}_thought.xml", next_num);
    let path = history_dir.join(&filename);
    
    fs::write(&path, xml)?;
    log::info!("Created thought delta: {:?}", path);
    
    Ok(path)
}

/// 生成 toolcall 差量文件
///
/// 保存工具调用和结果（一个文件包含完整的 toolcall）
pub fn create_toolcall_delta(root_path: &Path, tool_use: &ToolUse, tool_message: &ToolMessage) -> Result<PathBuf> {
    let history_dir = root_path.join("history");
    fs::create_dir_all(&history_dir)?;
    
    let next_num = get_next_number(&history_dir);
    let extends_file = get_latest_file(&history_dir)
        .unwrap_or_else(|| "0_workspace.xml".to_string());
    
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
    xml.push_str(&format!(
        r#"<Memory xmlns:x="xdsl.xdef" x:extends="{}">
"#,
        extends_file
    ));
    
    // toolcall action（按设计方案格式）
    if tool_message.is_error {
        xml.push_str(&format!(
            r#"    <toolcall id="{}" tool-call-id="{}" name="{}" arguments="{}" status="failed" error="{}" />
"#,
            next_num,
            escape_xml(&tool_use.id),
            escape_xml(&tool_use.name),
            escape_xml(&tool_use.arguments),
            escape_xml(&tool_message.content)
        ));
    } else {
        xml.push_str(&format!(
            r#"    <toolcall id="{}" tool-call-id="{}" name="{}" arguments="{}" status="success" result="{}" />
"#,
            next_num,
            escape_xml(&tool_use.id),
            escape_xml(&tool_use.name),
            escape_xml(&tool_use.arguments),
            escape_xml(&tool_message.content)
        ));
    }
    
    xml.push_str("</Memory>\n");
    
    let filename = format!("{}_toolcall.xml", next_num);
    let path = history_dir.join(&filename);
    
    fs::write(&path, xml)?;
    log::info!("Created toolcall delta: {:?}", path);
    
    Ok(path)
}

/// 生成 summary 差量文件
///
/// 保存历史摘要
pub fn create_summary_delta(root_path: &Path, content: &str, begin_action: u32, end_action: u32) -> Result<PathBuf> {
    let history_dir = root_path.join("history");
    fs::create_dir_all(&history_dir)?;
    
    let next_num = get_next_number(&history_dir);
    let extends_file = get_latest_file(&history_dir)
        .unwrap_or_else(|| "0_workspace.xml".to_string());
    
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
    xml.push_str(&format!(
        r#"<Memory xmlns:x="xdsl.xdef" x:extends="{}">
"#,
        extends_file
    ));
    
    // summary action（按设计方案格式）
    xml.push_str(&format!(
        r#"    <summary id="{}" begin_action="{}" end_action="{}" content="{}" />
"#,
        next_num,
        begin_action,
        end_action,
        escape_xml(content)
    ));
    
    xml.push_str("</Memory>\n");
    
    let filename = format!("{}_summary.xml", next_num);
    let path = history_dir.join(&filename);
    
    fs::write(&path, xml)?;
    log::info!("Created summary delta: {:?}", path);
    
    Ok(path)
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 从 agents 目录加载 agents 信息
fn load_agents(agents_dir: &Path) -> String {
    if !agents_dir.exists() {
        return String::new();
    }
    
    let mut agents_xml = String::from("<available-agents>\n");
    
    if let Ok(entries) = fs::read_dir(agents_dir) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() && entry.path().extension().map(|e| e == "md").unwrap_or(false) {
                    let path = entry.path();
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok((metadata, _)) = crate::utils::parse_frontmatter(&content) {
                            if let (Some(name), Some(description)) = (
                                metadata.get("name").map(|s| s.as_str()),
                                metadata.get("description").map(|s| s.as_str()),
                            ) {
                                agents_xml.push_str(&format!(
                                    "    <agent name=\"{}\" description=\"{}\" />\n",
                                    escape_xml(name),
                                    escape_xml(description)
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    
    agents_xml.push_str("</available-agents>");
    agents_xml
}

/// 从 Registry 生成 tools XML
fn generate_tools_xml() -> String {
    let tools = Registry::instance().get_all_tools();
    if tools.is_empty() {
        return String::new();
    }
    
    let mut xml = String::from("<available-tools>\n");
    for tool in tools {
        // 获取实际的 JSON Schema 并转义特殊字符
        let schema = tool.input_json_schema();
        // 压缩 JSON（移除多余空白）
        let compact_schema: String = schema.chars()
            .filter(|c| !c.is_whitespace() || *c == ' ')
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join("");
        
        xml.push_str(&format!(
            "    <tool name=\"{}\" description=\"{}\" input-schema='{}' />\n",
            tool.name(),
            escape_xml(tool.description()),
            escape_xml(&compact_schema)
        ));
    }
    xml.push_str("</available-tools>\n");
    xml
}

/// 检查是否需要生成摘要
///
/// 当 history-actions 超过 10 条时触发
pub fn should_generate_summary(history_dir: &Path) -> bool {
    let files = scan_history_files(history_dir);
    // 只计算非 workspace 的 action 文件
    let action_count = files.iter()
        .filter(|(_, name)| !name.contains("_workspace"))
        .count();
    action_count > 10
}

/// 获取 history-actions 的 XML 内容（用于构建 LLM 请求）
///
/// 使用 XDSL 合并引擎解析历史文件，只提取 toolcall 和 summary（不含 thought）
pub fn get_history_actions_xml(root_path: &Path) -> Result<String> {
    let history_dir = root_path.join("history");
    let files = scan_history_files(&history_dir);
    
    if files.is_empty() {
        return Ok("<history-actions />".to_string());
    }
    
    // 获取最后一个文件，使用 process_extends 递归合并整条继承链
    let (_, last_file) = files.last().unwrap();
    let last_path = history_dir.join(last_file);
    
    let last_node = YNode::from_xml(last_path.to_str().unwrap())
        .map_err(|e| JieyushaError::ConfigError(format!("Failed to parse XML: {}", e)))?;
    
    let merged = last_node.process_extends(&history_dir)
        .map_err(|e| JieyushaError::ConfigError(format!("Failed to merge: {}", e)))?;
    
    // 提取 toolcall 和 summary 子节点（不含 thought）
    let mut history_actions = YNode::new("history-actions");
    for child in merged.children() {
        match child.tag() {
            "toolcall" | "summary" => {
                history_actions.add_child(child.clone());
            }
            _ => {}
        }
    }
    
    if history_actions.children().is_empty() {
        Ok("<history-actions />".to_string())
    } else {
        Ok(history_actions.to_xml())
    }
}

/// 获取完整的 Memory XML（合并所有差量）
///
/// 使用 XDSL 合并引擎，对最后一个历史文件调用 process_extends() 递归解析整条继承链
pub fn get_merged_memory_xml(root_path: &Path) -> Result<String> {
    let history_dir = root_path.join("history");
    let files = scan_history_files(&history_dir);
    
    if files.is_empty() {
        return Err(JieyushaError::ConfigError("No memory files found".to_string()));
    }
    
    // 取最后一个文件，调用 process_extends 递归合并整条继承链
    let (_, last_file) = files.last().unwrap();
    let last_path = history_dir.join(last_file);
    
    let last_node = YNode::from_xml(last_path.to_str().unwrap())
        .map_err(|e| JieyushaError::ConfigError(format!("Failed to parse XML: {}", e)))?;
    
    let merged = last_node.process_extends(&history_dir)
        .map_err(|e| JieyushaError::ConfigError(format!("Failed to merge: {}", e)))?;
    
    Ok(merged.to_xml())
}

/// 获取结构化 Memory XML（仅包含配置，不含历史动作）
///
/// 使用 XDSL 合并引擎，过滤掉 thought、toolcall、summary 子节点
/// 只保留：system-prompt、available-tools、available-skills、available-agents、user-intent、current-progress
pub fn get_structural_memory_xml(root_path: &Path) -> Result<String> {
    let history_dir = root_path.join("history");
    let files = scan_history_files(&history_dir);
    
    if files.is_empty() {
        return Err(JieyushaError::ConfigError("No memory files found".to_string()));
    }
    
    let (_, last_file) = files.last().unwrap();
    let last_path = history_dir.join(last_file);
    
    let last_node = YNode::from_xml(last_path.to_str().unwrap())
        .map_err(|e| JieyushaError::ConfigError(format!("Failed to parse XML: {}", e)))?;
    
    let merged = last_node.process_extends(&history_dir)
        .map_err(|e| JieyushaError::ConfigError(format!("Failed to merge: {}", e)))?;
    
    // 过滤掉 action 元素，只保留结构化配置
    let mut structural = YNode::new("Memory");
    // 复制根节点的属性
    for (key, value) in merged.attributes() {
        structural = structural.with_attr(key.clone(), value.value.clone());
    }
    for child in merged.children() {
        match child.tag() {
            "thought" | "toolcall" | "summary" => {
                // 跳过 action 元素
            }
            _ => {
                structural.add_child(child.clone());
            }
        }
    }
    
    Ok(structural.to_xml())
}

/// 初始化 Memory 系统
///
/// 创建初始 workspace 文件
pub fn init(root_path: &Path) -> Result<PathBuf> {
    create_workspace_delta(root_path)
}

/// 获取 current-progress XML 内容
///
/// 从最新的 workspace 文件中提取 current-progress
pub fn get_current_progress_xml(root_path: &Path) -> Result<String> {
    let history_dir = root_path.join("history");
    
    // 找到最新的 workspace 文件
    let files = scan_history_files(&history_dir);
    let workspace_file = files.iter()
        .filter(|(_, name)| name.contains("_workspace"))
        .last()
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| "0_workspace.xml".to_string());
    
    let workspace_path = history_dir.join(&workspace_file);
    let content = fs::read_to_string(&workspace_path)?;
    
    // 提取 current-progress 内容
    if let Some(start) = content.find("<current-progress>") {
        if let Some(end) = content.find("</current-progress>") {
            return Ok(content[start..end + 19].to_string());
        }
    }
    
    // 返回空的 current-progress
    Ok(String::new())
}

/// 保存 Schedule Agent 生成的进度规划
///
/// 生成 NN_summary.xml 文件，包含 current-progress 部分
pub fn save_schedule_result(root_path: &Path, progress_xml: &str) -> Result<PathBuf> {
    let history_dir = root_path.join("history");
    fs::create_dir_all(&history_dir)?;
    
    // 获取最新的 workspace 文件作为继承目标
    let files = scan_history_files(&history_dir);
    let extends_file = files.iter()
        .filter(|(_, name)| name.contains("_workspace"))
        .last()
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| "0_workspace.xml".to_string());
    
    let next_num = get_next_number(&history_dir);
    
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
    xml.push_str(&format!(
        r#"<Memory xmlns:x="xdsl.xdef" x:extends="{}">
"#,
        extends_file
    ));
    
    // current-progress
    xml.push_str(&format!("    {}\n", progress_xml));
    
    xml.push_str("</Memory>\n");
    
    // 使用 _summary.xml 命名
    let filename = format!("{}_summary.xml", next_num);
    let path = history_dir.join(&filename);
    
    fs::write(&path, xml)?;
    log::info!("Saved schedule result: {:?}", path);
    
    Ok(path)
}

/// 清理历史动作文件（生成摘要后调用）
///
/// 删除已被 Summary 覆盖的 thought 和 toolcall 差量文件
/// 保留 workspace、intent 和 summary 文件
pub fn clear_history_actions(_root_path: &Path) -> Result<()> {
    // 不再自动删除历史文件
    log::info!("History files preserved, no automatic cleanup");
    Ok(())
}

/// 从 history 差量文件动态重建符合 OpenAI API 的消息链
///
/// 按顺序解析 history 目录下的差量文件，将其转换为 Message 列表
pub fn build_messages_from_history(root_path: &Path) -> Result<Vec<Message>> {
    let history_dir = root_path.join("history");
    let files = scan_history_files(&history_dir);
    
    if files.is_empty() {
        return Ok(Vec::new());
    }
    
    // 解析所有文件，提取 (type, parsed_data)
    #[derive(Debug)]
    enum EventType {
        Workspace,
        Intent(String),           // content
        Thought(String),          // content
        Toolcall {
            tool_call_id: String,
            name: String,
            arguments: String,
            status: String,       // "success" or "failed"
            result_or_error: String,
        },
        Summary,
    }
    
    let mut events: Vec<EventType> = Vec::new();
    
    for (_, filename) in &files {
        let file_path = history_dir.join(filename);
        
        // 根据文件名提取类型
        let file_type = if filename.contains("_workspace") {
            "workspace"
        } else if filename.contains("_intent") {
            "intent"
        } else if filename.contains("_thought") {
            "thought"
        } else if filename.contains("_toolcall") {
            "toolcall"
        } else if filename.contains("_summary") {
            "summary"
        } else {
            continue;
        };
        
        // 读取并解析 XML 文件
        let node = match YNode::from_xml(file_path.to_str().unwrap()) {
            Ok(n) => n,
            Err(_) => continue,
        };
        
        match file_type {
            "workspace" => {
                events.push(EventType::Workspace);
            }
            "summary" => {
                events.push(EventType::Summary);
            }
            "intent" => {
                // 找 user-intent 子节点，提取文本内容
                for child in node.children() {
                    if child.tag() == "user-intent" {
                        let content = extract_text_content(child);
                        if !content.is_empty() {
                            events.push(EventType::Intent(content));
                        }
                        break;
                    }
                }
            }
            "thought" => {
                // 找 thought 子节点，提取 content 属性
                for child in node.children() {
                    if child.tag() == "thought" {
                        if let Some(content_attr) = child.attr("content") {
                            let content = content_attr.value.to_string();
                            events.push(EventType::Thought(content));
                        }
                        break;
                    }
                }
            }
            "toolcall" => {
                // 找 toolcall 子节点，提取所有属性
                for child in node.children() {
                    if child.tag() == "toolcall" {
                        let id_attr = child.attr("id")
                            .map(|v| v.value.to_string())
                            .unwrap_or_default();
                        
                        // 兼容旧文件：如果没有 tool-call-id，使用合成 ID
                        let tool_call_id = child.attr("tool-call-id")
                            .map(|v| v.value.to_string())
                            .unwrap_or_else(|| format!("call_{}", id_attr));
                        
                        let name = child.attr("name")
                            .map(|v| v.value.to_string())
                            .unwrap_or_default();
                        
                        let arguments = child.attr("arguments")
                            .map(|v| v.value.to_string())
                            .unwrap_or_default();
                        
                        let status = child.attr("status")
                            .map(|v| v.value.to_string())
                            .unwrap_or_else(|| "success".to_string());
                        
                        let result_or_error = if status == "failed" {
                            child.attr("error")
                                .map(|v| v.value.to_string())
                                .unwrap_or_default()
                        } else {
                            child.attr("result")
                                .map(|v| v.value.to_string())
                                .unwrap_or_default()
                        };
                        
                        events.push(EventType::Toolcall {
                            tool_call_id,
                            name,
                            arguments,
                            status,
                            result_or_error,
                        });
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    
    // 按逻辑构建消息
    let mut messages: Vec<Message> = Vec::new();
    let mut i = 0;
    
    while i < events.len() {
        match &events[i] {
            EventType::Workspace | EventType::Summary => {
                // skip
                i += 1;
            }
            EventType::Intent(content) => {
                messages.push(Message::User(UserMessage::new(content.clone())));
                i += 1;
            }
            EventType::Thought(content) => {
                // 前瞻：收集紧随的 toolcall
                let mut toolcalls: Vec<&EventType> = Vec::new();
                let mut j = i + 1;
                while j < events.len() {
                    if let EventType::Toolcall { .. } = &events[j] {
                        toolcalls.push(&events[j]);
                        j += 1;
                    } else {
                        break;
                    }
                }
                
                if toolcalls.is_empty() {
                    // 纯文本回复（最终答案）
                    messages.push(Message::Assistant(AssistantMessage::new(content.clone())));
                } else {
                    // 含工具调用的回复
                    let tool_uses: Vec<ToolUse> = toolcalls.iter().filter_map(|tc| {
                        if let EventType::Toolcall { tool_call_id, name, arguments, .. } = tc {
                            Some(ToolUse {
                                id: tool_call_id.clone(),
                                name: name.clone(),
                                arguments: arguments.clone(),
                            })
                        } else {
                            None
                        }
                    }).collect();
                    
                    let mut assistant_msg = AssistantMessage::new(content.clone());
                    assistant_msg.tool_uses = Some(tool_uses);
                    messages.push(Message::Assistant(assistant_msg));
                    
                    // 每个 toolcall 生成对应的 Tool 消息
                    for tc in toolcalls {
                        if let EventType::Toolcall { tool_call_id, status, result_or_error, .. } = tc {
                            let tool_msg = if status == "failed" {
                                ToolMessage::from_error(result_or_error.clone(), tool_call_id.clone())
                            } else {
                                ToolMessage::new_content(result_or_error.clone(), tool_call_id.clone())
                            };
                            messages.push(Message::Tool(tool_msg));
                        }
                    }
                }
                
                i = j; // 跳过已消费的 toolcall
            }
            EventType::Toolcall { tool_call_id, name, arguments, status, result_or_error } => {
                // 单独出现的 toolcall（无前导 thought），作为独立处理
                // 生成空 content 的 assistant + tool
                let tool_use = ToolUse {
                    id: tool_call_id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                };
                let mut assistant_msg = AssistantMessage::new("");
                assistant_msg.tool_uses = Some(vec![tool_use]);
                messages.push(Message::Assistant(assistant_msg));
                
                let tool_msg = if status == "failed" {
                    ToolMessage::from_error(result_or_error.clone(), tool_call_id.clone())
                } else {
                    ToolMessage::new_content(result_or_error.clone(), tool_call_id.clone())
                };
                messages.push(Message::Tool(tool_msg));
                
                i += 1;
            }
        }
    }
    
    Ok(messages)
}

/// 从 YNode 中提取文本内容（处理 TEXT 子节点）
fn extract_text_content(node: &YNode) -> String {
    let mut content = String::new();
    
    // 如果节点本身有内容
    if let Some(c) = node.content_str() {
        content.push_str(c);
    }
    
    // 遍历子节点，提取 TEXT 节点的内容
    for child in node.children() {
        if child.is_text_node() {
            if let Some(c) = child.content_str() {
                content.push_str(c);
            }
        }
    }
    
    content.trim().to_string()
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_init_memory() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();
        
        // 创建 YUSHI.md
        fs::write(root_path.join("YUSHI.md"), "test prompt").unwrap();
        
        // 初始化
        let result = init(root_path);
        assert!(result.is_ok());
        
        // 检查文件是否存在
        let history_dir = root_path.join("history");
        assert!(history_dir.exists());
        
        let workspace_path = history_dir.join("0_workspace.xml");
        assert!(workspace_path.exists());
    }
    
    #[test]
    fn test_create_intent_delta() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();
        
        // 先创建 workspace
        fs::write(root_path.join("YUSHI.md"), "test").unwrap();
        init(root_path).unwrap();
        
        // 创建 intent
        let result = create_intent_delta(root_path, "测试意图");
        assert!(result.is_ok());
        
        // 检查文件
        let history_dir = root_path.join("history");
        let intent_path = history_dir.join("1_intent.xml");
        assert!(intent_path.exists());
    }
    
    #[test]
    fn test_create_toolcall_delta() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();
        
        fs::write(root_path.join("YUSHI.md"), "test").unwrap();
        init(root_path).unwrap();
        
        let tool_use = ToolUse {
            id: "call-001".to_string(),
            name: "Bash".to_string(),
            arguments: r#"{"cmd": "ls"}"#.to_string(),
        };
        
        let tool_message = ToolMessage::new_content("file1\nfile2", "call-001");
        
        let result = create_toolcall_delta(root_path, &tool_use, &tool_message);
        assert!(result.is_ok());
        
        let history_dir = root_path.join("history");
        let toolcall_path = history_dir.join("1_toolcall.xml");
        assert!(toolcall_path.exists());
    }
    
    #[test]
    fn test_should_generate_summary() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();
        
        fs::write(root_path.join("YUSHI.md"), "test").unwrap();
        init(root_path).unwrap();
        
        let history_dir = root_path.join("history");
        
        // 添加 11 个 action（超过 10 个触发摘要）
        for i in 1..=11 {
            create_intent_delta(root_path, &format!("intent {}", i)).unwrap();
        }
        
        // 应该触发摘要（超过 10 个）
        assert!(should_generate_summary(&history_dir));
    }
    
    #[test]
    fn test_thought_format() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();
        
        fs::write(root_path.join("YUSHI.md"), "test").unwrap();
        init(root_path).unwrap();
        
        create_thought_delta(root_path, "今天温度是25摄氏度").unwrap();
        
        let history_dir = root_path.join("history");
        let thought_path = history_dir.join("1_thought.xml");
        assert!(thought_path.exists());
        
        let content = fs::read_to_string(&thought_path).unwrap();
        // 验证格式： <thought id="1" content="..." />
        assert!(content.contains("<thought id=\"1\" content=\""));
        assert!(content.contains("今天温度是25摄氏度"));
    }
    
    #[test]
    fn test_toolcall_format() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();
        
        fs::write(root_path.join("YUSHI.md"), "test").unwrap();
        init(root_path).unwrap();
        
        let tool_use = ToolUse {
            id: "call-001".to_string(),
            name: "Bash".to_string(),
            arguments: r#"{\"cmd\": \"ls -al\"}"#.to_string(),
        };
        
        // 测试成功场景
        let tool_message = ToolMessage::new_content("a.pdf\nb.pdf", "call-001");
        create_toolcall_delta(root_path, &tool_use, &tool_message).unwrap();
        
        let history_dir = root_path.join("history");
        let toolcall_path = history_dir.join("1_toolcall.xml");
        let content = fs::read_to_string(&toolcall_path).unwrap();
        
        // 验证格式：<toolcall id="1" tool-call-id="call-001" name="Bash" arguments="..." status="success" result="..." />
        assert!(content.contains("<toolcall id=\"1\" tool-call-id=\"call-001\" name=\"Bash\""));
        assert!(content.contains("status=\"success\""));
        assert!(content.contains("result=\""));
    }
    
    #[test]
    fn test_toolcall_error_format() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();
        
        fs::write(root_path.join("YUSHI.md"), "test").unwrap();
        init(root_path).unwrap();
        
        let tool_use = ToolUse {
            id: "call-002".to_string(),
            name: "Bash".to_string(),
            arguments: r#"{\"cmd\": \"cat test.md\"}"#.to_string(),
        };
        
        // 测试失败场景
        let mut tool_message = ToolMessage::new_content("文件不存在", "call-002");
        tool_message.is_error = true;
        create_toolcall_delta(root_path, &tool_use, &tool_message).unwrap();
        
        let history_dir = root_path.join("history");
        let toolcall_path = history_dir.join("1_toolcall.xml");
        let content = fs::read_to_string(&toolcall_path).unwrap();
        
        // 验证格式：<toolcall id="1" tool-call-id="call-002" name="Bash" arguments="..." status="failed" error="..." />
        assert!(content.contains("tool-call-id=\"call-002\""));
        assert!(content.contains("status=\"failed\""));
        assert!(content.contains("error=\""));
    }
}
