//! Mod configs are the half of a modpack the launcher never showed text
//! files the game reads at startup a browser and editor so tuning one does
//! not mean finding the folder by hand
//!
//! Only text files under `config` and a short list of root option files are
//! reachable both because that is what users tune and because every path the
//! UI sends back must resolve inside those roots and never through a symlink

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::cluster::Cluster;
use crate::error::ClusterResult;

pub const CONFIG_DIR: &str = "config";

/// A text editor is not the place for a megabyte of generated json
pub const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Settings files living beside the config folder that modpack guides
/// always send people to
const ROOT_CONFIG_FILES: [&str; 3] = ["options.txt", "optionsof.txt", "iris.properties"];

const TEXT_EXTENSIONS: [&str; 10] = [
    "json",
    "json5",
    "toml",
    "cfg",
    "conf",
    "properties",
    "txt",
    "yaml",
    "yml",
    "ini",
];

#[derive(Debug, thiserror::Error)]
pub enum ConfigsError {
    #[error("not a config file: {0}")]
    InvalidPath(String),
    #[error("config file '{0}' not found")]
    NotFound(String),
    #[error("config file '{0}' is too large to edit")]
    TooLarge(String),
    #[error("config file '{0}' is not text")]
    NotText(String),
}

/// `path` is relative to the cluster folder with `/` separators an identical
/// string goes back into reading and writing
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigFileInfo {
    pub path: String,
    pub size_bytes: u64,
    pub modified: DateTime<Utc>,
}

pub fn list_config_files(cluster: &Cluster) -> ClusterResult<Vec<ConfigFileInfo>> {
    let base = cluster.dir()?;
    let mut out = Vec::new();

    collect_configs(&base, &base.join(CONFIG_DIR), &mut out);

    for name in ROOT_CONFIG_FILES {
        let path = base.join(name);
        if let Ok(meta) = std::fs::symlink_metadata(&path)
            && meta.file_type().is_file()
        {
            push_config(&mut out, &path, &base, &meta);
        }
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

pub fn read_config_file(cluster: &Cluster, rel: &str) -> ClusterResult<String> {
    let base = cluster.dir()?;
    let path = verified(&base, rel)?;
    let meta = std::fs::symlink_metadata(&path)?;

    if meta.len() > MAX_CONFIG_BYTES {
        return Err(ConfigsError::TooLarge(rel.to_string()).into());
    }

    let bytes = std::fs::read(&path)?;
    String::from_utf8(bytes).map_err(|_| ConfigsError::NotText(rel.to_string()).into())
}

pub fn write_config_file(cluster: &Cluster, rel: &str, content: &str) -> ClusterResult<()> {
    if content.len() as u64 > MAX_CONFIG_BYTES {
        return Err(ConfigsError::TooLarge(rel.to_string()).into());
    }

    let base = cluster.dir()?;
    let path = verified(&base, rel)?;

    // Write beside then rename so a full disk or a kill cannot leave the game
    // reading half a config
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|ext| ext.to_str()).unwrap_or("")
    ));
    std::fs::write(&tmp, content)?;

    if let Err(err) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err.into());
    }

    Ok(())
}

/// The game's own video settings live in this file next to the instance
pub const OPTIONS_FILE: &str = "options.txt";

/// Writes the launcher's own video settings into the game's options file
///
/// The game reads the file once at startup and writes back what it read, so a
/// setting that is not written here comes back with the value the game had
/// before the launch
pub fn apply_options_overrides(
    game_dir: &Path,
    overrides: &[(String, String)],
) -> ClusterResult<usize> {
    if overrides.is_empty() {
        return Ok(0);
    }

    let path = game_dir.join(OPTIONS_FILE);

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();

    for (key, value) in overrides {
        let prefix = format!("{key}:");
        match lines.iter_mut().find(|line| line.starts_with(&prefix)) {
            Some(line) => *line = format!("{prefix}{value}"),
            None => lines.push(format!("{prefix}{value}")),
        }
    }

    let mut content = lines.join("\n");
    if !content.ends_with('\n') {
        content.push('\n');
    }

    // Write beside then rename so the game never reads half a file
    let tmp = path.with_extension("txt.tmp");
    std::fs::write(&tmp, content)?;
    if let Err(err) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err.into());
    }

    Ok(overrides.len())
}

fn collect_configs(base: &Path, dir: &Path, out: &mut Vec<ConfigFileInfo>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // Directory symlinks could lead anywhere a real file is followed only
        // when it is directly inside the config tree
        if file_type.is_symlink() {
            continue;
        }

        let path = entry.path();
        if file_type.is_dir() {
            collect_configs(base, &path, out);
        } else if text_file(&path)
            && let Ok(meta) = entry.metadata()
        {
            push_config(out, &path, base, &meta);
        }
    }
}

fn push_config(out: &mut Vec<ConfigFileInfo>, path: &Path, base: &Path, meta: &std::fs::Metadata) {
    if meta.len() > MAX_CONFIG_BYTES {
        return;
    }

    let Ok(rel) = path.strip_prefix(base) else {
        return;
    };

    out.push(ConfigFileInfo {
        path: rel.to_string_lossy().replace('\\', "/"),
        size_bytes: meta.len(),
        modified: meta
            .modified()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now()),
    });
}

fn text_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            TEXT_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// Resolves a path the listing handed out back into the cluster rejecting
/// anything that is not exactly under the config folder or a known root file
/// and anything whose real location escapes those roots through a link
fn verified(base: &Path, rel: &str) -> ClusterResult<PathBuf> {
    let rel = rel.replace('\\', "/");

    let parts: Vec<&str> = rel.split('/').collect();
    if rel.is_empty()
        || rel.starts_with('/')
        || parts
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == "..")
        || rel.contains('\0')
    {
        return Err(ConfigsError::InvalidPath(rel).into());
    }

    let in_config = parts.first().is_some_and(|part| *part == CONFIG_DIR);
    let allowed = if in_config {
        parts.len() > 1
    } else {
        ROOT_CONFIG_FILES.contains(&rel.as_str())
    };
    if !allowed {
        return Err(ConfigsError::InvalidPath(rel).into());
    }

    let path = base.join(&rel);
    let Ok(meta) = std::fs::symlink_metadata(&path) else {
        return Err(ConfigsError::NotFound(rel).into());
    };
    if !meta.file_type().is_file() {
        return Err(ConfigsError::NotFound(rel).into());
    }

    let root = if in_config {
        base.join(CONFIG_DIR)
    } else {
        base.to_path_buf()
    };
    let real_root = root.canonicalize()?;
    let real = path.canonicalize()?;
    if !real.starts_with(&real_root) {
        return Err(ConfigsError::InvalidPath(rel).into());
    }

    Ok(real)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use oneclient_common::domain::GameLoader;

    use super::*;
    use crate::stage::ClusterStage;

    static CONFIGS_LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        CONFIGS_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn base_dir() -> PathBuf {
        crate::test_data::data_dir()
            .join("clusters")
            .join("configs-config")
    }

    fn cluster() -> Cluster {
        Cluster {
            id: 1,
            name: "configs-test".to_string(),
            folder_name: "configs-config".to_string(),
            setting_profile_name: None,
            mc_version: "1.20.1".to_string(),
            mc_loader: GameLoader::Fabric,
            mc_loader_version: None,
            stage: ClusterStage::Ready,
            created_at: None,
            last_played: None,
            overall_played: Default::default(),
            linked_modpack_hash: None,
            kind: oneclient_db::models::ClusterKind::Vanilla,
            user_created: true,
            description: None,
            tags: Vec::new(),
            cover_path: None,
        }
    }

    fn seed() -> Cluster {
        let dir = base_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("config").join("nested")).unwrap();
        std::fs::write(dir.join("config").join("main.toml"), "hello = 1\n").unwrap();
        std::fs::write(dir.join("config").join("nested").join("deep.json"), "{}\n").unwrap();
        std::fs::write(dir.join("config").join("binary.dat"), [0u8, 159]).unwrap();
        std::fs::write(dir.join("options.txt"), "fov:0.5\n").unwrap();
        std::fs::write(dir.join("notes.md"), "not a config\n").unwrap();
        cluster()
    }

    #[test]
    fn lists_nested_configs_and_root_options() {
        let _guard = lock();
        let cluster = seed();
        let names: Vec<String> = list_config_files(&cluster)
            .unwrap()
            .into_iter()
            .map(|file| file.path)
            .collect();

        assert_eq!(
            names,
            vec![
                "config/main.toml".to_string(),
                "config/nested/deep.json".to_string(),
                "options.txt".to_string(),
            ]
        );
    }

    #[test]
    fn reads_text_and_refuses_missing_or_binary_files() {
        let _guard = lock();
        let cluster = seed();

        assert_eq!(
            read_config_file(&cluster, "config/main.toml").unwrap(),
            "hello = 1\n"
        );

        assert!(
            read_config_file(&cluster, "config/nested/deep.json").is_ok(),
            "nested configs are reachable"
        );
        assert!(read_config_file(&cluster, "config/binary.dat").is_err());
        assert!(read_config_file(&cluster, "config/missing.toml").is_err());
    }

    #[test]
    fn writing_replaces_content_and_leaves_no_temp_files() {
        let _guard = lock();
        let cluster = seed();

        write_config_file(&cluster, "config/main.toml", "hello = 2\n").unwrap();
        assert_eq!(
            read_config_file(&cluster, "config/main.toml").unwrap(),
            "hello = 2\n"
        );

        let leftovers: Vec<_> = std::fs::read_dir(base_dir().join("config"))
            .unwrap()
            .flatten()
            .filter(|entry| entry.path().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary files must not survive");
    }

    #[test]
    fn paths_outside_the_roots_are_rejected() {
        let _guard = lock();
        let cluster = seed();

        for rel in [
            "../secret.txt",
            "config/../options.txt",
            "config/../../etc/passwd",
            "/etc/passwd",
            "notes.md",
            "config",
            "",
            "levels.dat",
        ] {
            assert!(
                read_config_file(&cluster, rel).is_err(),
                "{rel} must not be readable"
            );
            assert!(
                write_config_file(&cluster, rel, "x").is_err(),
                "{rel} must not be writable"
            );
        }
    }

    #[test]
    fn symlinked_files_inside_config_stay_unreachable() {
        let _guard = lock();
        let cluster = seed();
        let secret = base_dir().join("secret.txt");
        std::fs::write(&secret, "outside\n").unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&secret, base_dir().join("config").join("leak.txt"))
                .unwrap();
            assert!(read_config_file(&cluster, "config/leak.txt").is_err());
        }
        #[cfg(not(unix))]
        {
            let _ = secret;
        }
    }

    #[test]
    fn oversized_files_are_neither_listed_nor_read() {
        let _guard = lock();
        let cluster = seed();
        let big = base_dir().join("config").join("big.json");
        std::fs::write(&big, vec![b'a'; MAX_CONFIG_BYTES as usize + 1]).unwrap();

        let listed: Vec<String> = list_config_files(&cluster)
            .unwrap()
            .into_iter()
            .map(|file| file.path)
            .collect();
        assert!(!listed.contains(&"config/big.json".to_string()));
        assert!(read_config_file(&cluster, "config/big.json").is_err());
    }

    #[test]
    fn the_video_settings_replace_the_ones_the_game_had() {
        let _guard = lock();
        seed();

        let applied = apply_options_overrides(
            &base_dir(),
            &[
                ("renderDistance".to_string(), "6".to_string()),
                ("particles".to_string(), "minimal".to_string()),
            ],
        )
        .unwrap();

        assert_eq!(applied, 2);
        let written = std::fs::read_to_string(base_dir().join("options.txt")).unwrap();
        assert!(written.contains("renderDistance:6"), "{written}");
        assert!(written.contains("particles:minimal"), "{written}");
        assert!(written.contains("fov:0.5"), "{written}");
    }

    #[test]
    fn a_setting_the_game_never_had_is_added() {
        let _guard = lock();
        seed();

        apply_options_overrides(&base_dir(), &[("cloudStatus".to_string(), "false".to_string())])
            .unwrap();

        let written = std::fs::read_to_string(base_dir().join("options.txt")).unwrap();
        assert!(written.contains("cloudStatus:false"), "{written}");
    }

    #[test]
    fn an_instance_without_an_options_file_gets_one() {
        let _guard = lock();
        seed();
        std::fs::remove_file(base_dir().join("options.txt")).unwrap();

        apply_options_overrides(&base_dir(), &[("graphics".to_string(), "fast".to_string())])
            .unwrap();

        let written = std::fs::read_to_string(base_dir().join("options.txt")).unwrap();
        assert_eq!(written, "graphics:fast\n");
    }

    #[test]
    fn nothing_to_write_leaves_the_file_alone() {
        let _guard = lock();
        seed();

        let applied = apply_options_overrides(&base_dir(), &[]).unwrap();

        assert_eq!(applied, 0);
        assert_eq!(
            std::fs::read_to_string(base_dir().join("options.txt")).unwrap(),
            "fov:0.5\n"
        );
    }
}
