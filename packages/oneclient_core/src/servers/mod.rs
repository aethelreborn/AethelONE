//! Saved servers and their live status. The SLP exchange lives in
//! `oneclient_net`; this module owns the persistence side so the UI has one
//! entry point for both

pub use oneclient_db::models::SavedServerRow;
pub use oneclient_net::slp::{DEFAULT_PORT, PingError, ServerStatus, parse_address, ping_server};

use oneclient_db::DbPool;
use oneclient_db::dao::saved_server as server_dao;

use crate::error::LauncherResult;

pub async fn list_saved_servers(db: &DbPool) -> LauncherResult<Vec<SavedServerRow>> {
    Ok(server_dao::list_all(db).await?)
}

/// Normalises `host` and `host:25565` into one stored form so the same server
/// cannot be saved twice under different spellings
pub async fn add_saved_server(
    db: &DbPool,
    address: &str,
    label: Option<&str>,
) -> LauncherResult<SavedServerRow> {
    let (host, port) = parse_address(address)?;

    let stored = if port == DEFAULT_PORT {
        host
    } else {
        format!("{host}:{port}")
    };
    let label = label.map(str::trim).filter(|label| !label.is_empty());

    Ok(server_dao::add(db, &stored, label).await?)
}

pub async fn remove_saved_server(db: &DbPool, id: i64) -> LauncherResult<bool> {
    Ok(server_dao::remove(db, id).await?)
}

pub async fn mark_server_joined(db: &DbPool, id: i64, at: &str) -> LauncherResult<()> {
    Ok(server_dao::mark_joined(db, id, at).await?)
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use super::*;

    // The real migration is exercised by the db crate's tests; here the
    // table is created directly because sqlx::migrate! resolves relative to
    // this crate and would look for a migrations folder that is not ours
    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        sqlx::query(
            "CREATE TABLE saved_servers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                address TEXT NOT NULL UNIQUE,
                label TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_joined_at TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("saved_servers table");
        pool
    }

    #[tokio::test]
    async fn addresses_are_normalised_before_storage() {
        let pool = pool().await;

        let bare = add_saved_server(&pool, "play.example.com", Some("Main"))
            .await
            .unwrap();
        assert_eq!(bare.address, "play.example.com");

        let explicit = add_saved_server(&pool, "play.example.com:25565", None)
            .await
            .unwrap();
        assert_eq!(explicit.id, bare.id, "default port dedupes");

        let custom = add_saved_server(&pool, " play.example.com:25570 ", None)
            .await
            .unwrap();
        assert_eq!(custom.address, "play.example.com:25570");
        assert_eq!(list_saved_servers(&pool).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn junk_addresses_are_rejected() {
        let pool = pool().await;
        assert!(add_saved_server(&pool, "", None).await.is_err());
        assert!(
            add_saved_server(&pool, "host:notaport", None)
                .await
                .is_err()
        );
        assert!(list_saved_servers(&pool).await.unwrap().is_empty());
    }
}
