//! Summarizer - 生成历史摘要
//!
//! 负责调用 LLM 生成工具结果摘要和历史摘要

use crate::error::{JieyushaError, Result};
use crate::llm::ModelProfile;
use crate::Registry;

/// 摘要阈值
pub const TARGET_LENGTH: usize = 2000;

/// Summarizer 负责生成历史摘要
#[derive(Debug, Clone)]
pub struct Summarizer {
    model: ModelProfile,
}

impl Summarizer {
    /// 创建新的 Summarizer
    pub fn new() -> Result<Self> {
        let model = Registry::instance()
            .get_model_profile("main")
            .ok_or_else(|| JieyushaError::ConfigError("No main model configured".to_string()))?;
        Ok(Summarizer { model })
    }

    /// 使用指定模型创建 Summarizer
    pub fn with_model(model: ModelProfile) -> Self {
        Summarizer { model }
    }

    /// 生成工具结果摘要
    ///
    /// # 参数
    /// - `tool_name`: 工具名称
    /// - `content`: 工具返回的内容
    /// - `task_context`: 任务上下文（可选）
    pub async fn summarize_tool_result(
        &self,
        tool_name: &str,
        content: &str,
        task_context: Option<&str>,
    ) -> Result<String> {
        // TODO: 调用 LLM 生成摘要
        // 目前简单返回截断的内容
        let context_info = task_context
            .map(|c| format!("\n上下文: {}", c))
            .unwrap_or_default();
        
        if content.len() > TARGET_LENGTH {
            Ok(format!("[{}] {}...{}", tool_name, &content[..TARGET_LENGTH], context_info))
        } else {
            Ok(format!("[{}] {}{}", tool_name, content, context_info))
        }
    }

    /// 生成历史摘要
    ///
    /// # 参数
    /// - `context_text`: 需要摘要的 action 文本
    /// - `intent`: 用户意图
    /// - `count`: 摘要的 action 数量
    pub async fn summarize_actions(
        &self,
        context_text: &str,
        intent: &str,
        count: usize,
    ) -> Result<String> {
        // TODO: 调用 LLM 生成摘要
        // 目前简单返回截断的内容
        Ok(format!(
            "摘要({}个action, 意图: {}): {}",
            count,
            intent,
            if context_text.len() > TARGET_LENGTH {
                &context_text[..TARGET_LENGTH]
            } else {
                context_text
            }
        ))
    }
}

impl Default for Summarizer {
    fn default() -> Self {
        Self::new().expect("Failed to create default Summarizer")
    }
}
