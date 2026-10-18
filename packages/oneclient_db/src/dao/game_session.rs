use chrono::Utc;
use sqlx::SqlitePool;

use crate::models::{
    GameSessionRow, GameSessionServerRow, NewGameSession, RecentSessionRow, ServerJoinCount,
    SessionSpan, UnfinishedSession,
};

pub async fn insert_session(
    pool: &SqlitePool,
    new: &NewGameSession<'_>,
) -> Result<GameSessionRow, sqlx::Error> {
    sqlx::query_as!(
        GameSessionRow,
        r#"
		INSERT INTO game_sessions (
			cluster_id, started_at, ram_allocated_mb, mods_enabled, java_vendor, java_version
		)
		VALUES (?, ?, ?, ?, ?, ?)
		RETURNING
			cluster_id, started_at, ended_at, exit_code,
			ram_allocated_mb, mods_enabled, java_vendor, java_version
		"#,
        new.cluster_id,
        new.started_at,
        new.ram_allocated_mb,
        new.mods_enabled,
        new.java_vendor,
        new.java_version
    )
    .fetch_one(pool)
    .await
}

pub async fn finish_session(
    pool: &SqlitePool,
    started_at: &str,
    exit_code: Option<i64>,
) -> Result<(), sqlx::Error> {
    finish_session_at(pool, started_at, &Utc::now().to_rfc3339(), exit_code).await
}

/// For exit times inferred from the game's log rather than observed live
pub async fn finish_session_at(
    pool: &SqlitePool,
    started_at: &str,
    ended_at: &str,
    exit_code: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
		UPDATE game_sessions
		SET ended_at = ?, exit_code = ?
		WHERE started_at = ? AND ended_at IS NULL
		"#,
        ended_at,
        exit_code,
        started_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_session_process(
    pool: &SqlitePool,
    started_at: &str,
    pid: Option<i64>,
    pid_started_at: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
		UPDATE game_sessions
		SET pid = ?, pid_started_at = ?
		WHERE started_at = ?
		"#,
        pid,
        pid_started_at,
        started_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unfinished_sessions(pool: &SqlitePool) -> Result<Vec<UnfinishedSession>, sqlx::Error> {
    sqlx::query_as!(
        UnfinishedSession,
        r#"
		SELECT cluster_id, started_at, pid, pid_started_at
		FROM game_sessions
		WHERE ended_at IS NULL
		ORDER BY started_at ASC
		"#
    )
    .fetch_all(pool)
    .await
}

pub async fn insert_server_join(
    pool: &SqlitePool,
    session_started_at: &str,
    address: &str,
    port: Option<i64>,
) -> Result<String, sqlx::Error> {
    insert_server_join_at(
        pool,
        session_started_at,
        address,
        port,
        &Utc::now().to_rfc3339(),
    )
    .await
}

pub async fn insert_server_join_at(
    pool: &SqlitePool,
    session_started_at: &str,
    address: &str,
    port: Option<i64>,
    joined_at: &str,
) -> Result<String, sqlx::Error> {
    sqlx::query!(
        r#"
		INSERT INTO game_session_servers (session_started_at, address, port, joined_at)
		VALUES (?, ?, ?, ?)
		"#,
        session_started_at,
        address,
        port,
        joined_at
    )
    .execute(pool)
    .await?;
    Ok(joined_at.to_string())
}

pub async fn finish_server(pool: &SqlitePool, joined_at: &str) -> Result<(), sqlx::Error> {
    finish_server_at(pool, joined_at, &Utc::now().to_rfc3339()).await
}

pub async fn finish_server_at(
    pool: &SqlitePool,
    joined_at: &str,
    disconnected_at: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
		UPDATE game_session_servers
		SET disconnected_at = ?
		WHERE joined_at = ? AND disconnected_at IS NULL
		"#,
        disconnected_at,
        joined_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_sessions_for_cluster(
    pool: &SqlitePool,
    cluster_id: i64,
) -> Result<Vec<GameSessionRow>, sqlx::Error> {
    sqlx::query_as!(
        GameSessionRow,
        r#"
		SELECT
			cluster_id, started_at, ended_at, exit_code,
			ram_allocated_mb, mods_enabled, java_vendor, java_version
		FROM game_sessions
		WHERE cluster_id = ?
		ORDER BY started_at DESC
		"#,
        cluster_id
    )
    .fetch_all(pool)
    .await
}

pub async fn list_session_servers(
    pool: &SqlitePool,
    session_started_at: &str,
) -> Result<Vec<GameSessionServerRow>, sqlx::Error> {
    sqlx::query_as!(
        GameSessionServerRow,
        r#"
		SELECT s.session_started_at, s.address, s.port, s.joined_at, s.disconnected_at, g.cluster_id
		FROM game_session_servers s
		JOIN game_sessions g ON g.started_at = s.session_started_at
		WHERE s.session_started_at = ?
		ORDER BY s.joined_at ASC
		"#,
        session_started_at
    )
    .fetch_all(pool)
    .await
}

/// Lets a log replay rewrite a session's server spans from scratch
pub async fn delete_session_servers(
    pool: &SqlitePool,
    session_started_at: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
		DELETE FROM game_session_servers
		WHERE session_started_at = ?
		"#,
        session_started_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn server_join_counts(
    pool: &SqlitePool,
    cluster_id: i64,
) -> Result<Vec<ServerJoinCount>, sqlx::Error> {
    sqlx::query_as!(
        ServerJoinCount,
        r#"
		SELECT s.address AS "address!", COUNT(*) AS "joins!: i64"
		FROM game_session_servers s
		JOIN game_sessions g ON g.started_at = s.session_started_at
		WHERE g.cluster_id = ?
		GROUP BY s.address
		ORDER BY COUNT(*) DESC
		"#,
        cluster_id
    )
    .fetch_all(pool)
    .await
}

/// Finished sessions, newest first, optionally limited to one instance
pub async fn recent_sessions(
    pool: &SqlitePool,
    cluster_id: Option<i64>,
    limit: i64,
) -> Result<Vec<RecentSessionRow>, sqlx::Error> {
    sqlx::query_as::<_, RecentSessionRow>(
        "SELECT s.cluster_id, c.name AS cluster_name, s.started_at, s.ended_at \
         FROM game_sessions s JOIN clusters c ON c.id = s.cluster_id \
         WHERE s.ended_at IS NOT NULL AND (?1 IS NULL OR s.cluster_id = ?1) \
         ORDER BY s.started_at DESC LIMIT ?2",
    )
    .bind(cluster_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn all_session_spans(pool: &SqlitePool) -> Result<Vec<SessionSpan>, sqlx::Error> {
    sqlx::query_as!(
        SessionSpan,
        r#"
		SELECT started_at, ended_at
		FROM game_sessions
		WHERE ended_at IS NOT NULL
		ORDER BY started_at ASC
		"#
    )
    .fetch_all(pool)
    .await
}

pub async fn session_spans_for_cluster(
    pool: &SqlitePool,
    cluster_id: i64,
) -> Result<Vec<SessionSpan>, sqlx::Error> {
    sqlx::query_as!(
        SessionSpan,
        r#"
		SELECT started_at, ended_at
		FROM game_sessions
		WHERE cluster_id = ? AND ended_at IS NOT NULL
		ORDER BY started_at ASC
		"#,
        cluster_id
    )
    .fetch_all(pool)
    .await
}

pub async fn all_session_servers(
    pool: &SqlitePool,
) -> Result<Vec<GameSessionServerRow>, sqlx::Error> {
    sqlx::query_as!(
        GameSessionServerRow,
        r#"
		SELECT s.session_started_at, s.address, s.port, s.joined_at, s.disconnected_at, g.cluster_id
		FROM game_session_servers s
		JOIN game_sessions g ON g.started_at = s.session_started_at
		ORDER BY s.joined_at ASC
		"#
    )
    .fetch_all(pool)
    .await
}

pub async fn session_servers_for_cluster(
    pool: &SqlitePool,
    cluster_id: i64,
) -> Result<Vec<GameSessionServerRow>, sqlx::Error> {
    sqlx::query_as!(
        GameSessionServerRow,
        r#"
		SELECT s.session_started_at, s.address, s.port, s.joined_at, s.disconnected_at, g.cluster_id
		FROM game_session_servers s
		JOIN game_sessions g ON g.started_at = s.session_started_at
		WHERE g.cluster_id = ?
		ORDER BY s.joined_at ASC
		"#,
        cluster_id
    )
    .fetch_all(pool)
    .await
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

    async fn seed_cluster(pool: &SqlitePool, name: &str) -> i64 {
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

    async fn seed_session(pool: &SqlitePool, cluster_id: i64, started: &str, ended: &str) {
        insert_session(
            pool,
            &NewGameSession {
                cluster_id,
                started_at: started,
                ram_allocated_mb: 4096,
                mods_enabled: 0,
                java_vendor: None,
                java_version: None,
            },
        )
        .await
        .expect("insert session");
        finish_session_at(pool, started, ended, Some(0))
            .await
            .expect("finish session");
    }

    #[tokio::test]
    async fn recent_sessions_join_the_instance_name_and_skip_open_ones() {
        let pool = pool().await;
        let survival = seed_cluster(&pool, "Survival").await;
        let creative = seed_cluster(&pool, "Creative").await;

        seed_session(
            &pool,
            survival,
            "2026-10-01T10:00:00+00:00",
            "2026-10-01T11:00:00+00:00",
        )
        .await;
        seed_session(
            &pool,
            creative,
            "2026-10-02T10:00:00+00:00",
            "2026-10-02T10:30:00+00:00",
        )
        .await;
        insert_session(
            &pool,
            &NewGameSession {
                cluster_id: survival,
                started_at: "2026-10-03T10:00:00+00:00",
                ram_allocated_mb: 4096,
                mods_enabled: 0,
                java_vendor: None,
                java_version: None,
            },
        )
        .await
        .expect("open session");

        let all = recent_sessions(&pool, None, 10).await.unwrap();
        assert_eq!(all.len(), 2, "open sessions are still running, not history");
        assert_eq!(all[0].cluster_name, "Creative");
        assert_eq!(all[1].cluster_name, "Survival");

        let only_survival = recent_sessions(&pool, Some(survival), 10).await.unwrap();
        assert_eq!(only_survival.len(), 1);
        assert_eq!(only_survival[0].started_at, "2026-10-01T10:00:00+00:00");

        let capped = recent_sessions(&pool, None, 1).await.unwrap();
        assert_eq!(capped.len(), 1);
        assert_eq!(capped[0].cluster_name, "Creative");
    }
}
