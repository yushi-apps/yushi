//! Action 结构定义
//! 
//! 每次LLM交互或工具调用产生一个Action

use serde::{Deserialize, Serialize};
use tuo::OverrideRule;

/// Action 唯一标识类型
pub type ActionId = String;

/// Action 结构定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "action")]
pub struct Action {
    /// action 唯一标识
    #[serde(rename = "@id")]
    pub id: ActionId,
    /// 工具名称或 "thought"
    #[serde(rename = "@name")]
    pub name: String,
    /// 类型
    #[serde(rename = "@type")]
    pub action_type: ActionType,
    /// 合并规则（xdsl override）
    #[serde(rename = "@x:override", skip_serializing_if = "Option::is_none")]
    pub override_rule: Option<String>,
    /// 工具参数（JSON格式）
    #[serde(rename = "arguments", skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    /// 执行结果
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ActionResult>,
}

impl Action {
    /// 创建新的 Action
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
    
    /// 创建 thought 类型的 Action
    pub fn thought(id: impl Into<String>, content: impl Into<String>) -> Self {
        let mut action = Action::new(id, "thought", ActionType::Thought);
        action.result = Some(ActionResult {
            is_summary: false,
            status: ActionStatus::Ok,
            output: content.into(),
            error: None,
            raw_content: None,
        });
        action
    }
    
    /// 创建完整的工具 Action（包含调用参数和执行结果）
    /// 
    /// 一个 action 同时包含工具调用和结果
    pub fn tool(
        id: impl Into<String>, 
        tool_name: impl Into<String>, 
        arguments: serde_json::Value, 
        result: ActionResult
    ) -> Self {
        let mut action = Action::new(id, tool_name, ActionType::ToolCall);
        action.arguments = Some(arguments);
        action.result = Some(result);
        action
    }
    
    /// 创建系统消息类型的 Action
    pub fn system_message(id: impl Into<String>, content: impl Into<String>) -> Self {
        let mut action = Action::new(id, "system", ActionType::SystemMessage);
        action.result = Some(ActionResult {
            is_summary: false,
            status: ActionStatus::Ok,
            output: content.into(),
            error: None,
            raw_content: None,
        });
        action
    }
    
    /// 创建历史摘要类型的 Action
    /// 
    /// 由 Delta Agent 生成，用于压缩历史记录
    pub fn historical_summary(id: impl Into<String>, content: impl Into<String>, summarized_count: usize) -> Self {
        let mut action = Action::new(id, "_historical_summary", ActionType::HistoricalSummary);
        action.arguments = Some(serde_json::json!({
            "summarized_count": summarized_count,
            "summary_method": "llm_summarization"
        }));
        action.result = Some(ActionResult {
            is_summary: true,
            status: ActionStatus::Ok,
            output: content.into(),
            error: None,
            raw_content: None,
        });
        action
    }
    
    /// 设置合并规则
    pub fn with_override(mut self, rule: impl Into<String>) -> Self {
        self.override_rule = Some(rule.into());
        self
    }
    
    /// 获取合并规则
    pub fn get_override_rule(&self) -> OverrideRule {
        match self.override_rule.as_deref() {
            Some(s) => OverrideRule::from_str(s),
            None => OverrideRule::Merge,
        }
    }
}

/// Action 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    /// LLM 思考
    #[serde(rename = "thought")]
    Thought,
    /// 工具调用
    #[serde(rename = "tool-call")]
    ToolCall,
    /// 工具结果
    #[serde(rename = "tool-result")]
    ToolResult,
    /// 系统消息
    #[serde(rename = "system-message")]
    SystemMessage,
    /// 历史摘要（由 Delta Agent 生成的压缩摘要）
    #[serde(rename = "historical-summary")]
    HistoricalSummary,
}

impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionType::Thought => write!(f, "thought"),
            ActionType::ToolCall => write!(f, "tool-call"),
            ActionType::ToolResult => write!(f, "tool-result"),
            ActionType::SystemMessage => write!(f, "system-message"),
            ActionType::HistoricalSummary => write!(f, "historical-summary"),
        }
    }
}

impl ActionType {
    /// 获取类型的字符串表示
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

/// Action 执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "result")]
pub struct ActionResult {
    /// 是否为摘要
    #[serde(rename = "@is-summary", default)]
    pub is_summary: bool,
    /// 状态
    #[serde(rename = "@status")]
    pub status: ActionStatus,
    /// 结果摘要
    #[serde(rename = "@output", default)]
    pub output: String,
    /// 错误信息
    #[serde(rename = "@error", skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 工具输出完整内容
    #[serde(rename = "raw-content", skip_serializing_if = "Option::is_none")]
    pub raw_content: Option<String>,
}

impl ActionResult {
    /// 创建成功结果
    pub fn ok(output: impl Into<String>) -> Self {
        ActionResult {
            is_summary: false,
            status: ActionStatus::Ok,
            output: output.into(),
            error: None,
            raw_content: None,
        }
    }
    
    /// 创建错误结果
    pub fn error(error: impl Into<String>) -> Self {
        ActionResult {
            is_summary: false,
            status: ActionStatus::Error,
            output: String::new(),
            error: Some(error.into()),
            raw_content: None,
        }
    }
    
    /// 创建摘要结果
    pub fn summary(output: impl Into<String>) -> Self {
        ActionResult {
            is_summary: true,
            status: ActionStatus::Ok,
            output: output.into(),
            error: None,
            raw_content: None,
        }
    }
    
    /// 创建带原始内容的摘要结果
    pub fn summary_with_raw(output: impl Into<String>, raw_content: impl Into<String>) -> Self {
        ActionResult {
            is_summary: true,
            status: ActionStatus::Ok,
            output: output.into(),
            error: None,
            raw_content: Some(raw_content.into()),
        }
    }
    
    /// 设置原始内容
    pub fn with_raw_content(mut self, raw_content: impl Into<String>) -> Self {
        self.raw_content = Some(raw_content.into());
        self
    }
}

/// Action 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionStatus {
    /// 成功
    #[serde(rename = "OK")]
    Ok,
    /// 错误
    #[serde(rename = "ERROR")]
    Error,
}

impl Default for ActionStatus {
    fn default() -> Self {
        ActionStatus::Ok
    }
}

impl std::fmt::Display for ActionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionStatus::Ok => write!(f, "OK"),
            ActionStatus::Error => write!(f, "ERROR"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_action_thought() {
        let action = Action::thought("test-1", "这是一个思考");
        assert_eq!(action.action_type, ActionType::Thought);
        assert!(action.result.is_some());
    }
    
    #[test]
    fn test_action_result_ok() {
        let result = ActionResult::ok("成功");
        assert_eq!(result.status, ActionStatus::Ok);
        assert!(result.error.is_none());
    }
    
    #[test]
    fn test_action_result_error() {
        let result = ActionResult::error("失败");
        assert_eq!(result.status, ActionStatus::Error);
        assert!(result.error.is_some());
    }
    
    #[test]
    fn test_action_serialization_format() {
        // 测试 Action 序列化为正确的属性格式
        let action = Action::tool(
            "test-1",
            "bash",
            serde_json::json!({"cmd": "ls"}),
            ActionResult::ok("文件列表"),
        );
        
        let xml = quick_xml::se::to_string(&action).unwrap();
        eprintln!("=== Action XML ===\n{}", xml);
        
        // 验证属性格式（属性名不带 @ 前缀，那是 serde 的语法）
        assert!(xml.contains(r#"id="test-1""#), "id 应该是属性");
        assert!(xml.contains(r#"name="bash""#), "name 应该是属性");
        assert!(xml.contains(r#"type="tool-call""#), "type 应该是属性");
        // arguments 应该是子元素
        assert!(xml.contains("<arguments>"), "arguments 应该是子元素");
    }
    
    #[test]
    fn test_action_result_serialization_format() {
        // 测试 ActionResult 序列化为正确的属性格式
        let result = ActionResult {
            is_summary: false,
            status: ActionStatus::Ok,
            output: "执行成功".to_string(),
            error: None,
            raw_content: Some("完整输出内容".to_string()),
        };
        
        let xml = quick_xml::se::to_string(&result).unwrap();
        eprintln!("=== ActionResult XML ===\n{}", xml);
        
        // 验证属性格式（属性名不带 @ 前缀）
        assert!(xml.contains(r#"is-summary="false""#), "is-summary 应该是属性");
        assert!(xml.contains(r#"status="OK""#), "status 应该是属性");
        assert!(xml.contains(r#"output="执行成功""#), "output 应该是属性");
        // raw-content 应该是子元素
        assert!(xml.contains("<raw-content>"), "raw-content 应该是子元素");
    }
}
