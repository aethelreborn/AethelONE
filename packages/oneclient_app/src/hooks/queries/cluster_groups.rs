use std::collections::HashMap;

use freya::query::{QueriesStorage, Query, QueryCapability, UseQuery, use_query};
use oneclient_core::LauncherError;
use oneclient_core::clusters;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClusterGroupsKeys;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClusterGroupsQuery;

impl QueryCapability for ClusterGroupsQuery {
    type Ok = HashMap<i64, String>;
    type Err = LauncherError;
    type Keys = ClusterGroupsKeys;

    async fn run(&self, _keys: &Self::Keys) -> Result<Self::Ok, Self::Err> {
        let state = crate::launcher::state()?;
        let assignments = clusters::cluster_groups(&state.services.db).await?;
        Ok(assignments.into_iter().collect())
    }
}

pub fn use_cluster_groups() -> UseQuery<ClusterGroupsQuery> {
    use_query(Query::new(ClusterGroupsKeys, ClusterGroupsQuery))
}

pub fn try_cluster_groups(query: &UseQuery<ClusterGroupsQuery>) -> Option<HashMap<i64, String>> {
    super::state::settled_or_loading(query)
}

pub async fn set_cluster_group(
    cluster_id: i64,
    group: Option<String>,
) -> Result<(), LauncherError> {
    let state = crate::launcher::state()?;
    clusters::set_cluster_group(&state.services.db, cluster_id, group).await?;
    QueriesStorage::<ClusterGroupsQuery>::invalidate_all().await;
    Ok(())
}
