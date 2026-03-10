//! Workspace 工作目录文件描述
//! 
//! 仅用于向LLM提供上下文信息，不涉及实际文件读写操作
//! 记录工作目录相关文件信息，不记录差量xml文件

use serde::{Deserialize, Serialize};

/// 文件空间描述
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Workspace {
    /// 文件列表
    #[serde(default, rename = "file", skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileEntry>,
}

impl Workspace {
    /// 创建空的 Workspace
    pub fn new() -> Self {
        Workspace {
            files: Vec::new(),
        }
    }
    
    /// 添加文件条目
    pub fn add_file(&mut self, file: FileEntry) {
        self.files.push(file);
    }
    
    /// 添加文件条目（builder模式）
    pub fn with_file(mut self, file: FileEntry) -> Self {
        self.files.push(file);
        self
    }
    
    /// 查找文件条目
    pub fn find_file(&self, name: &str) -> Option<&FileEntry> {
        self.files.iter().find(|f| f.name == name)
    }
    
    /// 查找文件条目（可变）
    pub fn find_file_mut(&mut self, name: &str) -> Option<&mut FileEntry> {
        self.files.iter_mut().find(|f| f.name == name)
    }
    
    /// 移除文件条目
    pub fn remove_file(&mut self, name: &str) -> Option<FileEntry> {
        let idx = self.files.iter().position(|f| f.name == name)?;
        Some(self.files.remove(idx))
    }
    
    /// 合并另一个workspace（用于差量合并）
    pub fn merge(&mut self, other: &Workspace) {
        for file in &other.files {
            if let Some(existing) = self.find_file_mut(&file.name) {
                // 更新描述
                if file.description.is_some() {
                    existing.description = file.description.clone();
                }
            } else {
                self.files.push(file.clone());
            }
        }
    }
}

/// 文件条目（仅记录工作目录相关文件，不记录差量xml文件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// 文件名
    #[serde(rename = "@name")]
    pub name: String,
    /// 文件用途说明
    #[serde(rename = "@description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl FileEntry {
    /// 创建新的文件条目
    pub fn new(name: impl Into<String>) -> Self {
        FileEntry {
            name: name.into(),
            description: None,
        }
    }
    
    /// 创建带描述的文件条目
    pub fn with_description(name: impl Into<String>, description: impl Into<String>) -> Self {
        FileEntry {
            name: name.into(),
            description: Some(description.into()),
        }
    }
    
    /// 设置描述
    pub fn set_description(&mut self, description: impl Into<String>) {
        self.description = Some(description.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_workspace_new() {
        let ws = Workspace::new();
        assert!(ws.files.is_empty());
    }
    
    #[test]
    fn test_workspace_add_file() {
        let mut ws = Workspace::new();
        let file = FileEntry::new("test.txt");
        ws.add_file(file);
        
        assert_eq!(ws.files.len(), 1);
        assert!(ws.find_file("test.txt").is_some());
    }
    
    #[test]
    fn test_file_entry() {
        let file = FileEntry::new("test.txt");
        assert_eq!(file.name, "test.txt");
        assert!(file.description.is_none());
    }
    
    #[test]
    fn test_file_entry_with_description() {
        let file = FileEntry::with_description("test.txt", "测试文件");
        assert_eq!(file.description, Some("测试文件".to_string()));
    }
}
