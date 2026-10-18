use freya::query::{QueriesStorage, Query, QueryCapability, UseQuery, use_query};
use oneclient_core::{ClusterError, ConfigFileInfo, LauncherError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConfigFilesKeys {
    pub cluster_id: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConfigFilesQuery;

impl QueryCapability for ConfigFilesQuery {
    type Ok = Vec<ConfigFileInfo>;
    type Err = LauncherError;
    type Keys = ConfigFilesKeys;

    async fn run(&self, keys: &Self::Keys) -> Result<Self::Ok, Self::Err> {
        let state = crate::launcher::state()?;
        let cluster = state.clusters.get(keys.cluster_id).await?;
        run_blocking(move || oneclient_core::list_config_files(&cluster)).await
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConfigFileKeys {
    pub cluster_id: i64,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConfigFileQuery(ConfigFileKeys);

impl QueryCapability for ConfigFileQuery {
    type Ok = String;
    type Err = LauncherError;
    type Keys = ConfigFileKeys;

    fn matches(&self, keys: &Self::Keys) -> bool {
        self.0 == *keys
    }

    async fn run(&self, keys: &Self::Keys) -> Result<Self::Ok, Self::Err> {
        if keys.path.is_empty() {
            return Ok(String::new());
        }
        let state = crate::launcher::state()?;
        let cluster = state.clusters.get(keys.cluster_id).await?;
        let path = keys.path.clone();
        run_blocking(move || oneclient_core::read_config_file(&cluster, &path)).await
    }
}

pub fn use_config_files(cluster_id: i64) -> UseQuery<ConfigFilesQuery> {
    use_query(Query::new(ConfigFilesKeys { cluster_id }, ConfigFilesQuery))
}

pub fn try_config_files(query: &UseQuery<ConfigFilesQuery>) -> Option<Vec<ConfigFileInfo>> {
    super::state::settled_or_loading(query)
}

pub fn use_config_content(cluster_id: i64, path: String) -> UseQuery<ConfigFileQuery> {
    let keys = ConfigFileKeys { cluster_id, path };
    use_query(Query::new(keys.clone(), ConfigFileQuery(keys)))
}

pub fn try_config_content(query: &UseQuery<ConfigFileQuery>) -> Option<String> {
    super::state::settled_or_loading(query)
}

pub async fn write_config_file(
    cluster_id: i64,
    path: String,
    content: String,
) -> Result<(), LauncherError> {
    let state = crate::launcher::state()?;
    let cluster = state.clusters.get(cluster_id).await?;
    run_blocking(move || oneclient_core::write_config_file(&cluster, &path, &content)).await
}

pub async fn invalidate_config_files() {
    QueriesStorage::<ConfigFilesQuery>::invalidate_all().await;
    QueriesStorage::<ConfigFileQuery>::invalidate_all().await;
}

pub async fn invalidate_config_content() {
    QueriesStorage::<ConfigFileQuery>::invalidate_all().await;
}

async fn run_blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, ClusterError> + Send + 'static,
) -> Result<T, LauncherError> {
    Ok(tokio::task::spawn_blocking(task)
        .await
        .map_err(std::io::Error::other)??)
}
