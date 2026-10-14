use freya::query::{Query, QueryCapability, UseQuery, use_query};
use oneclient_core::LauncherError;
use oneclient_core::clusters::Cluster;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListClustersQuery;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListClustersKeys;

impl QueryCapability for ListClustersQuery {
    type Ok = Vec<Cluster>;
    type Err = LauncherError;
    type Keys = ListClustersKeys;

    async fn run(&self, _keys: &Self::Keys) -> Result<Self::Ok, Self::Err> {
        let state = crate::launcher::state()?;
        Ok(state.clusters.list().await?)
    }
}

pub fn use_clusters() -> UseQuery<ListClustersQuery> {
    use_query(Query::new(ListClustersKeys, ListClustersQuery))
}

/// A lookup not a fetch there is no per-cluster query the list is always
/// loaded and small A hook because it subscribes to the list
pub fn use_cluster(cluster_id: i64) -> Option<Cluster> {
    let clusters = super::state::settled_or_loading(&use_clusters()).unwrap_or_default();
    clusters.into_iter().find(|c| c.id == cluster_id)
}

pub async fn export_instance(
    cluster_id: i64,
    dest: std::path::PathBuf,
) -> Result<(), LauncherError> {
    let state = crate::launcher::state()?;
    let cluster = state.clusters.get(cluster_id).await?;
    Ok(oneclient_core::export_cluster(&cluster, &dest).await?)
}

/// Unpacks a `.zip` that `export_instance` produced into a brand-new instance.
/// Returns the freshly registered cluster so the caller can name it in a toast.
pub async fn import_instance(
    zip: std::path::PathBuf,
    name_override: Option<String>,
) -> Result<Cluster, LauncherError> {
    let state = crate::launcher::state()?;
    let global = state.settings.read().global_game_settings.clone();
    let cluster = state
        .clusters
        .import(&global, &zip, name_override.as_deref())
        .await?;
    state
        .services
        .events
        .signal(oneclient_events::Signal::ClustersChanged);
    Ok(cluster)
}
