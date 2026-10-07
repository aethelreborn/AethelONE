use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;
use thiserror::Error;

use crate::cluster::Cluster;
use crate::error::ClusterResult;

const SAVES_DIR: &str = "saves";
const DATAPACKS_DIR: &str = "datapacks";
pub const LEVEL_DAT: &str = "level.dat";
pub const WORLD_ICON: &str = "icon.png";
const PACK_META: &str = "pack.mcmeta";
const PACK_ICON: &str = "pack.png";

#[derive(Debug, Error)]
pub enum WorldsError {
    #[error("not a plain file or folder name: {0}")]
    InvalidName(String),
    #[error("world '{0}' not found")]
    NotFound(String),
    #[error("failed to move to trash: {0}")]
    Trash(String),
    #[error("a world named '{0}' already exists")]
    AlreadyExists(String),
    #[error("failed to rename world: {0}")]
    Rename(String),
    #[error("failed to duplicate world: {0}")]
    Duplicate(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldInfo {
    pub folder_name: String,
    pub path: PathBuf,
    pub icon: Option<PathBuf>,
    pub last_played: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PackIcon {
    File(PathBuf),
    Cached(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataPackInfo {
    pub file_name: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub modified: DateTime<Utc>,
    pub description: Option<String>,
    pub icon: Option<PackIcon>,
}

fn plain_name(name: &str) -> ClusterResult<&str> {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(name),
        _ => Err(WorldsError::InvalidName(name.to_string()).into()),
    }
}

fn world_dir(cluster: &Cluster, world: &str) -> ClusterResult<PathBuf> {
    let dir = cluster.game_dir()?.join(SAVES_DIR).join(plain_name(world)?);
    if dir.is_dir() {
        Ok(dir)
    } else {
        Err(WorldsError::NotFound(world.to_string()).into())
    }
}

fn modified_or_now(meta: &std::fs::Metadata) -> DateTime<Utc> {
    meta.modified()
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(|_| Utc::now())
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };

    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => dir_size(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

fn move_to_trash(path: &Path) -> ClusterResult<()> {
    trash::delete(path).map_err(|err| WorldsError::Trash(err.to_string()).into())
}

#[tracing::instrument(level = "debug", skip(cluster), fields(cluster_id = cluster.id))]
pub fn list_cluster_worlds(cluster: &Cluster) -> ClusterResult<Vec<WorldInfo>> {
    let dir = cluster.game_dir()?.join(SAVES_DIR);
    let mut out = Vec::new();

    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(out);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };

        if !path.is_dir() {
            continue;
        }

        let Ok(level_meta) = std::fs::metadata(path.join(LEVEL_DAT)) else {
            continue;
        };

        let icon = path.join(WORLD_ICON);

        out.push(WorldInfo {
            folder_name: name.to_string(),
            icon: icon.is_file().then_some(icon),
            last_played: modified_or_now(&level_meta),
            path,
        });
    }

    out.sort_by_key(|w| std::cmp::Reverse(w.last_played));
    Ok(out)
}

#[tracing::instrument(level = "debug", skip(cluster), fields(cluster_id = cluster.id))]
pub fn world_size(cluster: &Cluster, world: &str) -> ClusterResult<u64> {
    Ok(dir_size(&world_dir(cluster, world)?))
}

#[tracing::instrument(level = "debug", skip(cluster), fields(cluster_id = cluster.id))]
pub fn delete_world(cluster: &Cluster, world: &str) -> ClusterResult<()> {
    move_to_trash(&world_dir(cluster, world)?)
}

fn world_save_dir(cluster: &Cluster, name: &str) -> ClusterResult<PathBuf> {
    Ok(cluster.game_dir()?.join(SAVES_DIR).join(plain_name(name)?))
}

fn world_folder_exists(cluster: &Cluster, name: &str) -> bool {
    world_save_dir(cluster, name).is_ok_and(|dir| dir.is_dir())
}

#[tracing::instrument(level = "debug", skip(cluster), fields(cluster_id = cluster.id))]
pub async fn rename_world(cluster: &Cluster, world: &str, name: &str) -> ClusterResult<()> {
    let from = world_dir(cluster, world)?;
    let to = world_save_dir(cluster, name)?;

    if from == to {
        return Ok(());
    }
    if world_folder_exists(cluster, name) {
        return Err(WorldsError::AlreadyExists(name.to_string()).into());
    }

    polyio::rename(&from, &to)
        .await
        .map_err(|err| WorldsError::Rename(err.to_string()))?;

    Ok(())
}

#[tracing::instrument(level = "debug", skip(cluster), fields(cluster_id = cluster.id))]
pub async fn duplicate_world(cluster: &Cluster, world: &str, name: &str) -> ClusterResult<()> {
    let from = world_dir(cluster, world)?;
    let to = world_save_dir(cluster, name)?;

    if from == to || world_folder_exists(cluster, name) {
        return Err(WorldsError::AlreadyExists(name.to_string()).into());
    }

    polyio::create_dir_all(&to)
        .await
        .map_err(|err| WorldsError::Duplicate(err.to_string()))?;
    polyio::copy_dir(&from, &to, &[])
        .await
        .map_err(|err| WorldsError::Duplicate(err.to_string()))?;

    Ok(())
}

#[tracing::instrument(level = "debug", skip(cluster), fields(cluster_id = cluster.id))]
pub async fn list_world_datapacks(
    cluster: &Cluster,
    world: &str,
) -> ClusterResult<Vec<DataPackInfo>> {
    let dir = world_dir(cluster, world)?.join(DATAPACKS_DIR);
    let mut found = tokio::task::spawn_blocking(move || scan_datapacks(&dir))
        .await
        .map_err(std::io::Error::other)?;

    for (path, info) in &mut found {
        if !info.is_dir {
            (info.description, info.icon) = zip_meta(path).await;
        }
    }

    let mut out: Vec<DataPackInfo> = found.into_iter().map(|(_, info)| info).collect();
    out.sort_by_key(|p| p.file_name.to_lowercase());
    Ok(out)
}

fn scan_datapacks(dir: &Path) -> Vec<(PathBuf, DataPackInfo)> {
    let mut out = Vec::new();

    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };

        let is_dir = meta.is_dir();
        let is_pack = if is_dir {
            path.join(PACK_META).is_file()
        } else {
            name.to_ascii_lowercase().ends_with(".zip")
        };
        if !is_pack {
            continue;
        }

        let info = DataPackInfo {
            file_name: name.to_string(),
            is_dir,
            size_bytes: if is_dir { dir_size(&path) } else { meta.len() },
            modified: modified_or_now(&meta),
            description: if is_dir {
                std::fs::read(path.join(PACK_META))
                    .ok()
                    .and_then(|raw| parse_description(&raw))
            } else {
                None
            },
            icon: is_dir
                .then(|| path.join(PACK_ICON))
                .filter(|icon| icon.is_file())
                .map(PackIcon::File),
        };
        out.push((path, info));
    }

    out
}

#[tracing::instrument(level = "debug", skip(cluster), fields(cluster_id = cluster.id))]
pub async fn add_world_datapacks(
    cluster: &Cluster,
    world: &str,
    files: &[PathBuf],
) -> ClusterResult<()> {
    let dir = world_dir(cluster, world)?.join(DATAPACKS_DIR);
    polyio::create_dir_all(&dir).await?;

    for file in files {
        let Some(name) = file.file_name() else {
            continue;
        };
        let target = dir.join(name);
        if is_same_file(file, &target) {
            continue;
        }
        if file.is_dir() {
            polyio::copy_dir(file, &target, &[]).await?;
        } else {
            polyio::copy(file, &target).await?;
        }
    }

    Ok(())
}

#[tracing::instrument(level = "debug", skip(cluster), fields(cluster_id = cluster.id))]
pub fn delete_world_datapack(cluster: &Cluster, world: &str, file_name: &str) -> ClusterResult<()> {
    let path = world_dir(cluster, world)?
        .join(DATAPACKS_DIR)
        .join(plain_name(file_name)?);
    move_to_trash(&path)
}

fn is_same_file(a: &Path, b: &Path) -> bool {
    match (polyio::canonicalize(a), polyio::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

async fn zip_meta(path: &Path) -> (Option<String>, Option<PackIcon>) {
    let Ok(entries) =
        polyio::read_zip_file_entries(path, |name| name == PACK_META || name == PACK_ICON).await
    else {
        return (None, None);
    };

    let mut description = None;
    let mut icon = None;
    for (name, raw) in entries {
        if name == PACK_META {
            description = parse_description(&raw);
        } else {
            icon = store_icon(&raw).await;
        }
    }
    (description, icon)
}

async fn store_icon(bytes: &[u8]) -> Option<PackIcon> {
    if bytes.is_empty() {
        return None;
    }

    let mut hash = polyio::Sha1Stream::new();
    hash.update(bytes);
    let dir = oneclient_common::paths::local_icons_dir().ok()?;
    let name = format!("{}.png", hash.finish());
    let target = dir.join(&name);
    if !target.is_file() {
        polyio::create_dir_all(&dir).await.ok()?;
        polyio::write(&target, bytes).await.ok()?;
    }
    Some(PackIcon::Cached(format!(
        "{}{name}",
        oneclient_common::paths::LOCAL_IMAGE_SCHEME
    )))
}

fn parse_description(raw: &[u8]) -> Option<String> {
    let raw = raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(raw);
    let meta: Value = serde_json::from_slice(raw).ok()?;
    let text = strip_formatting(flatten_text(meta.get("pack")?.get("description")?).trim());
    (!text.is_empty()).then_some(text)
}

fn flatten_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(flatten_text).collect(),
        Value::Object(map) => {
            let mut out = map.get("text").map(flatten_text).unwrap_or_default();
            if let Some(extra) = map.get("extra") {
                out.push_str(&flatten_text(extra));
            }
            out
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
    }
}

fn strip_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use oneclient_common::domain::GameLoader;
    use oneclient_common::paths::set_data_dir;
    use oneclient_db::models::ClusterKind;

    use crate::cluster::Cluster;
    use crate::error::ClusterError;
    use crate::stage::ClusterStage;

    use super::*;

    static TEST_DIR: OnceLock<PathBuf> = OnceLock::new();

    fn test_root() -> &'static Path {
        TEST_DIR.get_or_init(|| {
            let dir = std::env::temp_dir().join(format!(
                "oneclient-worlds-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            set_data_dir(dir.clone());
            dir
        })
    }

    fn cluster() -> Cluster {
        Cluster {
            id: 1,
            name: "worlds-test".to_string(),
            folder_name: "alpha".to_string(),
            setting_profile_name: None,
            mc_version: "1.20.1".to_string(),
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

    fn saves_dir() -> PathBuf {
        test_root().join("clusters/alpha/saves")
    }

    fn seed_world(name: &str) -> PathBuf {
        let dir = saves_dir().join(name);
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join(LEVEL_DAT), b"level").unwrap();
        std::fs::write(dir.join(WORLD_ICON), b"png").unwrap();
        std::fs::write(dir.join("data/region.dat"), b"regions").unwrap();
        dir
    }

    fn sorted_world_names(cluster: &Cluster) -> Vec<String> {
        let mut names: Vec<String> = list_cluster_worlds(cluster)
            .unwrap()
            .into_iter()
            .map(|w| w.folder_name)
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn rename_moves_the_world_folder_into_a_new_name() {
        let root = test_root();
        std::fs::create_dir_all(root.join("clusters/alpha")).unwrap();
        let seed = seed_world("alpha_world");
        let cluster = cluster();

        rename_world(&cluster, "alpha_world", "beta_world")
            .await
            .expect("rename succeeds");

        assert!(!seed.is_dir());
        let renamed = saves_dir().join("beta_world");
        assert!(renamed.is_dir());
        assert!(renamed.join(LEVEL_DAT).is_file());
        assert!(renamed.join("data/region.dat").is_file());

        assert_eq!(sorted_world_names(&cluster), vec!["beta_world".to_string()]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn duplicate_copies_the_world_folder_under_a_new_name() {
        let root = test_root();
        std::fs::create_dir_all(root.join("clusters/alpha")).unwrap();
        let _seed = seed_world("dup_world");
        let cluster = cluster();

        duplicate_world(&cluster, "dup_world", "dup_world_copy")
            .await
            .expect("duplicate succeeds");

        let copy = saves_dir().join("dup_world_copy");
        assert!(copy.is_dir());
        assert_eq!(std::fs::read(copy.join(LEVEL_DAT)).unwrap(), b"level");
        assert_eq!(
            std::fs::read(copy.join("data/region.dat")).unwrap(),
            b"regions"
        );

        assert_eq!(
            sorted_world_names(&cluster),
            vec!["dup_world".to_string(), "dup_world_copy".to_string()]
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn rename_and_duplicate_refuse_existing_or_invalid_targets() {
        let root = test_root();
        std::fs::create_dir_all(root.join("clusters/alpha")).unwrap();
        seed_world("first");
        seed_world("second");
        let cluster = cluster();

        assert!(matches!(
            rename_world(&cluster, "first", "second").await,
            Err(ClusterError::Worlds(WorldsError::AlreadyExists(_)))
        ));
        assert!(matches!(
            rename_world(&cluster, "first", "a/b").await,
            Err(ClusterError::Worlds(WorldsError::InvalidName(_)))
        ));
        assert!(matches!(
            rename_world(&cluster, "missing", "other").await,
            Err(ClusterError::Worlds(WorldsError::NotFound(_)))
        ));
        assert!(matches!(
            duplicate_world(&cluster, "first", "second").await,
            Err(ClusterError::Worlds(WorldsError::AlreadyExists(_)))
        ));

        rename_world(&cluster, "first", "first")
            .await
            .expect("renaming to the current name is a no-op");

        let _ = std::fs::remove_dir_all(root);
    }
}
