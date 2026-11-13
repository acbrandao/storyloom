use serde::Serialize;

pub type Result<T> = std::result::Result<T, CuratorError>;

/// All errors surfaced by the core. Serializes to a friendly string for the UI.
#[derive(Debug, thiserror::Error)]
pub enum CuratorError {
    #[error("Database error: {0}")]
    Database(#[from] r2d2_sqlite::rusqlite::Error),
    #[error("Database pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("File error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Data error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("{0} was not found. {1}")]
    ToolMissing(String, String),
    #[error("External tool failed: {0}")]
    ToolFailed(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("Safety check failed: {0}")]
    Safety(String),
    #[error("Operation cancelled")]
    Cancelled,
}

impl CuratorError {
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }
}

impl Serialize for CuratorError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
