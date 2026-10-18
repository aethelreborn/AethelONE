use sqlx::SqlitePool;

/// Every cluster with a non-empty group, in a stable order the UI can group by
pub async fn list_assignments(pool: &SqlitePool) -> Result<Vec<(i64, String)>, sqlx::Error> {
    sqlx::query_as::<_, (i64, String)>(
        "SELECT id, group_name FROM clusters \
         WHERE group_name IS NOT NULL AND TRIM(group_name) != '' \
         ORDER BY group_name COLLATE NOCASE, name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await
}

/// Moves a cluster into a group, or out of every group when given `None`
pub async fn set_group(
    pool: &SqlitePool,
    cluster_id: i64,
    group: Option<&str>,
) -> Result<(), sqlx::Error> {
    let group = group
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    sqlx::query("UPDATE clusters SET group_name = ? WHERE id = ?")
        .bind(group)
        .bind(cluster_id)
        .execute(pool)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dao::cluster::insert;
    use crate::models::NewCluster;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        sqlx::migrate!().run(&pool).await.expect("migrations run");
        pool
    }

    async fn seed(pool: &SqlitePool, name: &str) -> i64 {
        insert(
            pool,
            &NewCluster {
                name,
                folder_name: name,
                mc_version: "1.21.4",
                mc_loader: 0,
                mc_loader_version: None,
                setting_profile_name: None,
                stage: 0,
                kind: 0,
                user_created: 1,
                description: None,
                tags: "[]",
                cover_path: None,
            },
        )
        .await
        .expect("insert cluster")
        .id
    }

    #[tokio::test]
    async fn assignments_only_list_active_groups() {
        let pool = pool().await;
        let first = seed(&pool, "alpha").await;
        let second = seed(&pool, "beta").await;
        let third = seed(&pool, "gamma").await;

        assert!(list_assignments(&pool).await.unwrap().is_empty());

        set_group(&pool, first, Some("Survival")).await.unwrap();
        set_group(&pool, second, Some("  Creative  "))
            .await
            .unwrap();
        set_group(&pool, third, None).await.unwrap();

        let listed = list_assignments(&pool).await.unwrap();
        assert_eq!(
            listed,
            vec![
                (second, "Creative".to_string()),
                (first, "Survival".to_string())
            ]
        );

        set_group(&pool, first, Some("  ")).await.unwrap();
        let listed = list_assignments(&pool).await.unwrap();
        assert_eq!(listed, vec![(second, "Creative".to_string())]);

        set_group(&pool, second, None).await.unwrap();
        assert!(list_assignments(&pool).await.unwrap().is_empty());
    }
}
