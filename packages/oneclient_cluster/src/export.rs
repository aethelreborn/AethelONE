use std::path::Path;

use async_zip::tokio::write::ZipFileWriter;

use crate::cluster::Cluster;
use crate::error::{ClusterError, ClusterResult};
use crate::zipwalk::zip_dir_recursive;

/// Zips the instance folder into `dest`, streaming the contents so a large
/// dedicated instance never has to fit in memory. The archive holds the
/// instance folder: launcher metadata, config, and for dedicated or isolated
/// instances the whole game folder.
#[tracing::instrument(level = "debug", skip(cluster), fields(cluster_id = cluster.id))]
pub async fn export_cluster(cluster: &Cluster, dest: &Path) -> ClusterResult<()> {
    let from = cluster.dir()?;

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| ClusterError::Export(err.to_string()))?;
    }
    if dest.is_file() {
        tokio::fs::remove_file(dest)
            .await
            .map_err(|err| ClusterError::Export(err.to_string()))?;
    }

    let file = tokio::fs::File::create(dest)
        .await
        .map_err(|err| ClusterError::Export(err.to_string()))?;
    let mut writer = ZipFileWriter::with_tokio(file);
    zip_dir_recursive(&mut writer, "", &from)
        .await
        .map_err(ClusterError::Export)?;
    writer
        .close()
        .await
        .map_err(|err| ClusterError::Export(err.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use oneclient_common::domain::GameLoader;
    use oneclient_db::models::ClusterKind;

    use crate::cluster::Cluster;
    use crate::stage::ClusterStage;

    use super::*;

    fn test_root() -> &'static Path {
        crate::test_data::data_dir()
    }

    fn cluster() -> Cluster {
        Cluster {
            id: 9,
            name: "export-test".to_string(),
            folder_name: "export-test".to_string(),
            setting_profile_name: None,
            mc_version: "1.21".to_string(),
            mc_loader: GameLoader::Vanilla,
            mc_loader_version: None,
            stage: ClusterStage::Ready,
            created_at: None,
            last_played: None,
            overall_played: Default::default(),
            linked_modpack_hash: None,
            kind: ClusterKind::Vanilla,
            user_created: true,
            description: None,
            tags: Vec::new(),
            cover_path: None,
        }
    }

    fn seed_instance() {
        let instance = test_root().join("clusters/export-test");
        std::fs::create_dir_all(instance.join("config")).unwrap();
        std::fs::write(instance.join(".instance.json"), b"{}").unwrap();
        std::fs::write(instance.join("config/options.txt"), b"fov:90").unwrap();
    }

    #[tokio::test]
    async fn export_zips_the_instance_folder_and_restores_it() {
        seed_instance();
        let dest = test_root().join("export-test.zip");
        export_cluster(&cluster(), &dest).await.unwrap();

        let out = test_root().join("export-restore");
        polyio::extract_zip(&dest, &out).await.unwrap();
        assert_eq!(std::fs::read(out.join(".instance.json")).unwrap(), b"{}");
        assert_eq!(
            std::fs::read(out.join("config/options.txt")).unwrap(),
            b"fov:90"
        );
    }

    #[tokio::test]
    async fn export_overwrites_a_previous_archive() {
        seed_instance();
        let dest = test_root().join("export-overwrite.zip");
        export_cluster(&cluster(), &dest).await.unwrap();
        export_cluster(&cluster(), &dest).await.unwrap();

        let out = test_root().join("export-overwrite-restore");
        polyio::extract_zip(&dest, &out).await.unwrap();
        assert_eq!(
            std::fs::read(out.join("config/options.txt")).unwrap(),
            b"fov:90"
        );
    }
}
