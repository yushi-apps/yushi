//! Event source module for automation system.
//!
//! This module defines event sources that provide data to the automation system.
//! Built-in `clock` source is always available, users can configure external
//! sources (MQTT/HTTP/File).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 事件源类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Mqtt,
    Http,
    File,
    Internal,
}

impl SourceType {
    /// 转换为 XML 属性值字符串
    fn as_str(&self) -> &'static str {
        match self {
            SourceType::Mqtt => "mqtt",
            SourceType::Http => "http",
            SourceType::File => "file",
            SourceType::Internal => "internal",
        }
    }

    /// 从字符串解析
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mqtt" => Some(SourceType::Mqtt),
            "http" => Some(SourceType::Http),
            "file" => Some(SourceType::File),
            "internal" => Some(SourceType::Internal),
            _ => None,
        }
    }
}

/// 数据格式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DataFormat {
    Json,
    Csv,
    Text,
}

impl DataFormat {
    /// 转换为 XML 属性值字符串
    fn as_str(&self) -> &'static str {
        match self {
            DataFormat::Json => "json",
            DataFormat::Csv => "csv",
            DataFormat::Text => "text",
        }
    }

    /// 从字符串解析
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(DataFormat::Json),
            "csv" => Some(DataFormat::Csv),
            "text" => Some(DataFormat::Text),
            _ => None,
        }
    }
}

/// 事件源定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSource {
    pub id: String,
    pub name: String,
    pub source_type: SourceType,
    /// 连接端点（MQTT broker URL / HTTP URL / 文件路径）
    pub endpoint: Option<String>,
    /// 主题（MQTT topic / HTTP path）
    pub topic: Option<String>,
    /// 数据格式
    pub format: Option<DataFormat>,
    /// 轮询间隔毫秒（HTTP/File 类型使用）
    pub poll_interval_ms: Option<i64>,
}

impl EventSource {
    /// 创建内建时钟事件源
    pub fn clock() -> Self {
        EventSource {
            id: "clock".to_string(),
            name: "系统时钟".to_string(),
            source_type: SourceType::Internal,
            endpoint: None,
            topic: None,
            format: None,
            poll_interval_ms: None,
        }
    }

    /// 判断是否为时钟事件源
    pub fn is_clock(&self) -> bool {
        self.id == "clock"
    }

    /// 序列化为单个 `<source />` XML 元素
    pub fn to_xml_element(&self) -> String {
        let mut attrs = Vec::new();

        attrs.push(format!("id=\"{}\"", xml_escape(&self.id)));
        attrs.push(format!("name=\"{}\"", xml_escape(&self.name)));
        attrs.push(format!("type=\"{}\"", self.source_type.as_str()));

        if let Some(ref endpoint) = self.endpoint {
            attrs.push(format!("endpoint=\"{}\"", xml_escape(endpoint)));
        }
        if let Some(ref topic) = self.topic {
            attrs.push(format!("topic=\"{}\"", xml_escape(topic)));
        }
        if let Some(ref format) = self.format {
            attrs.push(format!("format=\"{}\"", format.as_str()));
        }
        if let Some(poll_interval_ms) = self.poll_interval_ms {
            attrs.push(format!("poll-interval-ms=\"{}\"", poll_interval_ms));
        }

        format!("    <source {} />", attrs.join(" "))
    }
}

/// 事件源集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSources {
    pub sources: Vec<EventSource>,
}

impl EventSources {
    /// 创建包含内建 clock 的默认事件源集合
    pub fn default_with_clock() -> Self {
        EventSources {
            sources: vec![EventSource::clock()],
        }
    }

    /// 序列化为 XML 字符串
    pub fn to_xml(&self) -> String {
        let mut lines = Vec::new();
        lines.push("<EventSources xmlns:x=\"/nop/schema/xdsl.xdef\">".to_string());

        for source in &self.sources {
            lines.push(source.to_xml_element());
        }

        lines.push("</EventSources>".to_string());
        lines.join("\n")
    }

    /// 序列化为带 x:extends 的差量 XML
    pub fn to_delta_xml(&self, extends_file: &str) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "<EventSources xmlns:x=\"/nop/schema/xdsl.xdef\" x:extends=\"{}\">",
            xml_escape(extends_file)
        ));

        for source in &self.sources {
            lines.push(source.to_xml_element());
        }

        lines.push("</EventSources>".to_string());
        lines.join("\n")
    }

    /// 从 XML 字符串反序列化
    pub fn from_xml(xml: &str) -> anyhow::Result<Self> {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);

        let mut sources = Vec::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    if e.name().as_ref() == b"source" {
                        let source = parse_source_element(&e)?;
                        sources.push(source);
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow::anyhow!("XML parse error: {}", e)),
                _ => {}
            }
        }

        Ok(EventSources { sources })
    }
}

/// 解析 `<source>` 元素
fn parse_source_element(
    e: &quick_xml::events::BytesStart<'_>,
) -> anyhow::Result<EventSource> {
    let mut id = String::new();
    let mut name = String::new();
    let mut source_type = SourceType::Internal;
    let mut endpoint = None;
    let mut topic = None;
    let mut format = None;
    let mut poll_interval_ms = None;

    for attr in e.attributes().flatten() {
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = std::str::from_utf8(&attr.value)?;

        match key {
            "id" => id = value.to_string(),
            "name" => name = value.to_string(),
            "type" => {
                source_type = SourceType::from_str(value)
                    .ok_or_else(|| anyhow::anyhow!("Invalid source type: {}", value))?;
            }
            "endpoint" => endpoint = Some(value.to_string()),
            "topic" => topic = Some(value.to_string()),
            "format" => {
                format = Some(
                    DataFormat::from_str(value)
                        .ok_or_else(|| anyhow::anyhow!("Invalid format: {}", value))?,
                );
            }
            "poll-interval-ms" => {
                poll_interval_ms = Some(value.parse::<i64>()?);
            }
            _ => {}
        }
    }

    if id.is_empty() {
        return Err(anyhow::anyhow!("source element missing required 'id' attribute"));
    }
    if name.is_empty() {
        return Err(anyhow::anyhow!("source element missing required 'name' attribute"));
    }

    Ok(EventSource {
        id,
        name,
        source_type,
        endpoint,
        topic,
        format,
        poll_interval_ms,
    })
}

/// XML 转义特殊字符
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 事件数据点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventData {
    /// 事件来源 ID
    pub source_id: String,
    /// 时间戳（毫秒）
    pub timestamp_ms: i64,
    /// 数据字段（field_name → value）
    pub fields: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_source() {
        let clock = EventSource::clock();
        assert_eq!(clock.id, "clock");
        assert_eq!(clock.source_type, SourceType::Internal);
        assert!(clock.is_clock());
    }

    #[test]
    fn test_default_with_clock() {
        let sources = EventSources::default_with_clock();
        assert_eq!(sources.sources.len(), 1);
        assert!(sources.sources[0].is_clock());
    }

    #[test]
    fn test_to_xml() {
        let sources = EventSources {
            sources: vec![
                EventSource::clock(),
                EventSource {
                    id: "sensor_001".to_string(),
                    name: "设备传感器".to_string(),
                    source_type: SourceType::Mqtt,
                    endpoint: Some("mqtt://localhost:1883".to_string()),
                    topic: Some("sensors/device01".to_string()),
                    format: Some(DataFormat::Json),
                    poll_interval_ms: None,
                },
            ],
        };

        let xml = sources.to_xml();
        assert!(xml.contains("xmlns:x=\"/nop/schema/xdsl.xdef\""));
        assert!(xml.contains("id=\"clock\""));
        assert!(xml.contains("type=\"internal\""));
        assert!(xml.contains("id=\"sensor_001\""));
        assert!(xml.contains("type=\"mqtt\""));
        assert!(xml.contains("endpoint=\"mqtt://localhost:1883\""));
    }

    #[test]
    fn test_from_xml() {
        let xml = r#"
        <EventSources xmlns:x="/nop/schema/xdsl.xdef">
            <source id="clock" name="系统时钟" type="internal" />
            <source id="sensor_001" name="设备传感器" type="mqtt"
                    endpoint="mqtt://localhost:1883" topic="sensors/device01"
                    format="json" />
        </EventSources>
        "#;

        let sources = EventSources::from_xml(xml).unwrap();
        assert_eq!(sources.sources.len(), 2);
        assert_eq!(sources.sources[0].id, "clock");
        assert_eq!(sources.sources[0].source_type, SourceType::Internal);
        assert_eq!(sources.sources[1].id, "sensor_001");
        assert_eq!(sources.sources[1].source_type, SourceType::Mqtt);
        assert_eq!(
            sources.sources[1].endpoint,
            Some("mqtt://localhost:1883".to_string())
        );
    }

    #[test]
    fn test_to_delta_xml() {
        let sources = EventSources::default_with_clock();
        let xml = sources.to_delta_xml("base.xml");
        assert!(xml.contains("x:extends=\"base.xml\""));
    }
}
