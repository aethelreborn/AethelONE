//! User-defined instance groups (folders) shown on the Versions page. A group
//! is just a name stored on the cluster row; the module keeps the trimming
//! rules in one place

use oneclient_db::DbPool;
use oneclient_db::dao::cluster_group as group_dao;

use crate::error::LauncherResult;

/// The longest group name the UI offers to type
pub const MAX_GROUP_CHARS: usize = 60;

pub async fn cluster_groups(db: &DbPool) -> LauncherResult<Vec<(i64, String)>> {
    Ok(group_dao::list_assignments(db).await?)
}

/// Moves a cluster into `group`; whitespace-only names mean "no group"
pub async fn set_cluster_group(
    db: &DbPool,
    cluster_id: i64,
    group: Option<String>,
) -> LauncherResult<()> {
    let group = group
        .map(|name| {
            name.trim()
                .chars()
                .take(MAX_GROUP_CHARS)
                .collect::<String>()
        })
        .filter(|name| !name.is_empty());

    Ok(group_dao::set_group(db, cluster_id, group.as_deref()).await?)
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use super::*;

    // Mirrors the columns the group queries touch; the real migration is
    // exercised by the db crate's tests
    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        sqlx::query("CREATE TABLE clusters (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, group_name TEXT)")
            .execute(&pool)
            .await
            .expect("clusters table");
        for name in ["alpha", "beta"] {
            sqlx::query("INSERT INTO clusters (name) VALUES (?)")
                .bind(name)
                .execute(&pool)
                .await
                .expect("seed cluster");
        }
        pool
    }

    #[tokio::test]
    async fn groups_are_trimmed_and_cleared() {
        let pool = pool().await;

        set_cluster_group(&pool, 1, Some("  Survival  ".to_string()))
            .await
            .unwrap();
        set_cluster_group(&pool, 2, Some(" ".to_string()))
            .await
            .unwrap();

        assert_eq!(
            cluster_groups(&pool).await.unwrap(),
            vec![(1, "Survival".to_string())]
        );

        set_cluster_group(&pool, 1, Some("x".repeat(120)))
            .await
            .unwrap();
        let listed = cluster_groups(&pool).await.unwrap();
        assert_eq!(listed[0].1.chars().count(), MAX_GROUP_CHARS);

        set_cluster_group(&pool, 1, None).await.unwrap();
        assert!(cluster_groups(&pool).await.unwrap().is_empty());
    }
}
