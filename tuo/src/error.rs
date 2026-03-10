use thiserror::Error;

#[derive(Error, Debug)]
pub enum YError {
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("XML parse error: {0}")]
    XmlParseError(String),

    #[error("JSON parse error: {0}")]
    JsonParseError(#[from] serde_json::Error),

    #[error("{0}: Unsupported tag for {1}")]
    UnsupportedTagError(String, String),

    #[error("{0}")]
    ParseError(String),
}