//! Memory 模块 - Agent 记忆结构定义
//!
//! 对应 memory.xdef 定义的 Agent 记忆结构

use std::path::Path;
use serde::{Deserialize, Serialize};
use tuo::OverrideRule;
use uuid::Uuid;

use crate::error::Result;

// ============================================================================
// Action 相关定义
// ============================================================================

/// Action 唯一标识类型
pub type ActionId = String;

/// Action 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    #[serde(rename = "thought")]
    Thought,
    #[serde(rename = "tool-call")]
    ToolCall,
    #[serde(rename = "tool-result")]
    ToolResult,
    #[serde(rename = "system-message")]
    SystemMessage,
    #[serde(rename = "historical-summary")]
    HistoricalSummary,
}

impl ActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionType::Thought => "thought",
            ActionType::ToolCall => "tool-call",
            ActionType::ToolResult => "tool-result",
            ActionType::SystemMessage => "system-message",
            ActionType::HistoricalSummary => "historical-summary",
        }
    }
}

impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Action 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionStatus {
    #[serde(rename = "OK")]
    Ok,
    #[serde(rename = "ERROR")]
    Error,
}

impl Default for ActionStatus {
    fn default() -> Self {
        ActionStatus::Ok
    }
}

/// Action 执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "result")]
pub struct ActionResult {
    #[serde(rename = "@is-summary", default)]
    pub is_summary: bool,
    #[serde(rename = "@status")]
    pub status: ActionStatus,
    #[serde(rename = "@output", default)]
    pub output: String,
    #[serde(rename = "@error", skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(rename = "raw-content", skip_serializing_if = "Option::is_none")]
    pub raw_content: Option<String>,
}

impl ActionResult {
    pub fn ok(output: impl Into<String>) -> Self {
        ActionResult {
            is_summary: false,
            status: ActionStatus::Ok,
            output: output.into(),
            error: None,
            raw_content: None,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        ActionResult {
            is_summary: false,
            status: ActionStatus::Error,
            output: String::new(),
            error: Some(error.into()),
            raw_content: None,
        }
    }

    pub fn summary_with_raw(output: impl Into<String>, raw_content: impl Into<String>) -> Self {
        ActionResult {
            is_summary: true,
            status: ActionStatus::Ok,
            output: output.into(),
            error: None,
            raw_content: Some(raw_content.into()),
        }
    }
}

/// Action 结构定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "action")]
pub struct Action {
    #[serde(rename = "@id")]
    pub id: ActionId,
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@type")]
    pub action_type: ActionType,
    #[serde(rename = "@x:override", skip_serializing_if = "Option::is_none")]
    pub override_rule: Option<String>,
    #[serde(rename = "arguments", skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ActionResult>,
}

impl Action {
    pub fn new(id: impl Into<String>, name: impl Into<String>, action_type: ActionType) -> Self {
        Action {
            id: id.into(),
            name: name.into(),
            action_type,
            override_rule: None,
            arguments: None,
            result: None,
        }
    }

    pub fn thought(id: impl Into<String>, content: impl Into<String>) -> Self {
        let mut action = Action::new(id, "thought", ActionType::Thought);
        action.result = Some(ActionResult::ok(content));
        action
    }

    pub fn tool(
        id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
        result: ActionResult,
    ) -> Self {
        let mut action = Action::new(id, tool_name, ActionType::ToolCall);
        action.arguments = Some(arguments);
        action.result = Some(result);
        action
    }

    pub fn historical_summary(id: impl Into<String>, content: impl Into<String>, summarized_count: usize) -> Self {
        let content_str = content.into();
        let mut action = Action::new(id, "_historical_summary", ActionType::HistoricalSummary);
        action.arguments = Some(serde_json::json!({
            "summarized_count": summarized_count,
            "summary_method": "llm_summarization"
        }));
        action.result = Some(ActionResult::summary_with_raw(&content_str, &content_str));
        action
    }

    pub fn with_override(mut self, rule: impl Into<String>) -> Self {
        self.override_rule = Some(rule.into());
        self
    }

    pub fn get_override_rule(&self) -> OverrideRule {
        self.override_rule
            .as_deref()
            .map(OverrideRule::from_str)
            .unwrap_or(OverrideRule::Merge)
    }
}

// ============================================================================
// ActionList 包装器
// ============================================================================

/// Action 列表包装器
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionList {
    #[serde(default, rename = "action")]
    actions: Vec<Action>,
}

impl ActionList {
    pub fn new() -> Self {
        ActionList { actions: Vec::new() }
    }

    pub fn push(&mut self, action: Action) {
        self.actions.push(action);
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Action> {
        self.actions.iter()
    }

    pub fn last(&self) -> Option<&Action> {
        self.actions.last()
    }

    pub fn clear(&mut self) {
        self.actions.clear();
    }

    pub fn to_vec(&self) -> Vec<Action> {
        self.actions.clone()
    }

    pub fn drain_front(&mut self, n: usize) {
        self.actions.drain(0..n.min(self.actions.len()));
    }
}

impl std::ops::Deref for ActionList {
    type Target = Vec<Action>;
    fn deref(&self) -> &Self::Target {
        &self.actions
    }
}

impl std::ops::DerefMut for ActionList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.actions
    }
}

impl IntoIterator for ActionList {
    type Item = Action;
    type IntoIter = std::vec::IntoIter<Action>;
    fn into_iter(self) -> Self::IntoIter {
        self.actions.into_iter()
    }
}

impl<'a> IntoIterator for &'a ActionList {
    type Item = &'a Action;
    type IntoIter = std::slice::Iter<'a, Action>;
    fn into_iter(self) -> Self::IntoIter {
        self.actions.iter()
    }
}

// ============================================================================
// Workspace 定义
// ============================================================================

/// 文件条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl FileEntry {
    pub fn new(name: impl Into<String>) -> Self {
        FileEntry { name: name.into(), description: None }
    }
}

/// 工作空间
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(default, rename = "file", skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileEntry>,
}

impl Workspace {
    pub fn new() -> Self {
        Workspace { files: Vec::new() }
    }
}

// ============================================================================
// Tool/Skill 定义
// ============================================================================

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "tool")]
pub struct ToolDef {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ToolDef {
    pub fn with_description(name: impl Into<String>, description: impl Into<String>) -> Self {
        ToolDef { name: name.into(), description: Some(description.into()) }
    }
}

/// 工具列表
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tools {
    #[serde(rename = "tool", default)]
    pub items: Vec<ToolDef>,
}

impl Tools {
    pub fn new() -> Self {
        Tools { items: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl From<Vec<ToolDef>> for Tools {
    fn from(items: Vec<ToolDef>) -> Self {
        Tools { items }
    }
}

/// 技能定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "skill")]
pub struct SkillDef {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl SkillDef {
    pub fn with_description(name: impl Into<String>, description: impl Into<String>) -> Self {
        SkillDef { name: name.into(), description: Some(description.into()) }
    }
}

/// 技能列表
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Skills {
    #[serde(rename = "skill", default)]
    pub items: Vec<SkillDef>,
}

impl Skills {
    pub fn new() -> Self {
        Skills { items: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl From<Vec<SkillDef>> for Skills {
    fn from(items: Vec<SkillDef>) -> Self {
        Skills { items }
    }
}

// ============================================================================
// Memory 定义
// ============================================================================

/// Memory 唯一标识类型
pub type MemoryId = String;

/// Memory 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MemoryStatus {
    #[serde(alias = "Success")]
    Success,
    #[serde(alias = "Pending")]
    Pending,
    #[serde(alias = "Failed")]
    Failed,
}

impl Default for MemoryStatus {
    fn default() -> Self {
        MemoryStatus::Pending
    }
}

impl std::fmt::Display for MemoryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryStatus::Success => write!(f, "SUCCESS"),
            MemoryStatus::Pending => write!(f, "PENDING"),
            MemoryStatus::Failed => write!(f, "FAILED"),
        }
    }
}

/// Memory 结构定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "Memory")]
pub struct Memory {
    #[serde(rename = "@id")]
    pub id: MemoryId,
    #[serde(rename = "@version", default)]
    pub version: u32,
    #[serde(rename = "@created-at")]
    pub created_at: String,
    #[serde(rename = "@status", default)]
    pub status: MemoryStatus,
    #[serde(rename = "@xmlns:x", skip_serializing_if = "Option::is_none")]
    pub xmlns_x: Option<String>,
    #[serde(rename = "@x:extends", skip_serializing_if = "Option::is_none")]
    pub x_extends: Option<String>,
    #[serde(rename = "intent")]
    pub intent: String,
    #[serde(rename = "system-prompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub workspace: Workspace,
    #[serde(default, skip_serializing_if = "Tools::is_empty")]
    pub tools: Tools,
    #[serde(default, skip_serializing_if = "Skills::is_empty")]
    pub skills: Skills,
    #[serde(default)]
    pub current: ActionList,
    #[serde(default)]
    pub history: ActionList,
}

impl Memory {
    pub fn new(intent: impl Into<String>) -> Self {
        Memory {
            id: Uuid::new_v4().to_string(),
            version: 1,
            created_at: chrono_timestamp(),
            status: MemoryStatus::Pending,
            xmlns_x: None,
            x_extends: None,
            intent: intent.into(),
            system_prompt: None,
            workspace: Workspace::new(),
            tools: Tools::new(),
            skills: Skills::new(),
            current: ActionList::new(),
            history: ActionList::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let memory: Memory = quick_xml::de::from_str(&content)
            .map_err(|e| crate::error::JieyushaError::ParseError(e.to_string()))?;
        Ok(memory)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let content = quick_xml::se::to_string(self)
            .map_err(|e| crate::error::JieyushaError::ParseError(e.to_string()))?;
        let xml_content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
{}
"#, content);
        std::fs::write(path, xml_content)?;
        Ok(())
    }

    pub fn add_action(&mut self, action: Action) {
        self.history.push(action.clone());
        self.current.push(action);
        if self.current.len() > 10 {
            self.current.drain_front(self.current.len() - 10);
        }
        self.version += 1;
    }

    pub fn set_status(&mut self, status: MemoryStatus) {
        self.status = status;
        self.version += 1;
    }

    pub fn mark_success(&mut self) {
        self.status = MemoryStatus::Success;
        self.version += 1;
    }

    pub fn mark_failed(&mut self) {
        self.status = MemoryStatus::Failed;
        self.version += 1;
    }

    /// 从基础文件和差量文件链加载
    pub fn load_with_deltas(
        base_path: &Path,
        delta_paths: &[std::path::PathBuf],
    ) -> Result<Self> {
        if delta_paths.is_empty() {
            return Self::load(base_path);
        }
        // TODO: 实现基于 tuo/xdsl 的链式继承加载
        // 暂时只加载 base
        Self::load(base_path)
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

fn chrono_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    format!("{}Z", chrono_like_format(secs))
}

fn chrono_like_format(secs: u64) -> String {
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;
    let (year, month, day) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", year, month, day, hours, minutes, seconds)
}

fn days_to_ymd(days: u64) -> (i32, u32, u32) {
    let mut year = 1970i32;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }
    let (month, day) = days_to_md(remaining as u32, is_leap_year(year));
    (year, month, day)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_to_md(days: u32, leap: bool) -> (u32, u32) {
    let month_days = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut remaining = days + 1;
    for (i, &days_in_month) in month_days.iter().enumerate() {
        if remaining <= days_in_month {
            return ((i + 1) as u32, remaining);
        }
        remaining -= days_in_month;
    }
    (12, 31)
}
