//! Runtime queries: this table arrived after the checked query metadata
//! was generated and there is no sqlx CLI here to regenerate it, so the
//! statements stay runtime-only like the session snapshot queries

use sqlx::SqlitePool;

use crate::models::SavedServerRow;

pub async fn list_all(pool: &SqlitePool) -> Result<Vec<SavedServerRow>, sqlx::Error> {
    sqlx::query_as::<_, SavedServerRow>(
        r#"
		SELECT id, address, label, created_at, last_joined_at
		FROM saved_servers
		ORDER BY COALESCE(last_joined_at, created_at) DESC, id DESC
		"#,
    )
    .fetch_all(pool)
    .await
}

/// Adding an address that already exists keeps the old row and only fills in
/// a label when one was given
pub async fn add(
    pool: &SqlitePool,
    address: &str,
    label: Option<&str>,
) -> Result<SavedServerRow, sqlx::Error> {
    sqlx::query_as::<_, SavedServerRow>(
        r#"
		INSERT INTO saved_servers (address, label)
		VALUES (?, ?)
		ON CONFLICT(address) DO UPDATE SET
			label = COALESCE(excluded.label, saved_servers.label)
		RETURNING id, address, label, created_at, last_joined_at
		"#,
    )
    .bind(address)
    .bind(label)
    .fetch_one(pool)
    .await
}

pub async fn remove(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM saved_servers WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn mark_joined(pool: &SqlitePool, id: i64, at: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE saved_servers SET last_joined_at = ? WHERE id = ?")
        .bind(at)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        sqlx::migrate!().run(&pool).await.expect("migrations run");
        pool
    }

    #[tokio::test]
    async fn servers_round_trip_and_re_adding_keeps_the_row() {
        let pool = pool().await;

        let added = add(&pool, "play.example.com", Some("Main")).await.unwrap();
        assert_eq!(added.address, "play.example.com");
        assert_eq!(added.label.as_deref(), Some("Main"));
        assert!(added.last_joined_at.is_none());

        let renamed = add(&pool, "play.example.com", Some("Renamed"))
            .await
            .unwrap();
        assert_eq!(renamed.id, added.id, "same address reuses the row");
        assert_eq!(renamed.label.as_deref(), Some("Renamed"));

        let kept = add(&pool, "play.example.com", None).await.unwrap();
        assert_eq!(kept.label.as_deref(), Some("Renamed"), "no label keeps it");

        assert_eq!(list_all(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn joined_servers_sort_first_and_removal_is_by_id() {
        let pool = pool().await;

        let first = add(&pool, "a.example.com", None).await.unwrap();
        let second = add(&pool, "b.example.com", None).await.unwrap();

        mark_joined(&pool, first.id, "2026-10-18T12:00:00Z")
            .await
            .unwrap();
        let listed = list_all(&pool).await.unwrap();
        assert_eq!(listed[0].id, first.id, "most recently joined comes first");

        assert!(remove(&pool, second.id).await.unwrap());
        assert!(
            !remove(&pool, second.id).await.unwrap(),
            "idempotent enough"
        );
        assert_eq!(list_all(&pool).await.unwrap().len(), 1);
    }
}
