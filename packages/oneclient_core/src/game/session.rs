use std::sync::Arc;

use oneclient_db::DbPool;
use oneclient_db::dao::game_session as session_dao;
use oneclient_db::models::NewGameSession;
use tokio::sync::Mutex;

use crate::state::LauncherState;
use oneclient_content::packages::{ContentType, PackageStore};
use oneclient_java::JavaRuntime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerJoin {
    pub host: String,
    pub port: Option<u16>,
}

/// The session row's `started_at` which is its primary key
pub(crate) type SessionId = String;

pub(crate) fn parse_server_join(line: &str) -> Option<ServerJoin> {
    const MARKER: &str = "Connecting to ";
    let idx = line.find(MARKER)?;
    let rest = line[idx + MARKER.len()..].trim();
    if rest.is_empty() {
        return None;
    }

    let (host, port) = match rest.split_once(", ") {
        Some((host, port)) => {
            let port = port
                .split_whitespace()
                .next()
                .and_then(|p| p.parse::<u16>().ok());
            (host.trim(), port)
        }
        None => (rest, None),
    };

    let host = host.trim();
    if host.is_empty() || host.chars().any(char::is_whitespace) {
        return None;
    }

    Some(ServerJoin {
        host: host.to_string(),
        port,
    })
}

#[derive(Clone)]
pub(crate) struct SessionRecorder {
    session_started_at: String,
    db: DbPool,
    open_server: Arc<Mutex<Option<String>>>,
}

impl SessionRecorder {
    #[tracing::instrument(
        skip(state, java),
        fields(cluster_id, ram_allocated_mb),
        level = "debug"
    )]
    pub(crate) async fn start(
        state: &Arc<LauncherState>,
        cluster_id: i64,
        ram_allocated_mb: u32,
        java: &JavaRuntime,
    ) -> Option<Self> {
        let mods = enabled_mods(state, cluster_id).await;

        let java_vendor = java.vendor.to_string();
        let started_at = chrono::Utc::now().to_rfc3339();
        let new = NewGameSession {
            cluster_id,
            started_at: &started_at,
            ram_allocated_mb: i64::from(ram_allocated_mb),
            mods_enabled: mods.len() as i64,
            java_vendor: Some(java_vendor.as_str()),
            java_version: Some(java.version.as_str()),
        };

        let session = match session_dao::insert_session(&state.services.db, &new).await {
            Ok(row) => row,
            Err(err) => {
                tracing::warn!(cluster_id, error = %err, "failed to record game session");
                return None;
            }
        };

        // Best effort: the row exists and count for analytics without the
        // snapshot; only the next crash's "what changed" line goes missing
        if let Ok(snapshot) = serde_json::to_string(&mods)
            && let Err(err) =
                sqlx::query("UPDATE game_sessions SET mods_snapshot = ? WHERE started_at = ?")
                    .bind(&snapshot)
                    .bind(&session.started_at)
                    .execute(&state.services.db)
                    .await
        {
            tracing::warn!(cluster_id, error = %err, "failed to record the session mod snapshot");
        }

        Some(Self {
            session_started_at: session.started_at,
            db: state.services.db.clone(),
            open_server: Arc::new(Mutex::new(None)),
        })
    }

    /// Re-attaches to a row left open by a previous launcher run so a game that
    /// outlived the launcher keeps recording into the same session
    pub(crate) fn resume(
        db: DbPool,
        session_started_at: SessionId,
        open_server: Option<String>,
    ) -> Self {
        Self {
            session_started_at,
            db,
            open_server: Arc::new(Mutex::new(open_server)),
        }
    }

    /// Playtime is measured against this so it agrees with the session span
    /// analytics reads back out of the row
    pub(crate) fn started_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        chrono::DateTime::parse_from_rfc3339(&self.session_started_at)
            .ok()
            .map(|at| at.with_timezone(&chrono::Utc))
    }

    /// Without this a launcher restart cannot tell a live game from one that
    /// exited unobserved
    #[tracing::instrument(skip(self), level = "debug")]
    pub(crate) async fn record_process(&self, pid: u32, pid_started_at: Option<u64>) {
        if let Err(err) = session_dao::set_session_process(
            &self.db,
            &self.session_started_at,
            Some(i64::from(pid)),
            pid_started_at.map(|t| t as i64),
        )
        .await
        {
            tracing::warn!(pid, error = %err, "failed to record session process");
        }
    }

    pub(crate) async fn observe(&self, line: &str) {
        if let Some(join) = parse_server_join(line) {
            self.open(join).await;
        } else if super::log_replay::is_leave_marker(line) {
            self.close_open().await;
        }
    }

    async fn open(&self, join: ServerJoin) {
        let mut open = self.open_server.lock().await;
        if let Some(prev) = open.take()
            && let Err(err) = session_dao::finish_server(&self.db, &prev).await
        {
            tracing::warn!(error = %err, "failed to close previous server row");
        }

        let port = join.port.map(i64::from);
        match session_dao::insert_server_join(&self.db, &self.session_started_at, &join.host, port)
            .await
        {
            Ok(joined_at) => *open = Some(joined_at),
            Err(err) => tracing::warn!(error = %err, "failed to record server join"),
        }
    }

    async fn close_open(&self) {
        if let Some(prev) = self.open_server.lock().await.take()
            && let Err(err) = session_dao::finish_server(&self.db, &prev).await
        {
            tracing::warn!(error = %err, "failed to close server row");
        }
    }

    /// The time is explicit because an exit is not always observed as it happens
    /// one recovered from a log ended in the past
    #[tracing::instrument(skip(self), fields(exit_code), level = "debug")]
    pub(crate) async fn finish_at(self, ended_at: &str, exit_code: Option<i64>) {
        if let Some(open) = self.open_server.lock().await.take()
            && let Err(err) = session_dao::finish_server_at(&self.db, &open, ended_at).await
        {
            tracing::warn!(error = %err, "failed to close open server on exit");
        }

        if let Err(err) =
            session_dao::finish_session_at(&self.db, &self.session_started_at, ended_at, exit_code)
                .await
        {
            tracing::warn!(session = %self.session_started_at, error = %err, "failed to finish game session");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ModSnapshot {
    pub hash: String,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// Whether a session snapshot says anything a crash notification should carry
/// the question after a crash is always what changed since the game worked
pub(crate) fn describe_changes(previous: &[ModSnapshot], current: &[ModSnapshot]) -> Vec<String> {
    const MAX: usize = 4;

    let mut updates = Vec::new();
    let mut added = Vec::new();
    let mut removed = Vec::new();

    let same = |a: &ModSnapshot, b: &ModSnapshot| a.hash == b.hash;
    let name_of = |m: &ModSnapshot| m.name.to_lowercase();

    for now in current {
        if previous.iter().any(|before| same(before, now)) {
            continue;
        }

        match previous
            .iter()
            .find(|before| name_of(before) == name_of(now))
        {
            Some(before) => match (&before.version, &now.version) {
                (Some(from), Some(to)) if from != to => {
                    updates.push(format!("{} {from} -> {to}", now.name));
                }
                _ => updates.push(format!("{} updated", now.name)),
            },
            None => added.push(format!("{} added", now.name)),
        }
    }

    for before in previous {
        let still_there = current
            .iter()
            .any(|now| same(before, now) || name_of(now) == name_of(before));
        if !still_there {
            removed.push(format!("{} removed", before.name));
        }
    }

    let mut changes = updates;
    changes.extend(added);
    changes.extend(removed);

    if changes.len() > MAX {
        let extra = changes.len() - (MAX - 1);
        changes.truncate(MAX - 1);
        changes.push(format!("and {extra} more"));
    }

    changes
}

/// The mod set of the cluster's most recently finished session `None` when
/// nothing has run since snapshots existed
pub(crate) async fn previous_mod_snapshot(
    db: &DbPool,
    cluster_id: i64,
    before: chrono::DateTime<chrono::Utc>,
) -> Option<Vec<ModSnapshot>> {
    let json = sqlx::query_scalar::<_, String>(
        "SELECT mods_snapshot FROM game_sessions \
         WHERE cluster_id = ? AND started_at < ? AND ended_at IS NOT NULL \
         AND mods_snapshot IS NOT NULL \
         ORDER BY started_at DESC LIMIT 1",
    )
    .bind(cluster_id)
    .bind(before.to_rfc3339())
    .fetch_optional(db)
    .await
    .ok()
    .flatten()?;

    serde_json::from_str(&json).ok()
}

#[tracing::instrument(skip(state), fields(cluster_id), level = "debug")]
pub(crate) async fn enabled_mods(state: &Arc<LauncherState>, cluster_id: i64) -> Vec<ModSnapshot> {
    match PackageStore::list_linked_artifacts(cluster_id, &state.services.content()).await {
        Ok(linked) => linked
            .into_iter()
            .filter(|a| a.enabled && a.content_type == ContentType::Mod)
            .map(|a| ModSnapshot {
                name: a.display_name.unwrap_or(a.file_name),
                version: a.display_version,
                hash: a.hash,
            })
            .collect(),
        Err(err) => {
            tracing::warn!(cluster_id, error = %err, "failed to list mods for the session");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_and_port() {
        let join = parse_server_join(
            "[12:00:00] [Render thread/INFO]: Connecting to mc.hypixel.net, 25565",
        )
        .expect("should parse");
        assert_eq!(join.host, "mc.hypixel.net");
        assert_eq!(join.port, Some(25565));
    }

    #[test]
    fn parses_ip_and_port() {
        let join = parse_server_join("[Render thread/INFO]: Connecting to 192.168.1.10, 25577")
            .expect("should parse");
        assert_eq!(join.host, "192.168.1.10");
        assert_eq!(join.port, Some(25577));
    }

    #[test]
    fn parses_host_without_port() {
        let join = parse_server_join("Connecting to play.example.com").expect("should parse");
        assert_eq!(join.host, "play.example.com");
        assert_eq!(join.port, None);
    }

    #[test]
    fn ignores_unrelated_lines() {
        assert!(parse_server_join("[Render thread/INFO]: Loading world").is_none());
        assert!(parse_server_join("Connecting to ").is_none());
    }

    #[test]
    fn rejects_addresses_with_spaces() {
        assert!(
            parse_server_join("[INFO]: Connecting to voice chat server: 'vc.example.com:24454'")
                .is_none()
        );
    }

    fn mods(entries: &[(&str, &str, Option<&str>)]) -> Vec<ModSnapshot> {
        entries
            .iter()
            .map(|(hash, name, version)| ModSnapshot {
                hash: hash.to_string(),
                name: name.to_string(),
                version: version.map(str::to_string),
            })
            .collect()
    }

    #[test]
    fn changes_name_updates_by_version_and_additions() {
        let previous = mods(&[("a", "Sodium", Some("0.5.8")), ("b", "Iris", Some("1.7"))]);
        let current = mods(&[
            ("c", "Sodium", Some("0.5.9")),
            ("b", "Iris", Some("1.7")),
            ("d", "Jade", Some("11.0")),
        ]);

        assert_eq!(
            describe_changes(&previous, &current),
            vec!["Sodium 0.5.8 -> 0.5.9", "Jade added"]
        );
    }

    #[test]
    fn unchanged_mods_never_show_up() {
        let previous = mods(&[("a", "Sodium", Some("0.5.8"))]);
        let current = mods(&[("a", "Sodium", Some("0.5.8"))]);
        assert!(describe_changes(&previous, &current).is_empty());
    }

    #[test]
    fn removals_count_but_moves_do_not() {
        let previous = mods(&[("a", "Sodium", None), ("b", "Jade", None)]);
        let current = mods(&[("a", "Sodium", None)]);
        assert_eq!(describe_changes(&previous, &current), vec!["Jade removed"]);
    }

    #[test]
    fn a_long_list_is_capped_with_a_tail() {
        let previous: Vec<ModSnapshot> = Vec::new();
        let current = mods(&[
            ("a", "One", None),
            ("b", "Two", None),
            ("c", "Three", None),
            ("d", "Four", None),
            ("e", "Five", None),
            ("f", "Six", None),
        ]);

        let changes = describe_changes(&previous, &current);
        assert_eq!(changes.len(), 4);
        assert_eq!(changes[3], "and 3 more");
    }

    #[test]
    fn snapshots_round_trip_through_json_and_tolerate_missing_versions() {
        let list = mods(&[("a", "Sodium", Some("0.5.8")), ("b", "Jade", None)]);
        let json = serde_json::to_string(&list).expect("serializes");
        let back: Vec<ModSnapshot> = serde_json::from_str(&json).expect("parses");
        assert_eq!(back, list);

        let bare: Vec<ModSnapshot> =
            serde_json::from_str(r#"[{"hash":"c","name":"Bare"}]"#).expect("parses");
        assert_eq!(bare[0].version, None);
    }
}
