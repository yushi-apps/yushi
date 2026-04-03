//! Automation rule module.
//!
//! This module contains:
//! - Rule struct definition and related types
//! - XML serialization/deserialization
//! - Rule configuration types

use serde::{Deserialize, Serialize};
use serde_json;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// 操作符枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Op {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
    Ne,
    Contains,
    CronMatch,
}

impl Op {
    /// 转换为 XML 属性值
    pub fn as_str(&self) -> &'static str {
        match self {
            Op::Gt => "gt",
            Op::Lt => "lt",
            Op::Gte => "gte",
            Op::Lte => "lte",
            Op::Eq => "eq",
            Op::Ne => "ne",
            Op::Contains => "contains",
            Op::CronMatch => "cron-match",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "gt" => Some(Op::Gt),
            "lt" => Some(Op::Lt),
            "gte" => Some(Op::Gte),
            "lte" => Some(Op::Lte),
            "eq" => Some(Op::Eq),
            "ne" => Some(Op::Ne),
            "contains" => Some(Op::Contains),
            "cron-match" => Some(Op::CronMatch),
            _ => None,
        }
    }
}

/// 匹配模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    All,
    Any,
}

impl Default for MatchMode {
    fn default() -> Self {
        MatchMode::All
    }
}

impl MatchMode {
    /// 转换为 XML 属性值
    pub fn as_str(&self) -> &'static str {
        match self {
            MatchMode::All => "all",
            MatchMode::Any => "any",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "all" => Some(MatchMode::All),
            "any" => Some(MatchMode::Any),
            _ => None,
        }
    }
}

/// 条件（叶子谓词）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    /// 数据字段路径（如 "temperature"、"time"）
    pub field: String,
    /// 操作符
    pub op: Op,
    /// 比较值（cron-match 时为 cron 表达式）
    pub value: String,
    /// 持续时间要求（秒）：条件必须连续满足指定秒数
    pub duration_seconds: Option<u32>,
}

impl Condition {
    /// 序列化为自闭合 XML 元素
    pub fn to_xml_element(&self, indent: &str) -> String {
        let mut s = format!(
            "{}<condition field=\"{}\" op=\"{}\" value=\"{}\"",
            indent,
            xml_escape(&self.field),
            self.op.as_str(),
            xml_escape(&self.value)
        );
        if let Some(dur) = self.duration_seconds {
            s.push_str(&format!(" duration-seconds=\"{}\"", dur));
        }
        s.push_str(" />");
        s
    }

    /// 从 XML 属性解析
    fn from_xml_attrs(start: &BytesStart) -> anyhow::Result<Self> {
        let mut field = String::new();
        let mut op = None;
        let mut value = String::new();
        let mut duration_seconds = None;

        for attr in start.attributes().flatten() {
            let key = std::str::from_utf8(attr.key.as_ref())?;
            let val = xml_unescape(std::str::from_utf8(&attr.value)?);
            match key {
                "field" => field = val,
                "op" => op = Op::from_str(&val),
                "value" => value = val,
                "duration-seconds" => duration_seconds = val.parse().ok(),
                _ => {}
            }
        }

        Ok(Condition {
            field,
            op: op.ok_or_else(|| anyhow::anyhow!("missing or invalid op"))?,
            value,
            duration_seconds,
        })
    }
}

/// 条件组（逻辑组合器，支持递归嵌套）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionGroup {
    /// 组内匹配模式：all(AND) / any(OR)
    pub match_mode: MatchMode,
    /// 组内条件列表
    pub conditions: Vec<Condition>,
    /// 嵌套子组
    pub groups: Vec<ConditionGroup>,
}

impl ConditionGroup {
    /// 序列化为 XML 元素
    pub fn to_xml_element(&self, indent: &str) -> String {
        let child_indent = format!("{}    ", indent);
        let mut s = format!("{}<group match=\"{}\">", indent, self.match_mode.as_str());

        for cond in &self.conditions {
            s.push('\n');
            s.push_str(&cond.to_xml_element(&child_indent));
        }
        for grp in &self.groups {
            s.push('\n');
            s.push_str(&grp.to_xml_element(&child_indent));
        }

        s.push('\n');
        s.push_str(indent);
        s.push_str("</group>");
        s
    }

    /// 从 XML 递归解析
    fn from_xml_reader(reader: &mut Reader<&[u8]>, start: &BytesStart) -> anyhow::Result<Self> {
        let mut match_mode = MatchMode::All;
        for attr in start.attributes().flatten() {
            let key = std::str::from_utf8(attr.key.as_ref())?;
            let val = std::str::from_utf8(&attr.value)?;
            if key == "match" {
                match_mode = MatchMode::from_str(val).unwrap_or(MatchMode::All);
            }
        }

        let mut conditions = Vec::new();
        let mut groups = Vec::new();
        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref())?;
                    if tag == "group" {
                        groups.push(ConditionGroup::from_xml_reader(reader, e)?);
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref())?;
                    if tag == "condition" {
                        conditions.push(Condition::from_xml_attrs(e)?);
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref())?;
                    if tag == "group" {
                        break;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow::anyhow!("XML parse error: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        Ok(ConditionGroup {
            match_mode,
            conditions,
            groups,
        })
    }
}

/// 规则执行状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RuleStatus {
    Ok,
    Error,
    Skipped,
}

impl RuleStatus {
    /// 转换为 XML 属性值
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleStatus::Ok => "ok",
            RuleStatus::Error => "error",
            RuleStatus::Skipped => "skipped",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ok" => Some(RuleStatus::Ok),
            "error" => Some(RuleStatus::Error),
            "skipped" => Some(RuleStatus::Skipped),
            _ => None,
        }
    }
}

/// 规则定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    /// 关联的事件源 ID（"clock" 表示时间驱动）
    pub source_id: String,
    /// 顶层匹配模式
    pub match_mode: MatchMode,
    /// 捕获前 N 秒数据
    pub capture_pre_seconds: Option<u32>,
    /// 触发执行的消息/指令
    pub message: String,
    /// 超时秒数
    pub timeout_seconds: u32,
    /// 交付通道：stdout / file / none
    pub delivery_channel: String,
    /// 交付目标（file 时为文件路径）
    pub delivery_target: String,
    /// 顶层条件列表
    pub conditions: Vec<Condition>,
    /// 顶层条件组列表
    pub groups: Vec<ConditionGroup>,
}

impl Default for Rule {
    fn default() -> Self {
        Rule {
            id: String::new(),
            name: String::new(),
            description: None,
            enabled: true,
            source_id: "clock".to_string(),
            match_mode: MatchMode::All,
            capture_pre_seconds: None,
            message: String::new(),
            timeout_seconds: 300,
            delivery_channel: "none".to_string(),
            delivery_target: String::new(),
            conditions: Vec::new(),
            groups: Vec::new(),
        }
    }
}

impl Rule {
    /// 创建新规则
    pub fn new(id: String, name: String, source_id: String, message: String) -> Self {
        Rule {
            id,
            name,
            source_id,
            message,
            ..Default::default()
        }
    }

    /// 是否为时钟规则
    pub fn is_clock_rule(&self) -> bool {
        self.source_id == "clock"
    }

    /// 应用补丁更新
    pub fn apply_patch(&mut self, patch: RulePatch) {
        if let Some(name) = patch.name {
            self.name = name;
        }
        if let Some(desc) = patch.description {
            self.description = desc;
        }
        if let Some(enabled) = patch.enabled {
            self.enabled = enabled;
        }
        if let Some(message) = patch.message {
            self.message = message;
        }
        if let Some(timeout) = patch.timeout_seconds {
            self.timeout_seconds = timeout;
        }
        if let Some(channel) = patch.delivery_channel {
            self.delivery_channel = channel;
        }
        if let Some(target) = patch.delivery_target {
            self.delivery_target = target;
        }
        if let Some(conditions) = patch.conditions {
            self.conditions = conditions;
        }
        if let Some(groups) = patch.groups {
            self.groups = groups;
        }
    }

    /// 序列化为 XML 元素
    pub fn to_xml_element(&self) -> String {
        self.to_xml_element_with_indent("    ")
    }

    fn to_xml_element_with_indent(&self, indent: &str) -> String {
        let child_indent = format!("{}    ", indent);
        let mut s = format!("{}<rule id=\"{}\" name=\"{}\"", indent, xml_escape(&self.id), xml_escape(&self.name));

        if let Some(ref desc) = self.description {
            s.push_str(&format!(" description=\"{}\"", xml_escape(desc)));
        }
        // enabled 默认 true 时省略
        if !self.enabled {
            s.push_str(" enabled=\"false\"");
        }
        s.push_str(&format!(" source-id=\"{}\"", xml_escape(&self.source_id)));
        s.push_str(&format!(" match=\"{}\"", self.match_mode.as_str()));
        if let Some(c) = self.capture_pre_seconds {
            s.push_str(&format!(" capture-pre-seconds=\"{}\"", c));
        }
        s.push_str(&format!(" message=\"{}\"", xml_escape(&self.message)));
        s.push_str(&format!(" timeout-seconds=\"{}\"", self.timeout_seconds));

        // delivery 属性（非默认值时输出）
        if self.delivery_channel != "none" {
            s.push_str(&format!(" delivery-channel=\"{}\"", xml_escape(&self.delivery_channel)));
        }
        if !self.delivery_target.is_empty() {
            s.push_str(&format!(" delivery-target=\"{}\"", xml_escape(&self.delivery_target)));
        }

        // 检查是否有子元素
        if self.conditions.is_empty() && self.groups.is_empty() {
            s.push_str(" />");
        } else {
            s.push('>');
            for cond in &self.conditions {
                s.push('\n');
                s.push_str(&cond.to_xml_element(&child_indent));
            }
            for grp in &self.groups {
                s.push('\n');
                s.push_str(&grp.to_xml_element(&child_indent));
            }
            s.push('\n');
            s.push_str(indent);
            s.push_str("</rule>");
        }
        s
    }

    /// 序列化为带 x:override 的更新差量
    pub fn to_merge_xml_element(&self, patch: &RulePatch) -> String {
        rule_merge_xml_element(&self.id, patch)
    }
    /// 生成与 Rule 对应的 task.xdef 计划
    pub fn to_task_xdef(&self, source_session: &str) -> String {
        let created_at_ms = chrono::Utc::now().timestamp_millis();
        let description = if self.name.is_empty() {
            format!("Rule {} execution", self.id)
        } else {
            self.name.clone()
        };

        // 以 Task 工具作为执行载体，直接使用规则 message 作为 prompt
        let input_json = serde_json::json!({
            "description": description,
            "prompt": self.message,
            "subagent_type": "schedule"
        });

        let mut xml = String::new();
        xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str(&format!(
            "<task xmlns:x=\"/nop/schema/xdsl.xdef\" name=\"rule_{}\" source-session=\"{}\" created-at-ms=\"{}\">\n",
            xml_escape(&self.id),
            xml_escape(source_session),
            created_at_ms
        ));
        xml.push_str("  <sequence name=\"main\">\n");
        xml.push_str(&format!(
            "    <call-tool name=\"{}\" tool=\"Task\">\n",
            xml_escape(&self.id)
        ));
        xml.push_str("      <input name=\"description\" type=\"string\">\n");
        xml.push_str(&format!(
            "        <source xdef:value=\"{}\" />\n",
            xml_escape(&description)
        ));
        xml.push_str("      </input>\n");
        xml.push_str("      <input name=\"prompt\" type=\"string\">\n");
        xml.push_str(&format!(
            "        <source xdef:value=\"{}\" />\n",
            xml_escape(&self.message)
        ));
        xml.push_str("      </input>\n");
        xml.push_str("      <input name=\"subagent_type\" type=\"string\">\n");
        xml.push_str("        <source xdef:value=\"schedule\" />\n");
        xml.push_str("      </input>\n");
        xml.push_str("    </call-tool>\n");
        xml.push_str("  </sequence>\n");
        xml.push_str("</task>\n");

        xml
    }
    /// 序列化为删除差量
    pub fn to_remove_xml_element(&self) -> String {
        format!("    <rule id=\"{}\" x:override=\"remove\" />", xml_escape(&self.id))
    }

    /// 从 XML reader 解析单个 rule
    fn from_xml_reader(reader: &mut Reader<&[u8]>, start: &BytesStart) -> anyhow::Result<Self> {
        let mut rule = Rule::default();

        for attr in start.attributes().flatten() {
            let key = std::str::from_utf8(attr.key.as_ref())?;
            let val = xml_unescape(std::str::from_utf8(&attr.value)?);
            match key {
                "id" => rule.id = val,
                "name" => rule.name = val,
                "description" => rule.description = Some(val),
                "enabled" => rule.enabled = val != "false",
                "source-id" => rule.source_id = val,
                "match" => rule.match_mode = MatchMode::from_str(&val).unwrap_or(MatchMode::All),
                "capture-pre-seconds" => rule.capture_pre_seconds = val.parse().ok(),
                "message" => rule.message = val,
                "timeout-seconds" => rule.timeout_seconds = val.parse().unwrap_or(300),
                "delivery-channel" => rule.delivery_channel = val,
                "delivery-target" => rule.delivery_target = val,
                _ => {}
            }
        }

        // 解析子元素
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref())?;
                    if tag == "group" {
                        rule.groups.push(ConditionGroup::from_xml_reader(reader, e)?);
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref())?;
                    if tag == "condition" {
                        rule.conditions.push(Condition::from_xml_attrs(e)?);
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref())?;
                    if tag == "rule" {
                        break;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow::anyhow!("XML parse error: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        Ok(rule)
    }
}

/// 生成规则的更新差量 XML 元素（只需 rule_id）
pub fn rule_merge_xml_element(rule_id: &str, patch: &RulePatch) -> String {
    let indent = "    ";
    let child_indent = "        ";
    let mut s = format!("{}<rule id=\"{}\" x:override=\"merge\"", indent, xml_escape(rule_id));

    if let Some(ref name) = patch.name {
        s.push_str(&format!(" name=\"{}\"", xml_escape(name)));
    }
    if let Some(ref desc) = patch.description {
        if let Some(ref d) = desc {
            s.push_str(&format!(" description=\"{}\"", xml_escape(d)));
        }
    }
    if let Some(enabled) = patch.enabled {
        if !enabled {
            s.push_str(" enabled=\"false\"");
        }
    }
    if let Some(ref message) = patch.message {
        s.push_str(&format!(" message=\"{}\"", xml_escape(message)));
    }
    if let Some(timeout) = patch.timeout_seconds {
        s.push_str(&format!(" timeout-seconds=\"{}\"", timeout));
    }
    if let Some(ref channel) = patch.delivery_channel {
        s.push_str(&format!(" delivery-channel=\"{}\"", xml_escape(channel)));
    }
    if let Some(ref target) = patch.delivery_target {
        s.push_str(&format!(" delivery-target=\"{}\"", xml_escape(target)));
    }

    let has_children = patch.conditions.is_some() || patch.groups.is_some();
    if !has_children {
        s.push_str(" />");
    } else {
        s.push('>');
        if let Some(ref conditions) = patch.conditions {
            for cond in conditions {
                s.push('\n');
                s.push_str(&cond.to_xml_element(child_indent));
            }
        }
        if let Some(ref groups) = patch.groups {
            for grp in groups {
                s.push('\n');
                s.push_str(&grp.to_xml_element(child_indent));
            }
        }
        s.push('\n');
        s.push_str(indent);
        s.push_str("</rule>");
    }
    s
}

/// 规则集合
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Rules {
    pub rules: Vec<Rule>,
}

impl Rules {
    /// 序列化为完整 XML
    pub fn to_xml(&self) -> String {
        let mut s = String::from("<Rules xmlns:x=\"/nop/schema/xdsl.xdef\">");
        for rule in &self.rules {
            s.push('\n');
            s.push_str(&rule.to_xml_element());
        }
        if !self.rules.is_empty() {
            s.push('\n');
        }
        s.push_str("</Rules>");
        s
    }

    /// 序列化为带 x:extends 的差量 XML
    pub fn to_delta_xml(&self, extends_file: &str) -> String {
        let mut s = format!(
            "<Rules xmlns:x=\"/nop/schema/xdsl.xdef\" x:extends=\"{}\">",
            xml_escape(extends_file)
        );
        for rule in &self.rules {
            s.push('\n');
            s.push_str(&rule.to_xml_element());
        }
        if !self.rules.is_empty() {
            s.push('\n');
        }
        s.push_str("</Rules>");
        s
    }

    /// 从 XML 字符串反序列化
    pub fn from_xml(xml: &str) -> anyhow::Result<Self> {
        let mut reader = Reader::from_str(xml);

        let mut rules = Vec::new();
        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref())?;
                    if tag == "rule" {
                        rules.push(Rule::from_xml_reader(&mut reader, e)?);
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref())?;
                    if tag == "rule" {
                        // 自闭合的 rule（没有子元素）
                        let mut rule = Rule::default();
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref())?;
                            let val = xml_unescape(std::str::from_utf8(&attr.value)?);
                            match key {
                                "id" => rule.id = val,
                                "name" => rule.name = val,
                                "description" => rule.description = Some(val),
                                "enabled" => rule.enabled = val != "false",
                                "source-id" => rule.source_id = val,
                                "match" => rule.match_mode = MatchMode::from_str(&val).unwrap_or(MatchMode::All),
                                "capture-pre-seconds" => rule.capture_pre_seconds = val.parse().ok(),
                                "message" => rule.message = val,
                                "timeout-seconds" => rule.timeout_seconds = val.parse().unwrap_or(300),
                                "delivery-channel" => rule.delivery_channel = val,
                                "delivery-target" => rule.delivery_target = val,
                                _ => {}
                            }
                        }
                        rules.push(rule);
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow::anyhow!("XML parse error: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        Ok(Rules { rules })
    }
}

/// 规则更新补丁
#[derive(Debug, Clone, Default)]
pub struct RulePatch {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub message: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub delivery_channel: Option<String>,
    pub delivery_target: Option<String>,
    pub conditions: Option<Vec<Condition>>,
    pub groups: Option<Vec<ConditionGroup>>,
}

/// XML 转义特殊字符
pub fn xml_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(c),
        }
    }
    result
}

/// XML 反转义特殊字符
fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_rule_roundtrip() {
        let rule = Rule::new(
            "rule_001".to_string(),
            "测试规则".to_string(),
            "clock".to_string(),
            "执行任务".to_string(),
        );

        let rules = Rules { rules: vec![rule] };
        let xml = rules.to_xml();
        let parsed = Rules::from_xml(&xml).unwrap();

        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].id, "rule_001");
        assert_eq!(parsed.rules[0].name, "测试规则");
        assert_eq!(parsed.rules[0].message, "执行任务");
        assert!(parsed.rules[0].enabled);
    }

    #[test]
    fn test_rule_with_conditions_roundtrip() {
        let mut rule = Rule::new(
            "rule_002".to_string(),
            "带条件规则".to_string(),
            "clock".to_string(),
            "检查天气".to_string(),
        );
        rule.conditions.push(Condition {
            field: "time".to_string(),
            op: Op::CronMatch,
            value: "0 8 * * *".to_string(),
            duration_seconds: None,
        });
        rule.groups.push(ConditionGroup {
            match_mode: MatchMode::Any,
            conditions: vec![
                Condition {
                    field: "temperature".to_string(),
                    op: Op::Gt,
                    value: "30".to_string(),
                    duration_seconds: Some(60),
                },
                Condition {
                    field: "humidity".to_string(),
                    op: Op::Lt,
                    value: "20".to_string(),
                    duration_seconds: None,
                },
            ],
            groups: vec![],
        });

        let rules = Rules { rules: vec![rule] };
        let xml = rules.to_xml();
        let parsed = Rules::from_xml(&xml).unwrap();

        assert_eq!(parsed.rules.len(), 1);
        let r = &parsed.rules[0];
        assert_eq!(r.conditions.len(), 1);
        assert_eq!(r.conditions[0].field, "time");
        assert_eq!(r.conditions[0].op, Op::CronMatch);

        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].match_mode, MatchMode::Any);
        assert_eq!(r.groups[0].conditions.len(), 2);
        assert_eq!(r.groups[0].conditions[0].duration_seconds, Some(60));
    }

    #[test]
    fn test_delta_xml() {
        let rule = Rule::new(
            "rule_004".to_string(),
            "差量规则".to_string(),
            "clock".to_string(),
            "更新".to_string(),
        );
        let rules = Rules { rules: vec![rule] };
        let xml = rules.to_delta_xml("base_rules.xml");

        assert!(xml.contains("x:extends=\"base_rules.xml\""));
    }

    #[test]
    fn test_merge_xml_element() {
        let rule = Rule::new(
            "rule_005".to_string(),
            "原名".to_string(),
            "clock".to_string(),
            "原消息".to_string(),
        );
        let patch = RulePatch {
            name: Some("新名".to_string()),
            message: Some("新消息".to_string()),
            ..Default::default()
        };

        let xml = rule.to_merge_xml_element(&patch);
        assert!(xml.contains("x:override=\"merge\""));
        assert!(xml.contains("name=\"新名\""));
        assert!(xml.contains("message=\"新消息\""));
    }

    #[test]
    fn test_remove_xml_element() {
        let rule = Rule::new(
            "rule_006".to_string(),
            "要删除".to_string(),
            "clock".to_string(),
            "删除".to_string(),
        );

        let xml = rule.to_remove_xml_element();
        assert!(xml.contains("x:override=\"remove\""));
        assert!(xml.contains("id=\"rule_006\""));
    }

    #[test]
    fn test_apply_patch() {
        let mut rule = Rule::new(
            "rule_007".to_string(),
            "原名".to_string(),
            "clock".to_string(),
            "原消息".to_string(),
        );

        let patch = RulePatch {
            name: Some("新名".to_string()),
            enabled: Some(false),
            timeout_seconds: Some(600),
            ..Default::default()
        };

        rule.apply_patch(patch);
        assert_eq!(rule.name, "新名");
        assert!(!rule.enabled);
        assert_eq!(rule.timeout_seconds, 600);
        assert_eq!(rule.message, "原消息"); // 未修改
    }

    #[test]
    fn test_nested_groups() {
        let mut rule = Rule::new(
            "rule_008".to_string(),
            "嵌套组".to_string(),
            "sensor".to_string(),
            "复杂条件".to_string(),
        );
        rule.groups.push(ConditionGroup {
            match_mode: MatchMode::All,
            conditions: vec![Condition {
                field: "a".to_string(),
                op: Op::Eq,
                value: "1".to_string(),
                duration_seconds: None,
            }],
            groups: vec![ConditionGroup {
                match_mode: MatchMode::Any,
                conditions: vec![
                    Condition {
                        field: "b".to_string(),
                        op: Op::Gt,
                        value: "2".to_string(),
                        duration_seconds: None,
                    },
                    Condition {
                        field: "c".to_string(),
                        op: Op::Lt,
                        value: "3".to_string(),
                        duration_seconds: None,
                    },
                ],
                groups: vec![],
            }],
        });

        let rules = Rules { rules: vec![rule] };
        let xml = rules.to_xml();
        let parsed = Rules::from_xml(&xml).unwrap();

        let r = &parsed.rules[0];
        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].groups.len(), 1);
        assert_eq!(r.groups[0].groups[0].match_mode, MatchMode::Any);
        assert_eq!(r.groups[0].groups[0].conditions.len(), 2);
    }

    #[test]
    fn test_xml_escape() {
        let rule = Rule::new(
            "rule_009".to_string(),
            "名称<>&\"'测试".to_string(),
            "clock".to_string(),
            "消息含<特殊>字符".to_string(),
        );

        let rules = Rules { rules: vec![rule] };
        let xml = rules.to_xml();

        assert!(xml.contains("&lt;"));
        assert!(xml.contains("&gt;"));
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&quot;"));

        let parsed = Rules::from_xml(&xml).unwrap();
        assert_eq!(parsed.rules[0].name, "名称<>&\"'测试");
    }
}
