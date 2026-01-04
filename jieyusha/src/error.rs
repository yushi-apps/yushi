#[derive(Debug, thiserror::Error)]
pub enum JieyushaError {
    #[error("Configuration error: {0}")]
    ConfigError(String),
    /// Underlying error from reqwest library after an API call was made
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error), 
    #[error("LLM Error: {0}")]
    LlmError(String),
    #[error("Tool Error: {0}")]
    ToolError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, JieyushaError>;