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
        // 查找是否有匹配的子节点可以合并（基于 tag_name + key 属性）
        let existing_idx = result.children.iter().position(|c| {
            if c.tag_name != delta_child.tag_name {
                return false;
            }
            // 如果 delta_child 有 x:override 属性，走 override 路径（原有逻辑保留）
            if delta_child.attr(CoreConstants::X_OVERRIDE).is_some() {
                return false;
            }
            // key 属性匹配
            let key_attrs = ["id", "name", "key"];
            for key in &key_attrs {
                if let (Some(c_val), Some(d_val)) = (c.attr(key), delta_child.attr(key)) {
                    // 两者都有这个 key 属性，必须值相等
                    return c_val.value == d_val.value;
                }
            }
            // 都没有 key 属性，退化为 tag_name 匹配（原有行为）
            true
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
    
    #[test]
    fn test_merge_nodes_with_key_matching() {
        // 验证同 tag 不同 id 的节点各自保留
        let mut base = YNode::new("root");
        base.add_child(
            YNode::new("toolcall")
                .with_attr("id", YValue::String("tc1".to_string()))
                .with_attr("tool", YValue::String("tool_a".to_string()))
        );
        base.add_child(
            YNode::new("toolcall")
                .with_attr("id", YValue::String("tc2".to_string()))
                .with_attr("tool", YValue::String("tool_b".to_string()))
        );
        
        let mut delta = YNode::new("root");
        delta.add_child(
            YNode::new("toolcall")
                .with_attr("id", YValue::String("tc3".to_string()))
                .with_attr("tool", YValue::String("tool_c".to_string()))
        );
        
        let result = base.merge_with(&delta, OverrideRule::Merge);
        
        // 应该有三个 toolcall 节点，因为 id 不同
        assert_eq!(result.children.len(), 3);
        assert_eq!(result.children[0].attr("id").unwrap().as_string().unwrap(), "tc1");
        assert_eq!(result.children[1].attr("id").unwrap().as_string().unwrap(), "tc2");
        assert_eq!(result.children[2].attr("id").unwrap().as_string().unwrap(), "tc3");
    }
    
    #[test]
    fn test_merge_nodes_same_id_override() {
        // 验证同 tag 同 id 的 delta 节点覆盖 base 节点
        let mut base = YNode::new("root");
        base.add_child(
            YNode::new("toolcall")
                .with_attr("id", YValue::String("tc1".to_string()))
                .with_attr("tool", YValue::String("original_tool".to_string()))
        );
        
        let mut delta = YNode::new("root");
        delta.add_child(
            YNode::new("toolcall")
                .with_attr("id", YValue::String("tc1".to_string()))
                .with_attr("tool", YValue::String("updated_tool".to_string()))
        );
        
        let result = base.merge_with(&delta, OverrideRule::Merge);
        
        // 应该只有一个 toolcall 节点，因为 id 相同，被合并
        assert_eq!(result.children.len(), 1);
        assert_eq!(result.children[0].attr("id").unwrap().as_string().unwrap(), "tc1");
        assert_eq!(result.children[0].attr("tool").unwrap().as_string().unwrap(), "updated_tool");
    }
    
    #[test]
    fn test_merge_nodes_no_key_fallback() {
        // 验证没有 key 属性时退化为原行为（仅按 tag_name 匹配）
        let mut base = YNode::new("root");
        base.add_child(
            YNode::new("child")
                .with_attr("value", YValue::String("base_value".to_string()))
        );
        
        let mut delta = YNode::new("root");
        delta.add_child(
            YNode::new("child")
                .with_attr("value", YValue::String("delta_value".to_string()))
        );
        
        let result = base.merge_with(&delta, OverrideRule::Merge);
        
        // 没有 key 属性，应该按 tag_name 匹配并合并，结果只有一个 child
        assert_eq!(result.children.len(), 1);
        assert_eq!(result.children[0].attr("value").unwrap().as_string().unwrap(), "delta_value");
    }
}
