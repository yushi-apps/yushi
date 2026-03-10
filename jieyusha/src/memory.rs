//! Memory 模块 - Agent 记忆结构与差量管理
//!
//! 包含：
//! - Memory 结构定义（对应 memory.xdef）
//! - Delta 差量管理方法

use std::path::{Path, PathBuf};
use std::fs;

use serde::{Deserialize, Serialize};
use tuo::OverrideRule;
use uuid::Uuid;

use crate::Registry;
use crate::SkillTool;
use crate::error::{JieyushaError, Result};
use crate::llm::ModelProfile;
use crate::messages::{ToolUse, ToolMessage};
use crate::summarizer::Summarizer;

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

impl ActionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionStatus::Ok => "OK",
            ActionStatus::Error => "ERROR",
        }
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

    /// 从 ToolUse 和 ToolMessage 创建工具调用 Action
    ///
    /// # 参数
    /// - `tool_use`: LLM 请求的工具调用
    /// - `tool_message`: 工具执行结果
    ///
    /// # 返回
    /// 创建的 Action，类型为 ToolCall，包含参数和结果
    pub fn tool(tool_use: &ToolUse, tool_message: &ToolMessage) -> Self {
        let arguments: serde_json::Value = serde_json::from_str(&tool_use.arguments)
            .unwrap_or(serde_json::Value::Null);

        let result = if tool_message.is_error {
            ActionResult::error(&tool_message.content)
        } else {
            ActionResult::ok(&tool_message.content)
        };

        let mut action = Action::new(
            tool_use.id.clone(),
            tool_use.name.clone(),
            ActionType::ToolCall,
        );
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
    
    /// 从 ToolUse 和 ToolMessage 创建 Action 并保存到 XML 文件
    /// 
    /// # 参数
    /// - `tool_use`: LLM 请求的工具调用
    /// - `tool_message`: 工具执行结果
    /// - `path`: 保存的文件路径
    /// 
    /// # 返回
    /// 创建的 Action 和保存的文件路径
    pub fn save_tool_action(
        tool_use: &ToolUse,
        tool_message: &ToolMessage,
        path: &Path,
    ) -> Result<Self> {
        let action = Self::tool(tool_use, tool_message);
        action.save(path)?;
        Ok(action)
    }
    
    /// 保存单个 Action 到 XML 文件
    /// 
    /// # 参数
    /// - `path`: 保存的文件路径
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let content = quick_xml::se::to_string(self)
            .map_err(|e| JieyushaError::ParseError(e.to_string()))?;
        let xml_content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
{}
"#, content);
        std::fs::write(path, xml_content)?;
        Ok(())
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
    #[serde(default, rename = "action", skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Action>,
    
    // 运行时字段（不序列化）
    #[serde(skip)]
    pub root_path: Option<PathBuf>,
    #[serde(skip)]
    pub summarizer: Option<Summarizer>,
    #[serde(skip)]
    pub delta_counter: u32,
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
            history: Vec::new(),
            root_path: None,
            summarizer: None,
            delta_counter: 0,
        }
    }

    /// 创建 tool 差量文件
    ///
    /// 将工具调用和结果保存为 N_tool.xml，继承前一个文件
    /// 一个文件包含完整的 tool-call + tool-result
    ///
    /// # 参数
    /// - `tool_use`: LLM 请求的工具调用
    /// - `tool_message`: 工具执行结果
    ///
    /// # 返回
    /// 保存的文件路径
    pub fn tool_action(tool_use: &ToolUse, tool_message: &ToolMessage) -> Result<PathBuf> {
        let registry = Registry::instance();
        let root_path = PathBuf::from(registry.root_path());
        let history_dir = root_path.join("history");
        fs::create_dir_all(&history_dir)?;
        
        // 获取下一个序号和继承目标
        let next_num = Self::get_next_number(&history_dir);
        let extends_file = Self::get_latest_file(&history_dir)
            .unwrap_or_else(|| "0_base.xml".to_string());
        
        // 创建 action（包含 tool-call + tool-result）
        let action = Action::tool(tool_use, tool_message);
        
        // 生成 XML（带继承）
        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
        xml.push_str(&format!(
            r#"<Memory xmlns:x="xdsl.xdef" x:extends="{}">
"#,
            extends_file
        ));
        
        // 写入 action（直接在 Memory 下）
        xml.push_str(&format!(
            r#"    <action id="{}" name="{}" type="{}">
"#,
            action.id,
            Self::escape_xml(&action.name),
            action.action_type.as_str()
        ));
        
        // 写入 arguments（如果有）
        if let Some(ref args) = action.arguments {
            let args_str = serde_json::to_string(args).unwrap_or_default();
            xml.push_str(&format!("        <arguments>{}</arguments>\n", Self::escape_xml(&args_str)));
        }
        
        // 写入 result（如果有）
        if let Some(ref result) = action.result {
            xml.push_str(&format!(
                r#"        <result is-summary="{}" status="{}" output="{}">
"#,
                result.is_summary,
                result.status.as_str(),
                Self::escape_xml(&result.output)
            ));
            if let Some(ref raw) = result.raw_content {
                xml.push_str(&format!("            <raw-content>{}</raw-content>\n", Self::escape_xml(raw)));
            }
            if let Some(ref error) = result.error {
                xml.push_str(&format!("            <error>{}</error>\n", Self::escape_xml(error)));
            }
            xml.push_str("        </result>\n");
        }
        
        xml.push_str("    </action>\n");
        xml.push_str("</Memory>\n");
        
        // 保存 N_tool.xml
        let tool_path = history_dir.join(format!("{}_tool.xml", next_num));
        fs::write(&tool_path, xml)?;

        Ok(tool_path)
    }

    /// 创建 intent 差量文件
    ///
    /// 将用户输入保存为 N_intent.xml，继承前一个文件
    ///
    /// # 参数
    /// - `intent`: 用户输入的意图
    ///
    /// # 返回
    /// 保存的文件路径
    pub fn intent(intent: &str) -> Result<PathBuf> {
        let registry = Registry::instance();
        let root_path = PathBuf::from(registry.root_path());
        let history_dir = root_path.join("history");
        fs::create_dir_all(&history_dir)?;
        
        // 获取下一个序号和继承目标
        let next_num = Self::get_next_number(&history_dir);
        let extends_file = Self::get_latest_file(&history_dir)
            .unwrap_or_else(|| "0_base.xml".to_string());
        
        // 创建 thought action
        let action_id = format!("action-{}", Uuid::new_v4());
        
        // 生成 XML（带继承）
        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
        xml.push_str(&format!(
            r#"<Memory xmlns:x="xdsl.xdef" x:extends="{}">
"#,
            extends_file
        ));
        xml.push_str(&format!("    <intent>{}</intent>\n", Self::escape_xml(intent)));
        // 写入 action（直接在 Memory 下）
        xml.push_str(&format!(
            r#"    <action id="{}" name="thought" type="thought">
"#,
            action_id
        ));
        xml.push_str(&format!(
            r#"        <result status="OK" output="{}" />
"#,
            Self::escape_xml(intent)
        ));
        xml.push_str("    </action>\n");
        xml.push_str("</Memory>\n");
        
        // 保存 N_intent.xml
        let intent_path = history_dir.join(format!("{}_intent.xml", next_num));
        fs::write(&intent_path, xml)?;

        Ok(intent_path)
    }

    /// 初始化 base 文件
    ///
    /// 生成 0_base.xml（初始配置，无继承）
    /// 如果已存在任何差量文件，则不执行
    ///
    /// # 返回
    /// 保存的文件路径
    pub fn init_base() -> Result<PathBuf> {
        let registry = Registry::instance();
        let root_path = PathBuf::from(registry.root_path());
        let history_dir = root_path.join("history");
        fs::create_dir_all(&history_dir)?;
        
        // 检查是否已有差量文件
        let next_num = Self::get_next_number(&history_dir);
        if next_num > 0 {
            // 已有文件，返回最新的 base 文件
            let files = Self::scan_history_files(&history_dir);
            for (_, name) in files.iter().rev() {
                if name.contains("_base") {
                    return Ok(history_dir.join(name));
                }
            }
        }
        
        // 从 YUSHI.md 获取 system_prompt
        let yushi_md_path = root_path.join("YUSHI.md");
        let system_prompt = fs::read_to_string(&yushi_md_path).unwrap_or_default();
        
        // 从 skills 目录获取 skills 信息
        let skills_dir = root_path.join("skills");
        let skills_xml = if skills_dir.exists() {
            SkillTool::load_skills(&skills_dir)
        } else {
            String::new()
        };
        
        // 生成 XML（无继承）
        let id = Uuid::new_v4().to_string();
        let created_at = chrono_timestamp();
        
        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
        xml.push_str(&format!(r#"<Memory id="{}" version="1" created-at="{}" status="PENDING">
"#, id, created_at));
        
        // system-prompt (可选)
        if !system_prompt.is_empty() {
            xml.push_str(&format!("    <system-prompt>{}</system-prompt>\n", Self::escape_xml(&system_prompt)));
        }
        
        // workspace
        xml.push_str("    <workspace />\n");
        
        // tools - 直接从 Registry 获取并写入
        xml.push_str("    <tools>\n");
        for tool in registry.get_all_tools() {
            xml.push_str(&format!(
                "        <tool name=\"{}\" description=\"{}\" />\n",
                tool.name(),
                tool.description()
            ));
        }
        xml.push_str("    </tools>\n");
        
        // skills - 仅当有内容时才写入
        if !skills_xml.is_empty() {
            xml.push_str(&format!("    {}\n", skills_xml));
        }
        
        // current (history 现在是 Memory 下的直接 action 元素)
        xml.push_str("    <current />\n");
        
        xml.push_str("</Memory>\n");
        
        // 保存 0_base.xml
        let base_path = history_dir.join("0_base.xml");
        fs::write(&base_path, xml)?;

        Ok(base_path)
    }
    
    /// XML 转义
    fn escape_xml(s: &str) -> String {
        s.replace("&", "&amp;")
            .replace("<", "&lt;")
            .replace(">", "&gt;")
            .replace("\"", "&quot;")
            .replace("'", "&apos;")
    }
    
    /// 扫描 history 目录，返回按数字排序的文件列表
    /// 
    /// 返回: Vec<(序号, 文件名)>
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
                    // 解析序号（文件名格式：N_type.xml）
                    if let Some(num_str) = name.split('_').next() {
                        if let Ok(num) = num_str.parse::<u32>() {
                            files.push((num, name));
                        }
                    }
                }
            }
        }
        
        // 按数字大小排序
        files.sort_by_key(|(num, _)| *num);
        files
    }
    
    /// 获取下一个序号
    fn get_next_number(history_dir: &Path) -> u32 {
        let files = Self::scan_history_files(history_dir);
        files.last().map(|(num, _)| num + 1).unwrap_or(0)
    }
    
    /// 获取最新文件名
    fn get_latest_file(history_dir: &Path) -> Option<String> {
        let files = Self::scan_history_files(history_dir);
        files.last().map(|(_, name)| name.clone())
    }
    
    /// 计算配置的 hash（用于检测变化）
    fn compute_config_hash(root_path: &Path) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // 计算 YUSHI.md 的 hash
        let yushi_md_path = root_path.join("YUSHI.md");
        if let Ok(content) = fs::read_to_string(&yushi_md_path) {
            content.hash(&mut hasher);
        }
        
        // 计算 skills 目录的 hash
        let skills_dir = root_path.join("skills");
        if skills_dir.exists() {
            if let Ok(entries) = fs::read_dir(&skills_dir) {
                let mut skill_files: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let path = e.path();
                        if path.is_dir() {
                            let skill_md = path.join("SKILL.md");
                            if skill_md.exists() {
                                Some((path.file_name()?.to_string_lossy().to_string(), skill_md))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect();
                skill_files.sort_by(|a, b| a.0.cmp(&b.0));
                for (name, skill_md) in skill_files {
                    name.hash(&mut hasher);
                    if let Ok(content) = fs::read_to_string(&skill_md) {
                        content.hash(&mut hasher);
                    }
                }
            }
        }
        
        format!("{:x}", hasher.finish())
    }
    
    /// 检查并更新 base 配置
    ///
    /// 检测 skills 目录和 YUSHI.md 的变化，如有变化则生成新的 N_base.xml
    ///
    /// # 返回
    /// - Some(PathBuf): 生成了新的 base 文件
    /// - None: 无变化
    pub fn check_and_update_base() -> Result<Option<PathBuf>> {
        let registry = Registry::instance();
        let root_path = PathBuf::from(registry.root_path());
        let history_dir = root_path.join("history");
        fs::create_dir_all(&history_dir)?;
        
        // 计算 hash 文件路径
        let hash_file = history_dir.join(".config_hash");
        
        // 计算当前配置 hash
        let current_hash = Self::compute_config_hash(&root_path);
        
        // 读取上次保存的 hash
        let saved_hash = fs::read_to_string(&hash_file).unwrap_or_default();
        
        // 比较 hash
        if current_hash == saved_hash {
            return Ok(None);
        }
        
        // 有变化，生成新的 base 差量文件
        let next_num = Self::get_next_number(&history_dir);
        let extends_file = Self::get_latest_file(&history_dir)
            .unwrap_or_else(|| "0_base.xml".to_string());
        
        // 如果是第一个文件，使用 init_base
        if next_num == 0 {
            let base_path = Self::init_base()?;
            // 保存 hash
            fs::write(&hash_file, &current_hash)?;
            return Ok(Some(base_path));
        }
        
        // 从 YUSHI.md 获取 system_prompt
        let yushi_md_path = root_path.join("YUSHI.md");
        let system_prompt = fs::read_to_string(&yushi_md_path).unwrap_or_default();
        
        // 从 skills 目录获取 skills 信息
        let skills_dir = root_path.join("skills");
        let skills_xml = if skills_dir.exists() {
            SkillTool::load_skills(&skills_dir)
        } else {
            String::new()
        };
        
        // 生成差量 XML
        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
        xml.push_str(&format!(
            r#"<Memory xmlns:x="xdsl.xdef" x:extends="{}">
"#,
            extends_file
        ));
        
        // system-prompt (如果有变化)
        if !system_prompt.is_empty() {
            xml.push_str(&format!("    <system-prompt>{}</system-prompt>\n", Self::escape_xml(&system_prompt)));
        }
        
        // skills (如果有)
        if !skills_xml.is_empty() {
            xml.push_str(&format!("    {}\n", skills_xml));
        }
        
        xml.push_str("</Memory>\n");
        
        // 保存 N_base.xml
        let base_path = history_dir.join(format!("{}_base.xml", next_num));
        fs::write(&base_path, xml)?;
        
        // 保存 hash
        fs::write(&hash_file, &current_hash)?;
        
        Ok(Some(base_path))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let memory: Memory = quick_xml::de::from_str(&content)
            .map_err(|e| JieyushaError::ParseError(e.to_string()))?;
        Ok(memory)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let content = quick_xml::se::to_string(self)
            .map_err(|e| JieyushaError::ParseError(e.to_string()))?;
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
        delta_paths: &[PathBuf],
    ) -> Result<Self> {
        if delta_paths.is_empty() {
            return Self::load(base_path);
        }
        // TODO: 实现基于 tuo/xdsl 的链式继承加载
        // 暂时只加载 base
        Self::load(base_path)
    }
    
    // ========================================================================
    // Delta 管理方法（原 DeltaAgent 功能）
    // ========================================================================
    
    /// 初始化 Memory 并创建差量管理环境
    /// 
    /// # 参数
    /// - `root_path`: App::root_path() 返回的根目录
    /// - `model`: 模型配置
    /// - `intent`: 用户意图
    /// - `system_prompt`: 系统提示（可选）
    /// 
    /// # 创建的文件
    /// - history/0_base.xml: 初始Memory实例（空history/current）
    /// - history/1_intent.xml: 第一个差量（包含初始thought）
    /// - history/current.xml: 合并后的完整状态
    pub fn init(
        root_path: PathBuf,
        _model: ModelProfile,
        intent: &str,
        system_prompt: Option<&str>,
    ) -> Result<Self> {
        let history_dir = root_path.join("history");
        fs::create_dir_all(&history_dir)
            .map_err(|e| JieyushaError::IoError(e))?;
        
        // 创建初始上下文
        let mut memory = Memory::new(intent);
        if let Some(prompt) = system_prompt {
            memory.system_prompt = Some(prompt.to_string());
        }
        memory.root_path = Some(root_path.clone());
        memory.delta_counter = 0;
        
        // 保存 0_base.xml（空的初始状态）
        let base_path = history_dir.join("0_base.xml");
        memory.save(&base_path)?;
        
        // 创建初始thought action（用户意图）
        let initial_action = Action::thought(
            format!("action-{}", uuid::Uuid::new_v4()),
            intent,
        );
        
        // 创建第一个差量文件 1_intent.xml
        memory.create_delta(&initial_action)?;
        
        // 更新 memory
        memory.add_action(initial_action);
        
        // 保存current.xml
        memory.save_current()?;
        
        Ok(memory)
    }
    
    /// 检查是否需要生成周期性摘要
    /// 
    /// 当 history 超过 10 条 action 时触发
    pub fn should_generate_summary(&self) -> bool {
        self.history.len() > 10
    }
    
    /// 检查并生成周期性摘要（如果需要）
    /// 
    /// 便捷方法，用于在添加 action 后调用
    pub async fn maybe_generate_summary(&mut self) -> Result<Option<String>> {
        if self.should_generate_summary() {
            self.generate_periodic_summary().await
        } else {
            Ok(None)
        }
    }
    
    /// 生成周期性摘要
    /// 
    /// 当 history 超过 10 条 action 时，调用 LLM 生成摘要
    /// - history: 保留完整记录（原始 action + 摘要 action），永不删除
    /// - current: 更新为摘要 action + 最近的 action
    pub async fn generate_periodic_summary(&mut self) -> Result<Option<String>> {
        if !self.should_generate_summary() {
            return Ok(None);
        }
        
        // 找到最近的 historical-summary action
        let last_summary_idx = self.history.iter()
            .rposition(|a| a.action_type == ActionType::HistoricalSummary);
        
        // 确定需要摘要的 action 范围
        let start_idx = last_summary_idx.map(|idx| idx + 1).unwrap_or(0);
        let actions_to_summarize: Vec<&Action> = self.history.iter()
            .skip(start_idx)
            .take(self.history.len() - start_idx - 5) // 保留最近5个
            .collect();
        
        if actions_to_summarize.is_empty() {
            return Ok(None);
        }
        
        let summarize_count = actions_to_summarize.len();
        
        // 构建摘要请求内容
        let context_text = actions_to_summarize
            .iter()
            .map(|a| {
                let type_str = a.action_type.as_str();
                let name = &a.name;
                let output = a.result.as_ref().map(|r| r.output.as_str()).unwrap_or("");
                let args = a.arguments.as_ref()
                    .map(|v| serde_json::to_string(v).unwrap_or_default())
                    .unwrap_or_default();
                format!("[{}] {} | args: {} | result: {}", type_str, name, args, output)
            })
            .collect::<Vec<_>>()
            .join("\n");
        
        // 调用 LLM 生成摘要
        let summary = if let Some(ref summarizer) = self.summarizer {
            summarizer.summarize_actions(
                &context_text,
                &self.intent,
                summarize_count,
            ).await?
        } else {
            context_text
        };
        
        // 创建历史摘要 action
        let summary_action = Action::historical_summary(
            format!("action-summary-{}", uuid::Uuid::new_v4()),
            &summary,
            summarize_count,
        );
        
        // 添加摘要到 history（不删除原始 action）
        self.add_action(summary_action);
        
        // 更新 current（摘要 + 最近 action）
        self.update_current();
        
        // 创建摘要差量文件（先克隆 action 避免借用冲突）
        let summary_action_clone = self.history.last().unwrap().clone();
        let _ = self.create_delta_file(&summary_action_clone)?;
        
        // 保存 current.xml
        self.save_current()?;
        
        log::info!(
            "Generated periodic summary: compressed {} actions into summary", 
            summarize_count
        );
        
        Ok(Some(summary))
    }
    
    /// 更新 current 字段
    /// 
    /// current = 最近的 historical-summary + 其后的 action（最多10个）
    fn update_current(&mut self) {
        // 找到最近的 historical-summary action
        let summary_idx = self.history.iter()
            .rposition(|a| a.action_type == ActionType::HistoricalSummary);
        
        // 先收集需要添加的 action
        let actions_to_add: Vec<Action> = if let Some(idx) = summary_idx {
            // current = 摘要 + 摘要之后的 action（最多保留10个）
            let mut actions = Vec::new();
            if let Some(summary_action) = self.history.iter().nth(idx) {
                actions.push(summary_action.clone());
            }
            for action in self.history.iter().skip(idx + 1).take(9) {
                actions.push(action.clone());
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
    
    /// 创建差量文件
    pub fn create_delta(&mut self, action: &Action) -> Result<PathBuf> {
        self.create_delta_file(action)
    }
    
    /// 创建差量文件（内部方法）
    fn create_delta_file(&mut self, action: &Action) -> Result<PathBuf> {
        let root_path = self.root_path.as_ref().ok_or_else(|| {
            JieyushaError::ConfigError("root_path not set".to_string())
        })?;
        
        let history_dir = root_path.join("history");
        
        // 获取下一个序号和继承目标
        let next_num = Self::get_next_number(&history_dir);
        let extends_file = Self::get_latest_file(&history_dir)
            .unwrap_or_else(|| "0_base.xml".to_string());
        
        // 确定文件类型后缀
        let type_suffix = match action.action_type {
            ActionType::ToolCall | ActionType::ToolResult => "tool",
            ActionType::HistoricalSummary => "summary",
            ActionType::Thought => "intent",
            ActionType::SystemMessage => "system",
        };
        
        // 文件名格式: N_type.xml
        let filename = format!("{}_{}.xml", next_num, type_suffix);
        let delta_path = history_dir.join(&filename);
        
        // 生成差量 XML
        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
"#);
        xml.push_str(&format!(
            r#"<Memory xmlns:x="xdsl.xdef" x:extends="{}">
"#,
            extends_file
        ));
        
        // 写入 action（直接在 Memory 下，不在 history 容器内）
        xml.push_str(&format!(
            r#"    <action id="{}" name="{}" type="{}">
"#,
            action.id,
            Self::escape_xml(&action.name),
            action.action_type.as_str()
        ));
        
        // 写入 arguments（如果有）
        if let Some(ref args) = action.arguments {
            let args_str = serde_json::to_string(args).unwrap_or_default();
            xml.push_str(&format!("        <arguments>{}</arguments>\n", Self::escape_xml(&args_str)));
        }
        
        // 写入 result（如果有）
        if let Some(ref result) = action.result {
            xml.push_str(&format!(
                r#"        <result is-summary="{}" status="{}" output="{}">
"#,
                result.is_summary,
                result.status.as_str(),
                Self::escape_xml(&result.output)
            ));
            if let Some(ref raw) = result.raw_content {
                xml.push_str(&format!("            <raw-content>{}</raw-content>\n", Self::escape_xml(raw)));
            }
            if let Some(ref error) = result.error {
                xml.push_str(&format!("            <error>{}</error>\n", Self::escape_xml(error)));
            }
            xml.push_str("        </result>\n");
        }
        
        xml.push_str("    </action>\n");
        xml.push_str("</Memory>\n");
        
        fs::write(&delta_path, xml)?;
        
        // 更新 delta_counter
        self.delta_counter = next_num;
        
        Ok(delta_path)
    }
    
    /// 保存上下文到current.xml
    pub fn save_current(&self) -> Result<()> {
        let root_path = self.root_path.as_ref().ok_or_else(|| {
            JieyushaError::ConfigError("root_path not set".to_string())
        })?;
        let current_path = root_path.join("history").join("current.xml");
        self.save(&current_path)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    fn get_test_model() -> ModelProfile {
        ModelProfile {
            model_name: "test".to_string(),
            base_url: "http://localhost".to_string(),
            api_key: "test".to_string(),
            max_tokens: 1000,
            temperature: 0.7,
        }
    }
    
    #[test]
    fn test_memory_init() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path().to_path_buf();
        
        let memory = Memory::init(
            root_path.clone(),
            get_test_model(),
            "测试任务",
            Some("测试系统提示"),
        ).unwrap();
        
        assert_eq!(memory.intent, "测试任务");
        // 新建时会创建初始 thought action，所以 delta_counter 为 1
        assert_eq!(memory.delta_counter, 1);
        // history 应该有一个初始 thought action
        assert_eq!(memory.history.len(), 1);
        
        // 检查0_base.xml是否存在
        let base_path = root_path.join("history").join("0_base.xml");
        assert!(base_path.exists());
        
        // 检查1_intent.xml差量文件是否存在
        let history_dir = root_path.join("history");
        let delta_files: Vec<_> = std::fs::read_dir(&history_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("1_"))
            .collect();
        assert_eq!(delta_files.len(), 1);
    }
    
    #[test]
    fn test_delta_file_format_with_extends() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path().to_path_buf();
        
        let mut memory = Memory::init(
            root_path.clone(),
            get_test_model(),
            "测试继承链",
            None,
        ).unwrap();
        
        let history_dir = root_path.join("history");
        
        // 第一个差量（由 add_action 创建）
        let action1 = Action::thought("action-001", "第一个思考");
        memory.add_action(action1.clone());
        memory.create_delta_file(&action1).unwrap();
        
        // 文件名格式是 N_thought.xml（不带前导零）
        let delta1_path = history_dir.join("2_intent.xml");
        let delta1_content = std::fs::read_to_string(&delta1_path).unwrap();
        
        println!("=== Delta File 1 ===\n{}", delta1_content);
        
        // 验证包含 xmlns:x 和 x:extends
        assert!(delta1_content.contains("xmlns:x"), "应该包含xmlns:x命名空间");
        assert!(delta1_content.contains("x:extends"), "应该包含x:extends继承链");
        // 第一个差量应该继承 1_intent.xml（由 init 创建的初始 intent）
        assert!(delta1_content.contains("x:extends=\"1_intent.xml\""), "应该继承初始intent文件");
        
        // 第二个差量应该继承前一个差量
        let action2 = Action::thought("action-002", "第二个思考");
        memory.add_action(action2.clone());
        memory.create_delta_file(&action2).unwrap();
        
        let delta2_path = history_dir.join("3_intent.xml");
        let delta2_content = std::fs::read_to_string(&delta2_path).unwrap();
        
        println!("=== Delta File 2 ===\n{}", delta2_content);
        
        // 验证继承链指向正确的文件（使用完整文件名）
        assert!(delta2_content.contains("x:extends=\"2_intent.xml\""), "应该继承前一个差量文件2_intent.xml");
    }
    
    #[test]
    fn test_should_generate_summary() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path().to_path_buf();
        
        let mut memory = Memory::init(
            root_path.clone(),
            get_test_model(),
            "测试摘要触发",
            None,
        ).unwrap();
        
        // 初始状态：只有1个 thought action，不应该触发摘要
        assert!(!memory.should_generate_summary());
        
        // 添加 9 个 action（总共 10 个）
        for i in 0..9 {
            let action = Action::thought(format!("action-{}", i), format!("思考 {}", i));
            memory.add_action(action);
        }
        
        // 现在 history 有 10 个 action，不应该触发（条件是 > 10）
        assert_eq!(memory.history.len(), 10);
        assert!(!memory.should_generate_summary());
        
        // 添加第 11 个 action
        let action = Action::thought("action-10", "第十一个思考");
        memory.add_action(action);
        
        // 现在应该触发摘要生成
        assert_eq!(memory.history.len(), 11);
        assert!(memory.should_generate_summary());
    }
    
    #[test]
    fn test_historical_summary_action_creation() {
        let action = Action::historical_summary(
            "action-summary-test",
            "这是历史摘要内容",
            5,
        );
        
        assert_eq!(action.action_type, ActionType::HistoricalSummary);
        assert_eq!(action.name, "_historical_summary");
        assert!(action.result.is_some());
        
        let result = action.result.unwrap();
        assert!(result.is_summary);
        assert_eq!(result.output, "这是历史摘要内容");
        
        // 验证 arguments 包含 summarized_count
        let args = action.arguments.unwrap();
        assert_eq!(args["summarized_count"], 5);
    }
    
    #[test]
    fn test_update_current_with_summary() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path().to_path_buf();
        
        let mut memory = Memory::init(
            root_path.clone(),
            get_test_model(),
            "测试current更新",
            None,
        ).unwrap();
        
        // 添加 10 个 action
        for i in 0..10 {
            let action = Action::thought(format!("action-{}", i), format!("思考 {}", i));
            memory.add_action(action);
        }
        
        // 手动添加一个 historical-summary action
        let summary_action = Action::historical_summary(
            "action-summary-001",
            "历史摘要",
            5,
        );
        memory.add_action(summary_action);
        
        // 再添加几个 action
        for i in 11..14 {
            let action = Action::thought(format!("action-{}", i), format!("思考 {}", i));
            memory.add_action(action);
        }
        
        // 调用 update_current
        memory.update_current();
        
        // current 应该包含：摘要 + 摘要之后的 action
        assert!(memory.current.len() > 0);
        
        // 第一个应该是 summary
        let first = memory.current.iter().next().unwrap();
        assert_eq!(first.action_type, ActionType::HistoricalSummary);
    }
}
