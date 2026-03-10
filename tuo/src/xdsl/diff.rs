//! 差量计算模块
//! 
//! 计算两个YNode之间的差量

use crate::YNode;
use crate::error::YError;


/// 差量表示
#[derive(Debug, Clone)]
pub struct YDelta {
    /// 新增的节点
    pub added: Vec<YNode>,
    /// 删除的节点
    pub removed: Vec<YNode>,
    /// 修改的节点
    pub modified: Vec<YModification>,
    /// 差量应用的路径
    pub path: String,
}

impl YDelta {
    /// 创建空的差量
    pub fn new() -> Self {
        YDelta {
            added: Vec::new(),
            removed: Vec::new(),
            modified: Vec::new(),
            path: String::new(),
        }
    }
    
    /// 判断差量是否为空
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }
    
    /// 应用差量到目标节点
    pub fn apply(&self, target: &YNode) -> Result<YNode, YError> {
        let mut result = target.clone();
        
        // 移除被删除的节点
        for removed in &self.removed {
            result.children.retain(|c| !node_matches(c, removed));
        }
        
        // 添加新节点
        for added in &self.added {
            result.children.push(added.clone());
        }
        
        // 应用修改
        for modification in &self.modified {
            if let Some(child) = result.children.iter_mut().find(|c| node_matches(c, &modification.old_value)) {
                *child = modification.new_value.clone();
            }
        }
        
        Ok(result)
    }
}

impl Default for YDelta {
    fn default() -> Self {
        Self::new()
    }
}

/// 节点修改表示
#[derive(Debug, Clone)]
pub struct YModification {
    /// 节点路径
    pub path: String,
    /// 修改前的值
    pub old_value: YNode,
    /// 修改后的值
    pub new_value: YNode,
}

impl YModification {
    /// 创建新的修改表示
    pub fn new(path: String, old_value: YNode, new_value: YNode) -> Self {
        YModification {
            path,
            old_value,
            new_value,
        }
    }
}

/// 计算两个YNode之间的差量
pub fn diff(from: &YNode, to: &YNode) -> Result<YDelta, YError> {
    let mut delta = YDelta::new();
    
    // 如果tag不同，整个节点被视为修改
    if from.tag_name != to.tag_name {
        delta.modified.push(YModification::new(
            String::new(),
            from.clone(),
            to.clone(),
        ));
        return Ok(delta);
    }
    
    // 计算属性差异
    let attr_diff = diff_attributes(from, to);
    if !attr_diff.is_empty() {
        // 创建一个只包含属性差异的修改记录
        let mut old_node = YNode::new(&from.tag_name);
        let mut new_node = YNode::new(&to.tag_name);
        
        for (key, value) in &from.attributes {
            old_node.attributes.insert(key.clone(), value.clone());
        }
        for (key, value) in &to.attributes {
            new_node.attributes.insert(key.clone(), value.clone());
        }
        
        delta.modified.push(YModification::new(
            String::new(),
            old_node,
            new_node,
        ));
    }
    
    // 计算子节点差异
    diff_children(from, to, &mut delta);
    
    Ok(delta)
}

/// 计算属性差异
fn diff_attributes(from: &YNode, to: &YNode) -> Vec<String> {
    let mut changed = Vec::new();
    
    // 检查from中有但to中没有或不同的属性
    for (key, value) in &from.attributes {
        match to.attributes.get(key) {
            None => changed.push(key.clone()),
            Some(to_value) if value != to_value => changed.push(key.clone()),
            _ => {}
        }
    }
    
    // 检查to中有但from中没有的属性
    for key in to.attributes.keys() {
        if !from.attributes.contains_key(key) {
            changed.push(key.clone());
        }
    }
    
    changed
}

/// 计算子节点差异
fn diff_children(from: &YNode, to: &YNode, delta: &mut YDelta) {
    // 使用tag名称和关键属性来匹配节点
    let from_children = &from.children;
    let to_children = &to.children;
    
    // 标记已处理的to节点
    let mut matched_to: Vec<bool> = vec![false; to_children.len()];
    
    // 查找被删除和被修改的节点
    for from_child in from_children {
        let mut found = false;
        
        for (i, to_child) in to_children.iter().enumerate() {
            if matched_to[i] {
                continue;
            }
            
            if nodes_can_match(from_child, to_child) {
                matched_to[i] = true;
                found = true;
                
                // 检查是否修改
                if !nodes_equal(from_child, to_child) {
                    let child_delta = diff(from_child, to_child).unwrap();
                    if !child_delta.is_empty() {
                        delta.modified.push(YModification::new(
                            from_child.tag_name.clone(),
                            from_child.clone(),
                            to_child.clone(),
                        ));
                    }
                }
                break;
            }
        }
        
        if !found {
            // 节点被删除
            delta.removed.push(from_child.clone());
        }
    }
    
    // 查找新增的节点
    for (i, to_child) in to_children.iter().enumerate() {
        if !matched_to[i] {
            delta.added.push(to_child.clone());
        }
    }
}

/// 判断两个节点是否可以匹配（同一节点）
fn nodes_can_match(a: &YNode, b: &YNode) -> bool {
    if a.tag_name != b.tag_name {
        return false;
    }
    
    // 文本节点匹配内容
    if a.is_text_node() && b.is_text_node() {
        return true;
    }
    
    // 尝试通过key属性匹配
    let key_attrs = ["id", "name", "key"];
    for key in &key_attrs {
        if let (Some(a_val), Some(b_val)) = (a.attr(key), b.attr(key)) {
            return a_val == b_val;
        }
    }
    
    // 没有key属性时，假设同一位置的相同tag节点是同一个节点
    true
}

/// 判断两个节点是否相等
fn nodes_equal(a: &YNode, b: &YNode) -> bool {
    if a.tag_name != b.tag_name {
        return false;
    }
    
    if a.attributes != b.attributes {
        return false;
    }
    
    if a.children.len() != b.children.len() {
        return false;
    }
    
    // 比较内容
    match (&a.content, &b.content) {
        (None, None) => {}
        (Some(ac), Some(bc)) if ac == bc => {}
        _ => return false,
    }
    
    // 递归比较子节点
    for (ac, bc) in a.children.iter().zip(b.children.iter()) {
        if !nodes_equal(ac, bc) {
            return false;
        }
    }
    
    true
}

/// 判断节点是否匹配（用于差量应用）
fn node_matches(node: &YNode, pattern: &YNode) -> bool {
    if node.tag_name != pattern.tag_name {
        return false;
    }
    
    // 检查关键属性
    let key_attrs = ["id", "name", "key"];
    for key in &key_attrs {
        match (node.attr(key), pattern.attr(key)) {
            (Some(nv), Some(pv)) if nv != pv => return false,
            _ => {}
        }
    }
    
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::YValue;
    
    #[test]
    fn test_diff_empty() {
        let a = YNode::new("root");
        let b = YNode::new("root");
        
        let delta = diff(&a, &b).unwrap();
        assert!(delta.is_empty());
    }
    
    #[test]
    fn test_diff_attributes() {
        let a = YNode::new("root")
            .with_attr("a", YValue::String("1".to_string()));
        
        let b = YNode::new("root")
            .with_attr("a", YValue::String("2".to_string()));
        
        let delta = diff(&a, &b).unwrap();
        assert!(!delta.is_empty());
    }
    
    #[test]
    fn test_diff_children_added() {
        let a = YNode::new("root");
        
        let mut b = YNode::new("root");
        b.add_child(YNode::new("child"));
        
        let delta = diff(&a, &b).unwrap();
        assert_eq!(delta.added.len(), 1);
        assert_eq!(delta.added[0].tag_name, "child");
    }
    
    #[test]
    fn test_diff_children_removed() {
        let mut a = YNode::new("root");
        a.add_child(YNode::new("child"));
        
        let b = YNode::new("root");
        
        let delta = diff(&a, &b).unwrap();
        assert_eq!(delta.removed.len(), 1);
        assert_eq!(delta.removed[0].tag_name, "child");
    }
    
    #[test]
    fn test_delta_apply() {
        let target = YNode::new("root");
        let delta = YDelta {
            added: vec![YNode::new("child")],
            removed: vec![],
            modified: vec![],
            path: String::new(),
        };
        
        let result = delta.apply(&target).unwrap();
        assert_eq!(result.children.len(), 1);
        assert_eq!(result.children[0].tag_name, "child");
    }
}
