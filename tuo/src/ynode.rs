use std::{fs::File, io::BufReader};

use hashlink::LinkedHashMap;
use xml::reader::{EventReader, XmlEvent};

use crate::error::YError;
use crate::constants::CoreConstants;
use crate::util::{ValueWithLocation, YValue};

#[derive(Debug, Clone)]
pub struct YNode {
    pub(crate) tag_name: String,
    pub(crate) children: Vec<YNode>,
    pub(crate) content: Option<ValueWithLocation>,
    pub(crate) attributes: LinkedHashMap<String, ValueWithLocation>,
}

impl YNode {
    pub fn new(tag_name: impl Into<String>) -> YNode {
        YNode {
            tag_name: tag_name.into(),
            children: Vec::new(),
            content: None,
            attributes: LinkedHashMap::new(),
        }
    }
    pub fn attr(&self, name: &str) -> Option<&ValueWithLocation> {
        self.attributes.get(name)
    }


    pub fn tag(&self) -> &str {
        &self.tag_name
    }

    pub fn tag_name(&self) -> String {
        self.tag_name.clone()
    }

    pub fn attributes(&self) -> &LinkedHashMap<String, ValueWithLocation> {
        &self.attributes
    }

    pub fn children(&self) -> &Vec<YNode> {
        &self.children
    }

    pub fn attr_count(&self) -> usize {
        self.attributes.len()
    }

    pub fn with_attr(mut self, key: impl Into<String>, value: YValue) -> Self {
        self.attributes.insert(key.into(), ValueWithLocation::new(value, None));
        self
    }

    pub fn with_content(mut self, content: YValue) -> Self {
        self.content = Some(ValueWithLocation::new(content, None));
        self
    }
    
    /// 获取节点内容
    pub fn content(&self) -> Option<&ValueWithLocation> {
        self.content.as_ref()
    }
    
    /// 获取节点内容字符串
    pub fn content_str(&self) -> Option<&str> {
        self.content.as_ref().and_then(|v| {
            if let YValue::String(s) = &v.value {
                Some(s.as_str())
            } else {
                None
            }
        })
    }
    
    pub fn add_child(&mut self, child: YNode) {
        self.children.push(child);
    }

    pub fn has_child(&self) -> bool {
        !self.children.is_empty()
    }   

    pub fn has_body(&self) -> bool {
        return self.has_child() || self.content.is_some();
    }

    pub fn is_text_node(&self) -> bool {
        self.tag_name == CoreConstants::TEXT_TAG_NAME 
    }

    /// 从文件路径解析 XML
    pub fn from_xml(path: &str) -> Result<YNode, YError> {
        let file = File::open(path)?;
        let file = BufReader::new(file);
        Self::parse_xml(EventReader::new(file))
    }

    /// 从字符串解析 XML
    pub fn from_str(xml: &str) -> Result<YNode, YError> {
        Self::parse_xml(EventReader::from_str(xml))
    }

    /// 解析 XML 的核心逻辑
    fn parse_xml<R: std::io::Read>(parser: EventReader<R>) -> Result<YNode, YError> {
        let mut stack: Vec<YNode> = Vec::new();
        let mut root: Option<YNode> = None;
        
        for e in parser {
            match e.map_err(|e| YError::XmlParseError(e.to_string()))? {
                XmlEvent::StartElement { name, attributes, .. } => {
                    let mut node = YNode::new(name.borrow().to_repr());
                    
                    // Add attributes to the node
                    for attr in attributes {
                        node.attributes.insert(
                            attr.name.borrow().to_repr(),
                            ValueWithLocation::new(YValue::String(attr.value), None)
                        );
                    }
                    
                    stack.push(node);
                }
                
                XmlEvent::EndElement { .. } => {
                    let node = stack.pop().ok_or_else(|| YError::XmlParseError("Unexpected end element".to_string()))?;
                    
                    if stack.is_empty() {
                        // This is the root element
                        root = Some(node);
                    } else {
                        // Add this node as a child to its parent
                        let parent_idx = stack.len() - 1;
                        stack[parent_idx].add_child(node);
                    }
                }
                
                XmlEvent::Characters(text) => {
                    if let Some(parent_node) = stack.last_mut() {
                        let text_node = YNode::new(CoreConstants::TEXT_TAG_NAME)
                            .with_content(YValue::String(text));
                        parent_node.add_child(text_node);
                    }
                }
                
                XmlEvent::Whitespace(text) => {
                    // Optionally handle whitespace, for now we ignore it
                    if !text.trim().is_empty() {
                        if let Some(parent_node) = stack.last_mut() {
                            let text_node = YNode::new(CoreConstants::TEXT_TAG_NAME)
                                .with_content(YValue::String(text));
                            parent_node.add_child(text_node);
                        }
                    }
                }
                
                _ => {
                    // Ignore other events like comments, processing instructions, etc.
                }
            }
        }
        
        root.ok_or_else(|| YError::XmlParseError("Empty XML document".to_string()))
    }

    pub fn to_xml(&self) -> String { 
        fn write_node(node: &YNode, indent_level: usize) -> String {
            let indent = "  ".repeat(indent_level);
            let mut result = String::new();

            if node.is_text_node() {
                if let Some(content) =  &node.content {
                    result.push_str(&content.value.to_string());
                }
                return result;
            }

            result.push_str(&format!("{}<{}", indent, node.tag_name));

            for (key, value) in &node.attributes {
                result.push_str(&format!(" {}=\"{}\"", key, value.value.to_string()));
            }

            if !node.has_body() {
                result.push_str("/>\n");
                return result;
            }

            result.push('>');

            if let Some(content) = &node.content {
                result.push_str(&content.value.to_string());
            }

            if !node.children.is_empty() {
                result.push('\n');
                for child in &node.children {
                    result.push_str(&write_node(child, indent_level + 1));
                } 
                result.push_str(&format!("{}</{}>\n", indent, node.tag_name));
            } else {
                result.push_str(&format!("</{}>\n", node.tag_name));
            }
            
            result
        }

        write_node(self, 0)
    }
}

impl std::fmt::Display for YNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn write_node(node: &YNode, f: &mut std::fmt::Formatter<'_>, indent: usize) -> std::fmt::Result {
            // Write indentation
            for _ in 0..indent {
                write!(f, "  ")?;
            }
            
            // Write tag name
            write!(f, "<{}>", node.tag_name)?;
            
            // Write attributes if any
            if !node.attributes.is_empty() {
                write!(f, " {{")?;
                let mut attrs = node.attributes.iter().peekable();
                while let Some((key, value)) = attrs.next() {
                    write!(f, "{}={:?}", key, value.value)?;
                    if attrs.peek().is_some() {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "}}")?;
            }
            
            // Write content if present
            if let Some(content) = &node.content {
                write!(f, " \"{:?}\"", content.value)?;
            }
            
            writeln!(f)?;
            
            // Recursively write children
            for child in &node.children {
                write_node(child, f, indent + 1)?;
            }
            
            Ok(())
        }
        
        write_node(self, f, 0)
    }
}

#[cfg(test)]
mod tests{
    use super::*;

    #[test]
    fn test_from_xml() {
        let device = r#"
            <device name="string" hetu:name="DeviceModel" x:schema="/hetu/schema/hetu.htd" xmlns:x="/hetu/schema/delta.htd" xmlns:hetu="/hetu/schema/hetu.htd">
                <description hetu:value="string"/>
                <connections hetu:body-type="list" hetu:key-attr="name">
                    <connection name="string" hetu:name="DeviceConnectionModel" port="string" baudrate="number" bytesize="number" parity="string" stopbits="number" crlf="string"/>
                </connections>
            </device>
        "#;

        let _ = std::fs::write("./device.xml", device);

        let root = YNode::from_xml("./device.xml").unwrap();
        let tree = format!("{}", root);
        println!("{}", tree);

        assert_eq!(root.tag_name, "device");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.attributes.len(), 3);
        assert_eq!(root.attributes.get("name"), Some(&ValueWithLocation::new(YValue::String("string".to_string()), None)));
        assert_eq!(root.attributes.get("hetu:name"), Some(&ValueWithLocation::new(YValue::String("DeviceModel".to_string()), None)));
        //assert_eq!(tree[root].attributes.get("xmlns:x"), Some(&ValueWithLocation::new(YValue::String("/hetu/schema/delta.htd".to_string()))));
        //assert_eq!(tree[root].attributes.get("xmlns:hetu"), Some(&ValueWithLocation::new(YValue::String("/hetu/schema/hetu.htd".to_string()))));
        assert_eq!(root.attributes.get("x:schema"), Some(&ValueWithLocation::new(YValue::String("/hetu/schema/hetu.htd".to_string()), None)));

        let mut node = root.children[0].clone();
        assert_eq!(node.tag_name, "description");
        assert_eq!(node.attributes.len(), 1);
        assert_eq!(node.attributes.get("hetu:value"), Some(&ValueWithLocation::new(YValue::String("string".to_string()), None)));

        node = root.children[1].clone();
        assert_eq!(node.tag_name, "connections");
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.attributes.len(), 2);
        assert_eq!(node.attributes.get("hetu:body-type"), Some(&ValueWithLocation::new(YValue::String("list".to_string()), None)));
        assert_eq!(node.attributes.get("hetu:key-attr"), Some(&ValueWithLocation::new(YValue::String("name".to_string()), None)));

        node = node.children[0].clone();
        assert_eq!(node.tag_name, "connection");
        assert_eq!(node.children.len(), 0);
        assert_eq!(node.attributes.len(), 8);
        assert_eq!(node.attributes.get("name"), Some(&ValueWithLocation::new(YValue::String("string".to_string()), None)));
        assert_eq!(node.attributes.get("hetu:name"), Some(&ValueWithLocation::new(YValue::String("DeviceConnectionModel".to_string()), None)));
        assert_eq!(node.attributes.get("port"), Some(&ValueWithLocation::new(YValue::String("string".to_string()), None)));
        assert_eq!(node.attributes.get("baudrate"), Some(&ValueWithLocation::new(YValue::String("number".to_string()), None)));
        assert_eq!(node.attributes.get("bytesize"), Some(&ValueWithLocation::new(YValue::String("number".to_string()), None)));
        assert_eq!(node.attributes.get("parity"), Some(&ValueWithLocation::new(YValue::String("string".to_string()), None)));
        assert_eq!(node.attributes.get("stopbits"), Some(&ValueWithLocation::new(YValue::String("number".to_string()), None)));
        assert_eq!(node.attributes.get("crlf"), Some(&ValueWithLocation::new(YValue::String("string".to_string()), None)));

        std::fs::remove_file("./device.xml").unwrap();
    }

    #[test]
    fn test_from_str() {
        // 测试从字符串解析 XML
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<task source-session="test" created-at-ms="123">
    <step tool="Bash">
        <arguments>{"command":"echo hello"}</arguments>
    </step>
</task>"#;

        let root = YNode::from_str(xml).unwrap();
        assert_eq!(root.tag_name, "task");
        assert_eq!(root.attributes.len(), 2);
        assert_eq!(root.attributes.get("source-session"), Some(&ValueWithLocation::new(YValue::String("test".to_string()), None)));
        
        // 检查 step 子节点
        assert_eq!(root.children.len(), 1);
        let step = &root.children[0];
        assert_eq!(step.tag_name, "step");
        assert_eq!(step.attributes.get("tool"), Some(&ValueWithLocation::new(YValue::String("Bash".to_string()), None)));
        
        // 检查 arguments 子节点
        assert_eq!(step.children.len(), 1);
        let args = &step.children[0];
        assert_eq!(args.tag_name, "arguments");
        // 验证文本内容
        assert!(args.has_child());
        if let Some(child) = args.children.first() {
            if let Some(content) = child.content_str() {
                assert!(content.contains("echo hello"));
            }
        }
    }

    #[test]
    fn test_self_closing_tags() {
        // 测试自闭合标签解析
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Memory id="test">
    <toolcall id="3" name="Skill" arguments="{&quot;name&quot;: &quot;weather&quot;}" status="success" />
    <toolcall id="5" name="Bash" arguments="{&quot;cmd&quot;: &quot;curl&quot;}" status="failed" />
</Memory>"#;

        let root = YNode::from_str(xml).unwrap();
        assert_eq!(root.tag_name, "Memory");
        
        // 应该有 2 个 toolcall 子节点（自闭合标签）
        assert_eq!(root.children.len(), 2);
        
        // 检查第一个 toolcall
        let tc1 = &root.children[0];
        assert_eq!(tc1.tag_name, "toolcall");
        assert_eq!(tc1.attr("name").map(|v| v.value.to_string()).unwrap_or_default(), "Skill");
        assert_eq!(tc1.attr("status").map(|v| v.value.to_string()).unwrap_or_default(), "success");
        // XML 实体应该被自动解码
        let args = tc1.attr("arguments").map(|v| v.value.to_string()).unwrap_or_default();
        assert!(args.contains("weather"), "arguments should contain 'weather', got: {}", args);
        
        // 检查第二个 toolcall
        let tc2 = &root.children[1];
        assert_eq!(tc2.tag_name, "toolcall");
        assert_eq!(tc2.attr("status").map(|v| v.value.to_string()).unwrap_or_default(), "failed");
    }
}