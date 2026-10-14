use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("api error {status}: {message}")]
    Api { status: u16, message: String },

    #[error("mcp error: {0}")]
    Mcp(String),

    #[error("tool not found: {0}")]
    ToolNotFound(String),

    #[error("invalid data: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
