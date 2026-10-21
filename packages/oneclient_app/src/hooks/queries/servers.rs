use std::time::Duration;

use freya::query::{QueriesStorage, Query, QueryCapability, UseQuery, use_query};
use oneclient_core::LauncherError;
use oneclient_core::servers::{self, SavedServerRow, ServerStatus};

const PING_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SavedServersKeys;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SavedServersQuery;

impl QueryCapability for SavedServersQuery {
    type Ok = Vec<SavedServerRow>;
    type Err = LauncherError;
    type Keys = SavedServersKeys;

    async fn run(&self, _keys: &Self::Keys) -> Result<Self::Ok, Self::Err> {
        let state = crate::launcher::state()?;
        servers::list_saved_servers(&state.services.db).await
    }
}

pub fn use_saved_servers() -> UseQuery<SavedServersQuery> {
    use_query(Query::new(SavedServersKeys, SavedServersQuery))
}

pub fn try_saved_servers(query: &UseQuery<SavedServersQuery>) -> Option<Vec<SavedServerRow>> {
    super::state::settled_or_loading(query)
}

/// `Ok(Err(reason))` is a server that answered badly or not at all an
/// unreadable value only means the row has not finished its first check
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerPingKeys {
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerPingQuery(ServerPingKeys);

impl QueryCapability for ServerPingQuery {
    type Ok = Result<ServerStatus, String>;
    type Err = LauncherError;
    type Keys = ServerPingKeys;

    fn matches(&self, keys: &Self::Keys) -> bool {
        self.0 == *keys
    }

    async fn run(&self, keys: &Self::Keys) -> Result<Self::Ok, Self::Err> {
        let result = servers::ping_server(&keys.address, PING_TIMEOUT)
            .await
            .map_err(|err| err.to_string());
        Ok(result)
    }
}

pub fn use_server_ping(address: String) -> UseQuery<ServerPingQuery> {
    let keys = ServerPingKeys { address };
    use_query(Query::new(keys.clone(), ServerPingQuery(keys)))
}

pub fn try_server_ping(query: &UseQuery<ServerPingQuery>) -> Option<Result<ServerStatus, String>> {
    super::state::settled_or_loading(query)
}

pub async fn invalidate_saved_servers() {
    QueriesStorage::<SavedServersQuery>::invalidate_all().await;
    QueriesStorage::<ServerPingQuery>::invalidate_all().await;
}

pub async fn add_saved_server(address: &str, label: Option<&str>) -> Result<(), LauncherError> {
    let state = crate::launcher::state()?;
    servers::add_saved_server(&state.services.db, address, label).await?;
    invalidate_saved_servers().await;
    Ok(())
}

pub async fn remove_saved_server(id: i64) -> Result<(), LauncherError> {
    let state = crate::launcher::state()?;
    servers::remove_saved_server(&state.services.db, id).await?;
    invalidate_saved_servers().await;
    Ok(())
}

/// Names a saved server, an empty name takes the name away again
pub async fn set_saved_server_label(id: i64, label: Option<&str>) -> Result<(), LauncherError> {
    let state = crate::launcher::state()?;
    servers::set_saved_server_label(&state.services.db, id, label).await?;
    invalidate_saved_servers().await;
    Ok(())
}

pub async fn mark_server_joined(id: i64) -> Result<(), LauncherError> {
    let state = crate::launcher::state()?;
    let at = chrono::Utc::now().to_rfc3339();
    servers::mark_server_joined(&state.services.db, id, &at).await?;
    invalidate_saved_servers().await;
    Ok(())
}
