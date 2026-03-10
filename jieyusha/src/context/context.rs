//! Memory 结构定义
//! 
//! 对应 memory.xdef 定义的 Agent 记忆结构

use std::path::Path;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::action::{Action, ActionType};
use super::workspace::Workspace;
use crate::error::Result;

/// Memory 唯一标识类型
pub type MemoryId = String;

/// AgentContext 唯一标识类型 - 兼容旧命名
pub type ContextId = MemoryId;

/// Memory 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MemoryStatus {
    /// 成功完成
    #[serde(alias = "Success")]
    Success,
    /// 进行中
    #[serde(alias = "Pending")]
    Pending,
    /// 失败
    #[serde(alias = "Failed")]
    Failed,
}

/// AgentContext 状态 - 兼容旧命名
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ContextStatus {
    /// 成功完成
    #[serde(alias = "Success")]
    Success,
    /// 进行中
    #[serde(alias = "Pending")]
    Pending,
    /// 失败
    #[serde(alias = "Failed")]
    Failed,
}

impl Default for MemoryStatus {
    fn default() -> Self {
        MemoryStatus::Pending
    }
}

impl Default for ContextStatus {
    fn default() -> Self {
        ContextStatus::Pending
    }
}

impl From<MemoryStatus> for ContextStatus {
    fn from(status: MemoryStatus) -> Self {
        match status {
            MemoryStatus::Success => ContextStatus::Success,
            MemoryStatus::Pending => ContextStatus::Pending,
            MemoryStatus::Failed => ContextStatus::Failed,
        }
    }
}

impl From<ContextStatus> for MemoryStatus {
    fn from(status: ContextStatus) -> Self {
        match status {
            ContextStatus::Success => MemoryStatus::Success,
            ContextStatus::Pending => MemoryStatus::Pending,
            ContextStatus::Failed => MemoryStatus::Failed,
        }
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

impl std::fmt::Display for ContextStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextStatus::Success => write!(f, "SUCCESS"),
            ContextStatus::Pending => write!(f, "PENDING"),
            ContextStatus::Failed => write!(f, "FAILED"),
        }
    }
}

/// Action 列表包装器
/// 用于正确序列化为 <current><action/><action/></current> 格式
/// 
/// 注意：使用命名字段而非tuple struct，以便quick-xml正确序列化
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionList {
    /// xdsl合并规则
    #[serde(rename = "@x:override", skip_serializing_if = "Option::is_none")]
    pub x_override: Option<String>,
    #[serde(default, rename = "action")]
    actions: Vec<Action>,
}

impl ActionList {
    pub fn new() -> Self {
        ActionList { 
            x_override: None,
            actions: Vec::new() 
        }
    }
    
    /// 创建带override规则的ActionList
    pub fn with_override(rule: &str) -> Self {
        ActionList {
            x_override: Some(rule.to_string()),
            actions: Vec::new(),
        }
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
    
    /// 返回slice迭代器，支持rev()
    pub fn iter(&self) -> std::slice::Iter<'_, Action> {
        self.actions.iter()
    }
    
    pub fn last(&self) -> Option<&Action> {
        self.actions.last()
    }
    
    pub fn truncate(&mut self, len: usize) {
        self.actions.truncate(len);
    }
    
    /// 移除前n个元素，保留后面的
    pub fn drain_front(&mut self, n: usize) {
        self.actions.drain(0..n.min(self.actions.len()));
    }
    
    /// 清空列表
    pub fn clear(&mut self) {
        self.actions.clear();
    }
    
    /// 跳过前n个元素后的迭代器
    pub fn skip(&self, n: usize) -> std::iter::Skip<std::slice::Iter<'_, Action>> {
        self.actions.iter().skip(n)
    }
    
    /// 转换为Vec
    pub fn to_vec(&self) -> Vec<Action> {
        self.actions.clone()
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

impl FromIterator<Action> for ActionList {
    fn from_iter<I: IntoIterator<Item = Action>>(iter: I) -> Self {
        ActionList { 
            x_override: None,
            actions: iter.into_iter().collect() 
        }
    }
}

/// Memory 结构定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "Memory")]
pub struct Memory {
    /// 唯一标识
    #[serde(rename = "@id")]
    pub id: MemoryId,
    /// 版本号，每次合并递增
    #[serde(rename = "@version", default)]
    pub version: u32,
    /// 创建时间戳
    #[serde(rename = "@created-at")]
    pub created_at: String,
    /// 状态
    #[serde(rename = "@status", default)]
    pub status: MemoryStatus,
    /// xdsl命名空间声明
    #[serde(rename = "@xmlns:x", skip_serializing_if = "Option::is_none")]
    pub xmlns_x: Option<String>,
    /// 链式继承：指向基础文件的路径
    #[serde(rename = "@x:extends", skip_serializing_if = "Option::is_none")]
    pub x_extends: Option<String>,
    /// 用户意图
    #[serde(rename = "intent")]
    pub intent: String,
    /// 系统提示
    #[serde(rename = "system-prompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// 文件空间
    #[serde(default)]
    pub workspace: Workspace,
    /// 工具定义列表
    #[serde(default, skip_serializing_if = "Tools::is_empty")]
    pub tools: Tools,
    /// 技能定义列表
    #[serde(default, skip_serializing_if = "Skills::is_empty")]
    pub skills: Skills,
    /// 最近10次 action（用于 LLM 上下文窗口）
    /// 使用 ActionList 包装器来正确序列化
    #[serde(default)]
    pub current: ActionList,
    /// 所有 action 历史
    #[serde(default)]
    pub history: ActionList,
}

/// AgentContext 结构定义 - 兼容旧命名
pub type AgentContext = Memory;

impl Memory {
    /// 创建初始 Memory
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
    
    /// 创建带系统提示的 Memory
    pub fn with_system_prompt(intent: impl Into<String>, system_prompt: impl Into<String>) -> Self {
        let mut memory = Self::new(intent);
        memory.system_prompt = Some(system_prompt.into());
        memory
    }
    
    /// 从 XML 文件加载
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let memory: Memory = quick_xml::de::from_str(&content)
            .map_err(|e| crate::error::JieyushaError::ParseError(e.to_string()))?;
        Ok(memory)
    }
    
    /// 保存为 XML 文件
    pub fn save(&self, path: &Path) -> Result<()> {
        // 确保父目录存在
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        
        // 使用 quick-xml 序列化
        let content = quick_xml::se::to_string(self)
            .map_err(|e| crate::error::JieyushaError::ParseError(e.to_string()))?;
        
        // 添加 XML 声明
        let xml_content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
{}
"#, content);
        
        std::fs::write(path, xml_content)?;
        Ok(())
    }
    
    /// 转换为完整XML字符串
    pub fn to_xml_full(&self) -> Result<String> {
        let content = quick_xml::se::to_string(self)
            .map_err(|e| crate::error::JieyushaError::ParseError(e.to_string()))?;
        
        Ok(format!(r#"<?xml version="1.0" encoding="UTF-8"?>
{}"#, content))
    }
    
    /// 转换为 XML 字符串（简化版本，用于调试）
    pub fn to_xml(&self) -> Result<String> {
        self.to_xml_full()
    }
    
    /// 从 XML 字符串解析
    pub fn from_xml(xml: &str) -> Result<Self> {
        quick_xml::de::from_str(xml)
            .map_err(|e| crate::error::JieyushaError::ParseError(e.to_string()))
    }
    
    /// 添加 action 到历史
    pub fn add_action(&mut self, action: Action) {
        // 添加到 history
        self.history.push(action.clone());
        
        // 添加到 current（保持最近10个）
        self.current.push(action);
        if self.current.len() > 10 {
            self.current.drain_front(self.current.len() - 10);
        }
        
        // 递增版本
        self.version += 1;
    }
    
    /// 获取最近的 actions
    pub fn get_recent_actions(&self, count: usize) -> Vec<&Action> {
        self.history.iter().rev().take(count).collect()
    }
    
    /// 获取 action by id
    pub fn get_action(&self, id: &str) -> Option<&Action> {
        self.history.iter().find(|a| a.id == id)
    }
    
    /// 设置工具列表
    pub fn set_tools(&mut self, tools: Vec<ToolDef>) {
        self.tools = Tools::from(tools);
    }
    
    /// 设置技能列表
    pub fn set_skills(&mut self, skills: Vec<SkillDef>) {
        self.skills = Skills::from(skills);
    }
    
    /// 更新状态
    pub fn set_status(&mut self, status: MemoryStatus) {
        self.status = status;
        self.version += 1;
    }
    
    /// 标记任务成功完成
    pub fn mark_success(&mut self) {
        self.status = MemoryStatus::Success;
        self.version += 1;
    }
    
    /// 标记任务失败
    pub fn mark_failed(&mut self) {
        self.status = MemoryStatus::Failed;
        self.version += 1;
    }
    
    /// 合并另一个 Memory（作为差量）
    ///
    /// 使用 tuo/xdsl 模块的合并功能：
    /// - 根据 x:override 属性决定合并方式
    /// - history: 使用 ActionList 的 x:override 规则
    /// - workspace: merge
    /// - current: 重新计算（取最近10个）
    /// - 其他属性: delta覆盖base
    pub fn merge(&self, _delta: &Memory) -> Result<Self> {
        // TODO: 实现基于 tuo/xdsl 的合并
        // 暂时返回自身副本
        let mut result = self.clone();
        result.version += 1;
        Ok(result)
    }
    
    /// 从 history 重新计算 current
    /// 
    /// 规则：current = 最近的 historical-summary + 其后的 action（最多10个）
    fn update_current_from_history(&mut self) {
        // 找到最近的 historical-summary action
        let summary_idx = self.history.iter()
            .rposition(|a| a.action_type == ActionType::HistoricalSummary);
        
        // 先收集需要添加的 action
        let actions_to_add: Vec<Action> = if let Some(idx) = summary_idx {
            // current = 摘要 + 摘要之后的 action（最多保留10个）
            let mut actions = Vec::new();
            for (i, action) in self.history.iter().enumerate() {
                if i == idx {
                    actions.push(action.clone());
                } else if i > idx && actions.len() < 10 {
                    actions.push(action.clone());
                }
            }
            actions
        } else {
            // 无摘要，取最近10个
            let start = self.history.len().saturating_sub(10);
            self.history.iter().skip(start).cloned().collect()
        };
        
        // 清空并重新填充 current
        self.current.clear();
        for action in actions_to_add {
            self.current.push(action);
        }
    }
    
    /// 从基础文件和差量文件链加载
    /// 
    /// 使用 tuo/xdsl 的 process_extends 处理链式继承：
    /// - 最后一个差量文件通过 x:extends 指向前一个差量
    /// - process_extends 会自动处理整个继承链
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

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "tool")]
pub struct ToolDef {
    /// 工具名称
    #[serde(rename = "@name")]
    pub name: String,
    /// 工具描述
    #[serde(rename = "@description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 输入 JSON Schema
    #[serde(rename = "@input-schema", skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

impl ToolDef {
    /// 创建新的工具定义
    pub fn new(name: impl Into<String>) -> Self {
        ToolDef {
            name: name.into(),
            description: None,
            input_schema: None,
        }
    }
    
    /// 创建带描述的工具定义
    pub fn with_description(name: impl Into<String>, description: impl Into<String>) -> Self {
        ToolDef {
            name: name.into(),
            description: Some(description.into()),
            input_schema: None,
        }
    }
}

/// 工具列表（包装器，用于正确序列化为 <tools><tool>...</tool></tools>）
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
    
    pub fn push(&mut self, tool: ToolDef) {
        self.items.push(tool);
    }
    
    pub fn iter(&self) -> impl Iterator<Item = &ToolDef> {
        self.items.iter()
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
    /// 技能名称
    #[serde(rename = "@name")]
    pub name: String,
    /// 技能描述
    #[serde(rename = "@description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl SkillDef {
    /// 创建新的技能定义
    pub fn new(name: impl Into<String>) -> Self {
        SkillDef {
            name: name.into(),
            description: None,
        }
    }
    
    /// 创建带描述的技能定义
    pub fn with_description(name: impl Into<String>, description: impl Into<String>) -> Self {
        SkillDef {
            name: name.into(),
            description: Some(description.into()),
        }
    }
}

/// 技能列表（包装器）
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
    
    pub fn iter(&self) -> impl Iterator<Item = &SkillDef> {
        self.items.iter()
    }
}

impl From<Vec<SkillDef>> for Skills {
    fn from(items: Vec<SkillDef>) -> Self {
        Skills { items }
    }
}

/// 生成 ISO 8601 格式的时间戳
fn chrono_timestamp() -> String {
    // 使用简单的 RFC 3339 格式
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    
    let secs = now.as_secs();
    let datetime = chrono_like_format(secs);
    format!("{}Z", datetime)
}

/// 简单的时间格式化（避免引入chrono依赖）
fn chrono_like_format(secs: u64) -> String {
    // 简化实现，实际项目可以使用 chrono crate
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;
    
    // 从 1970-01-01 计算日期
    let (year, month, day) = days_to_ymd(days);
    
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", year, month, day, hours, minutes, seconds)
}

/// 将天数转换为年月日
fn days_to_ymd(days: u64) -> (i32, u32, u32) {
    // 简化实现
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

/// 判断是否是闰年
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// 将天数转换为月日
fn days_to_md(days: u32, leap: bool) -> (u32, u32) {
    let month_days = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    
    let mut remaining = days + 1; // days 是从0开始的
    for (i, &days_in_month) in month_days.iter().enumerate() {
        if remaining <= days_in_month {
            return ((i + 1) as u32, remaining);
        }
        remaining -= days_in_month;
    }
    
    (12, 31) // fallback
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_agent_context_new() {
        let ctx = AgentContext::new("测试意图");
        assert!(!ctx.id.is_empty());
        assert_eq!(ctx.version, 1);
        assert_eq!(ctx.intent, "测试意图");
        assert_eq!(ctx.status, ContextStatus::Pending);
    }
    
    #[test]
    fn test_agent_context_add_action() {
        let mut ctx = AgentContext::new("测试");
        let action = Action::thought("action-1", "思考内容");
        
        ctx.add_action(action);
        
        assert_eq!(ctx.history.len(), 1);
        assert_eq!(ctx.current.len(), 1);
        assert_eq!(ctx.version, 2);
    }
    
    #[test]
    fn test_agent_context_current_limit() {
        let mut ctx = AgentContext::new("测试");
        
        // 添加12个action
        for i in 0..12 {
            ctx.add_action(Action::thought(format!("action-{}", i), "思考"));
        }
        
        assert_eq!(ctx.history.len(), 12);
        assert_eq!(ctx.current.len(), 10); // 最多保留10个
    }
    
    #[test]
    fn test_agent_context_xml_roundtrip() {
        use tempfile::TempDir;
        
        let ctx = AgentContext::new("测试XML序列化");
        
        // 使用文件保存和加载来测试
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.xml");
        
        ctx.save(&file_path).unwrap();
        
        let file_content = std::fs::read_to_string(&file_path).unwrap();
        eprintln!("=== File Content ===\n{}", file_content);
        
        let loaded = AgentContext::load(&file_path).unwrap();
        
        assert_eq!(loaded.id, ctx.id);
        assert_eq!(loaded.intent, ctx.intent);
        assert_eq!(loaded.version, ctx.version);
    }
    
    #[test]
    fn test_context_with_actions() {
        // 测试带有 Action 的上下文序列化
        let mut ctx = AgentContext::new("测试带Action");
        ctx.add_action(Action::thought("action-001", "这是思考内容"));
        
        let xml = quick_xml::se::to_string(&ctx).unwrap();
        eprintln!("=== XML with Action ===\n{}", xml);
        
        // 当前 quick-xml 序列化 Vec 时，Action 的 serde(rename="action") 不生效
        // 因为 Vec 序列化时，容器字段名（history/current）会覆盖元素名
        // 这是 quick-xml 的已知行为，需要特殊处理
    }
    
    #[test]
    fn test_single_action_serialization() {
        // 单独测试 Action 的序列化
        let action = Action::thought("action-001", "测试内容");
        let xml = quick_xml::se::to_string(&action).unwrap();
        eprintln!("=== Single Action XML ===\n{}", xml);
        
        // 验证 action 节点名称
        assert!(xml.contains("<action "), "Action should serialize as <action>");
    }
    
    #[test]
    fn test_simple_xml_roundtrip() {
        // 测试最简单的 quick-xml 序列化/反序列化
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[serde(rename = "Memory")]
        struct SimpleMemory {
            #[serde(rename = "@id")]
            id: String,
            intent: String,
        }
        
        let m = SimpleMemory { 
            id: "test-id".to_string(), 
            intent: "hello".to_string() 
        };
        let xml = quick_xml::se::to_string(&m).unwrap();
        eprintln!("Simple XML: {}", xml);
        
        let parsed: SimpleMemory = quick_xml::de::from_str(&xml).unwrap();
        assert_eq!(parsed.id, m.id);
        assert_eq!(parsed.intent, m.intent);
    }
    
    #[test]
    fn test_context_with_workspace() {
        // 测试带 workspace 的结构
        let ctx = AgentContext::new("test");
        let xml = quick_xml::se::to_string(&ctx).unwrap();
        eprintln!("Context XML: {}", xml);
        
        // 尝试解析
        let parsed: AgentContext = quick_xml::de::from_str(&xml).unwrap();
        assert_eq!(parsed.id, ctx.id);
    }
    
    #[test]
    fn test_status_parsing() {
        // 测试 ContextStatus 解析
        let xml = r#"<Memory id="test-id" version="1" created-at="2026-03-05T00:00:00Z" status="PENDING"><intent>test</intent><workspace/></Memory>"#;
        eprintln!("Parsing with status: {}", xml);
        
        let result: std::result::Result<AgentContext, _> = quick_xml::de::from_str(xml);
        eprintln!("Result: {:?}", result);
        
        let parsed = result.unwrap();
        assert_eq!(parsed.status, ContextStatus::Pending);
    }
    
    #[test]
    fn test_minimal_memory_parsing() {
        // 最小化测试 - 直接解析一个简单的 XML
        // 注意：不包含 status 属性，测试是否是 ContextStatus 的问题
        let xml = r#"<Memory id="test-id" version="1" created-at="2026-03-05T00:00:00Z"><intent>test</intent><workspace/></Memory>"#;
        eprintln!("Parsing minimal XML: {}", xml);
        
        let parsed: AgentContext = quick_xml::de::from_str(xml).unwrap();
        assert_eq!(parsed.id, "test-id");
        assert_eq!(parsed.intent, "test");
    }
    
    #[test]
    fn test_tool_def() {
        let tool = ToolDef::with_description("bash", "执行bash命令");
        assert_eq!(tool.name, "bash");
        assert_eq!(tool.description, Some("执行bash命令".to_string()));
    }
    
    #[test]
    fn test_skill_def() {
        let skill = SkillDef::with_description("weather", "获取天气信息");
        assert_eq!(skill.name, "weather");
        assert_eq!(skill.description, Some("获取天气信息".to_string()));
    }
    
    #[test]
    fn test_tools_serialization() {
        // 测试 tools 序列化为正确的 XML 格式（属性形式）
        let mut ctx = AgentContext::new("测试tools");
        ctx.tools = Tools::from(vec![ToolDef::with_description("bash", "执行bash命令")]);
        
        let xml = ctx.to_xml_full().unwrap();
        eprintln!("=== Tools XML ===\n{}", xml);
        
        // 验证 tools 内容 - 属性形式（属性名不带 @ 前缀）
        assert!(xml.contains("<tools>"), "应该包含 tools 元素");
        assert!(xml.contains(r#"tool name="bash" description="执行bash命令""#), 
            "tool 应该使用属性形式");
    }
}
