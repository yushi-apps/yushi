//! 合并算子实现
//! 
//! 实现xdsl.xdef规范定义的合并算子

use std::path::Path;
use crate::YNode;
use crate::error::YError;
use crate::constants::CoreConstants;

/// 合并算子枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideRule {
    /// 合并属性和子节点（默认）
    Merge,
    /// 追加子节点到末尾
    Append,
    /// 前置子节点到开头
    Prepend,
    /// 替换整个节点
    Replace,
    /// 删除节点
    Delete,
}

impl Default for OverrideRule {
    fn default() -> Self {
        OverrideRule::Merge
    }
}

impl OverrideRule {
    /// 从字符串解析合并算子
    pub fn from_str(s: &str) -> Self {
        match s {
            CoreConstants::OVERRIDE_APPEND => OverrideRule::Append,
            CoreConstants::OVERRIDE_PREPEND => OverrideRule::Prepend,
            CoreConstants::OVERRIDE_REPLACE => OverrideRule::Replace,
            CoreConstants::OVERRIDE_DELETE => OverrideRule::Delete,
            _ => OverrideRule::Merge,
        }
    }
}

/// 扩展YNode的合并能力
pub trait YNodeMerge {
    /// 合并两个节点（应用override规则）
    fn merge_with(&self, other: &YNode, rule: OverrideRule) -> YNode;
    
    /// 执行x:extends链式继承
    fn process_extends(&self, base_dir: &Path) -> Result<YNode, YError>;
    
    /// 获取x:override属性值
    fn get_override_rule(&self) -> OverrideRule;
    
    /// 获取x:extends属性值
    fn get_extends_paths(&self) -> Option<Vec<String>>;
}

impl YNodeMerge for YNode {
    fn merge_with(&self, other: &YNode, rule: OverrideRule) -> YNode {
        match rule {
            OverrideRule::Merge => merge_nodes(self, other),
            OverrideRule::Append => append_children(self, other),
            OverrideRule::Prepend => prepend_children(self, other),
            OverrideRule::Replace => other.clone(),
            OverrideRule::Delete => self.clone(), // 标记删除，由调用方处理
        }
    }
    
    fn process_extends(&self, base_dir: &Path) -> Result<YNode, YError> {
        let paths = self.get_extends_paths();
        
        match paths {
            Some(path_list) => {
                let mut result = self.clone();
                for path in path_list {
                    let full_path = base_dir.join(&path);
                    let base_node = YNode::from_xml(full_path.to_str().unwrap_or(""))?;
                    result = base_node.merge_with(&result, OverrideRule::Merge);
                }
                Ok(result)
            }
            None => Ok(self.clone()),
        }
    }
    
    fn get_override_rule(&self) -> OverrideRule {
        self.attr(CoreConstants::X_OVERRIDE)
            .and_then(|v| v.as_string())
            .map(|s| OverrideRule::from_str(&s))
            .unwrap_or_default()
    }
    
    fn get_extends_paths(&self) -> Option<Vec<String>> {
        self.attr(CoreConstants::X_EXTENDS)
            .and_then(|v| v.as_string())
            .map(|s| {
                s.split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            })
    }
}

/// 合并两个节点（merge规则）
fn merge_nodes(base: &YNode, delta: &YNode) -> YNode {
    let mut result = base.clone();
    
    // 合并属性（delta属性覆盖base属性）
    for (key, value) in delta.attributes() {
        result.attributes.insert(key.clone(), value.clone());
    }
    
    // 合并子节点
    for delta_child in delta.children() {
        // 查找是否有同tag的子节点可以合并
        let existing_idx = result.children.iter().position(|c| {
            c.tag_name == delta_child.tag_name && 
            !delta_child.attr(CoreConstants::X_OVERRIDE).is_some()
        });
        
        if let Some(idx) = existing_idx {
            let rule = delta_child.get_override_rule();
            result.children[idx] = result.children[idx].merge_with(delta_child, rule);
        } else {
            result.children.push(delta_child.clone());
        }
    }
    
    // 如果delta有content，覆盖base的content
    if let Some(content) = &delta.content {
        result.content = Some(content.clone());
    }
    
    result
}

/// 追加子节点
fn append_children(base: &YNode, delta: &YNode) -> YNode {
    let mut result = base.clone();
    
    // 合并属性
    for (key, value) in delta.attributes() {
        if key != CoreConstants::X_OVERRIDE {
            result.attributes.insert(key.clone(), value.clone());
        }
    }
    
    // 追加所有子节点
    for child in delta.children() {
        result.children.push(child.clone());
    }
    
    result
}

/// 前置子节点
fn prepend_children(base: &YNode, delta: &YNode) -> YNode {
    let mut result = base.clone();
    
    // 合并属性
    for (key, value) in delta.attributes() {
        if key != CoreConstants::X_OVERRIDE {
            result.attributes.insert(key.clone(), value.clone());
        }
    }
    
    // 前置所有子节点
    let mut new_children = delta.children().clone();
    new_children.extend(result.children.drain(..));
    result.children = new_children;
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::YValue;
    
    #[test]
    fn test_override_rule_from_str() {
        assert_eq!(OverrideRule::from_str("merge"), OverrideRule::Merge);
        assert_eq!(OverrideRule::from_str("append"), OverrideRule::Append);
        assert_eq!(OverrideRule::from_str("prepend"), OverrideRule::Prepend);
        assert_eq!(OverrideRule::from_str("replace"), OverrideRule::Replace);
        assert_eq!(OverrideRule::from_str("delete"), OverrideRule::Delete);
        assert_eq!(OverrideRule::from_str("unknown"), OverrideRule::Merge);
    }
    
    #[test]
    fn test_merge_nodes() {
        let base = YNode::new("root")
            .with_attr("a", YValue::String("1".to_string()))
            .with_attr("b", YValue::String("2".to_string()));
        
        let delta = YNode::new("root")
            .with_attr("b", YValue::String("3".to_string()))
            .with_attr("c", YValue::String("4".to_string()));
        
        let result = base.merge_with(&delta, OverrideRule::Merge);
        
        assert_eq!(result.attr("a").unwrap().as_string().unwrap(), "1");
        assert_eq!(result.attr("b").unwrap().as_string().unwrap(), "3");
        assert_eq!(result.attr("c").unwrap().as_string().unwrap(), "4");
    }
    
    #[test]
    fn test_append_children() {
        let base = YNode::new("root");
        let mut base = base;
        base.add_child(YNode::new("child1"));
        
        let delta = YNode::new("root");
        let mut delta = delta;
        delta.add_child(YNode::new("child2"));
        
        let result = base.merge_with(&delta, OverrideRule::Append);
        
        assert_eq!(result.children.len(), 2);
        assert_eq!(result.children[0].tag_name, "child1");
        assert_eq!(result.children[1].tag_name, "child2");
    }
}
