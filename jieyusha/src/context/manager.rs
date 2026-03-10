//! ContextManager - 上下文管理器
//! 
//! 提供基于 AgentContext 的管理功能
//! 支持差量链加载和合并

use std::path::{Path, PathBuf};
use std::fs;
use uuid::Uuid;

use crate::context::{AgentContext, Action, ActionResult, ContextStatus};
use crate::error::{JieyushaError, Result};

/// AgentContext 管理器
pub struct ContextManager {
    /// 当前的 AgentContext
    pub(crate) context: AgentContext,
    /// 上下文存储路径（history目录）
    pub(crate) history_path: PathBuf,
    /// 差量计数器
    delta_counter: u32,
}

impl ContextManager {
    /// 创建新的上下文管理器
    /// 
    /// # 参数
    /// - `intent`: 用户意图
    /// - `root_path`: App::root_path() 返回的根目录
    pub fn new(intent: impl Into<String>, root_path: impl Into<PathBuf>) -> Result<Self> {
        let root_path = root_path.into();
        let history_path = root_path.join("history");
        
        // 确保目录存在
        fs::create_dir_all(&history_path)
            .map_err(|e| JieyushaError::IoError(e))?;
        
        let context = AgentContext::new(intent);
        
        // 保存base.xml
        let base_path = history_path.join("base.xml");
        context.save(&base_path)?;
        
        Ok(ContextManager {
            context,
            history_path,
            delta_counter: 0,
        })
    }
    
    /// 从history目录加载上下文（合并所有差量）
    pub fn load(history_path: impl Into<PathBuf>) -> Result<Self> {
        let history_path = history_path.into();
        
        let base_path = history_path.join("base.xml");
        if !base_path.exists() {
            return Err(JieyushaError::ConfigError(
                format!("Base file not found: {:?}", base_path)
            ));
        }
        
        // 收集所有差量文件
        let mut delta_files: Vec<(u32, PathBuf)> = Vec::new();
        for entry in fs::read_dir(&history_path)
            .map_err(|e| JieyushaError::IoError(e))?
        {
            let entry = entry.map_err(|e| JieyushaError::IoError(e))?;
            let path = entry.path();
            
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".xml") && name != "base.xml" && name != "current.xml" {
                    // 解析序号
                    if let Some(num_str) = name.split('_').next() {
                        if let Ok(num) = num_str.parse::<u32>() {
                            delta_files.push((num, path));
                        }
                    }
                }
            }
        }
        
        // 按序号排序
        delta_files.sort_by_key(|(num, _)| *num);
        
        // 使用load_with_deltas加载
        let delta_paths: Vec<PathBuf> = delta_files.into_iter().map(|(_, p)| p).collect();
        let context = AgentContext::load_with_deltas(&base_path, &delta_paths)?;
        let delta_counter = delta_paths.len() as u32;
        
        Ok(ContextManager {
            context,
            history_path,
            delta_counter,
        })
    }
    
    /// 获取上下文引用
    pub fn context(&self) -> &AgentContext {
        &self.context
    }
    
    /// 获取上下文可变引用
    pub fn context_mut(&mut self) -> &mut AgentContext {
        &mut self.context
    }
    
    /// 添加 thought action 并创建差量
    pub fn add_thought(&mut self, content: impl Into<String>) -> Result<String> {
        let action_id = Uuid::new_v4().to_string();
        let action = Action::thought(&action_id, content);
        self.context.add_action(action.clone());
        self.create_delta_file(&action)?;
        Ok(action_id)
    }
    
    /// 添加工具调用 action 并创建差量
    pub fn add_tool_call(
        &mut self,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Result<String> {
        let action_id = Uuid::new_v4().to_string();
        // 使用 tool 函数创建一个只有参数、没有结果的 action
        let action = Action::tool(&action_id, tool_name, arguments, ActionResult::ok(""));
        self.context.add_action(action.clone());
        self.create_delta_file(&action)?;
        Ok(action_id)
    }
    
    /// 添加工具结果 action 并创建差量
    pub fn add_tool_result(
        &mut self,
        tool_name: impl Into<String>,
        result: ActionResult,
    ) -> Result<String> {
        let action_id = Uuid::new_v4().to_string();
        // 使用 tool 函数创建一个只有结果的 action
        let action = Action::tool(&action_id, tool_name, serde_json::Value::Null, result);
        self.context.add_action(action.clone());
        self.create_delta_file(&action)?;
        Ok(action_id)
    }
    
    /// 添加系统消息 action 并创建差量
    pub fn add_system_message(&mut self, content: impl Into<String>) -> Result<String> {
        let action_id = Uuid::new_v4().to_string();
        let action = Action::system_message(&action_id, content);
        self.context.add_action(action.clone());
        self.create_delta_file(&action)?;
        Ok(action_id)
    }
    
    /// 创建差量文件
    /// 
    /// 使用 xdsl 规范创建差量：
    /// - 添加 xmlns:x="xdsl.xdef" 命名空间
    /// - 第一个差量继承 base.xml，后续差量继承前一个差量
    /// - history 使用 x:override="append"
    fn create_delta_file(&mut self, action: &Action) -> Result<PathBuf> {
        self.delta_counter += 1;
        
        let filename = format!("{:03}_{}.xml", self.delta_counter, action.id);
        let delta_path = self.history_path.join(&filename);
        
        // 确定继承路径
        let extends_path = if self.delta_counter == 1 {
            "base.xml".to_string()
        } else {
            // 查找前一个差量文件
            let mut prev_file = "base.xml".to_string();
            if self.history_path.exists() {
                if let Ok(entries) = std::fs::read_dir(&self.history_path) {
                    let mut delta_files: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .filter_map(|e| {
                            let name = e.file_name().to_string_lossy().to_string();
                            if name.ends_with(".xml") && 
                               name != "base.xml" && 
                               name != "current.xml" {
                                Some(name)
                            } else {
                                None
                            }
                        })
                        .collect();
                    delta_files.sort();
                    if let Some(last) = delta_files.last() {
                        prev_file = last.clone();
                    }
                }
            }
            prev_file
        };
        
        // 创建差量 Memory
        let mut delta = AgentContext::new("");
        delta.id = self.context.id.clone();
        delta.version = self.context.version;
        // 添加 xdsl 命名空间和链式继承
        delta.xmlns_x = Some("xdsl.xdef".to_string());
        delta.x_extends = Some(extends_path);
        // history 使用 x:override="append"
        delta.history = crate::context::ActionList::with_override("append");
        delta.history.push(action.clone());
        
        delta.save(&delta_path)?;
        
        // 更新 current.xml
        self.save()?;
        
        Ok(delta_path)
    }
    
    /// 保存上下文到current.xml
    pub fn save(&self) -> Result<()> {
        let current_path = self.history_path.join("current.xml");
        self.context.save(&current_path)
    }
    
    /// 获取最近 N 个 actions
    pub fn get_recent_actions(&self, count: usize) -> Vec<&Action> {
        self.context.get_recent_actions(count)
    }
    
    /// 获取 current 中的 action IDs
    pub fn get_current_ids(&self) -> Vec<&String> {
        self.context.current.iter().map(|r| &r.id).collect()
    }
    
    /// 更新上下文状态
    pub fn set_status(&mut self, status: ContextStatus) {
        self.context.set_status(status);
    }
    
    /// 获取history目录路径
    pub fn history_path(&self) -> &Path {
        &self.history_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_context_manager_new() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new("测试任务", temp_dir.path()).unwrap();
        
        assert_eq!(manager.context().intent, "测试任务");
        assert_eq!(manager.context().history.len(), 0);
        
        // 检查base.xml是否存在
        let base_path = manager.history_path().join("base.xml");
        assert!(base_path.exists());
    }
    
    #[test]
    fn test_context_manager_add_actions() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = ContextManager::new("测试", temp_dir.path()).unwrap();
        
        manager.add_thought("思考中...").unwrap();
        manager.add_tool_call("bash", serde_json::json!({"command": "ls"})).unwrap();
        manager.add_tool_result("bash", ActionResult::ok("成功")).unwrap();
        
        assert_eq!(manager.context().history.len(), 3);
    }
    
    #[test]
    fn test_context_manager_save_load() {
        let temp_dir = TempDir::new().unwrap();
        
        // 创建并添加actions
        let mut manager = ContextManager::new("测试任务", temp_dir.path()).unwrap();
        manager.add_thought("思考").unwrap();
        
        // 加载
        let loaded = ContextManager::load(manager.history_path()).unwrap();
        assert_eq!(loaded.context().intent, "测试任务");
        assert_eq!(loaded.context().history.len(), 1);
    }
}

