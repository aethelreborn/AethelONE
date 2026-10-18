use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, PartialEq, FromRow, Serialize, Deserialize)]
pub struct SavedServerRow {
    pub id: i64,
    pub address: String,
    pub label: Option<String>,
    pub created_at: String,
    pub last_joined_at: Option<String>,
}
