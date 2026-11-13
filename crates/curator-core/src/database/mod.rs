pub mod connection;
pub mod migrations;
pub mod models;
pub mod repositories;

pub use connection::{Database, DbConn};

/// Current time as unix milliseconds.
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
