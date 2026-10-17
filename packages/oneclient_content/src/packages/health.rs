//! Offline health scan of an instance's mods folder.
//!
//! Every finding is derivable from the jars on disk alone: two jars claiming
//! the same mod id break the game before it draws a frame, a jar built for a
//! different loader is silently ignored, a missing required dependency
//! usually ends in a mixin crash naming an absent class, and a jar without
//! any mod metadata was likely never meant to be here. Catching these in the
//! launcher beats reading a crash report.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use oneclient_common::domain::GameLoader;
use serde_json::Value;

use super::local_manifest::{
    MANIFESTS, loader_from_entries, mod_id_from_entries, read_manifest_entries,
};

/// Ids the game or the loader itself provides; a dependency on one of these
/// is never "missing".
pub const ENV_MOD_IDS: [&str; 7] = [
    "minecraft",
    "java",
    "fabricloader",
    "fabric",
    "forge",
    "neoforge",
    "quilt_loader",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthIssue {
    /// Two or more jars declare the same mod id; refuses to start.
    DuplicateMod { id: String, files: Vec<String> },
    /// The jar targets a loader the instance does not run.
    WrongLoader { file: String, loader: GameLoader },
    /// A required dependency is not among the installed mods.
    MissingDependency {
        id: String,
        required_by: Vec<String>,
    },
    /// The jar could not be opened as a zip.
    Unreadable { file: String },
    /// The jar has none of the known mod metadata files.
    NoMetadata { file: String },
}

impl HealthIssue {
    /// Stable ordering: hard failures first, then by identifier.
    #[must_use]
    pub fn sort_key(&self) -> (u8, String) {
        match self {
            Self::DuplicateMod { id, .. } => (0, id.clone()),
            Self::WrongLoader { file, .. } => (1, file.clone()),
            Self::MissingDependency { id, .. } => (2, id.clone()),
            Self::Unreadable { file } => (3, file.clone()),
            Self::NoMetadata { file } => (4, file.clone()),
        }
    }

    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Self::DuplicateMod { .. } => "Duplicate mod id".to_string(),
            Self::WrongLoader { .. } => "Wrong loader".to_string(),
            Self::MissingDependency { .. } => "Missing dependency".to_string(),
            Self::Unreadable { .. } => "Unreadable jar".to_string(),
            Self::NoMetadata { .. } => "Not a mod".to_string(),
        }
    }

    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::DuplicateMod { id, files } => format!(
                "\"{id}\" is provided by {} jars — Minecraft refuses to start until only one is left: {}",
                files.len(),
                files.join(", ")
            ),
            Self::WrongLoader { file, loader } => format!(
                "{file} is a {loader} mod, which this instance cannot load; it is ignored by the game"
            ),
            Self::MissingDependency { id, required_by } => format!(
                "\"{id}\" is required but not installed — needed by {}",
                required_by.join(", ")
            ),
            Self::Unreadable { file } => format!(
                "{file} could not be opened as a zip; it is likely corrupted or not a mod jar"
            ),
            Self::NoMetadata { file } => format!(
                "{file} has no mod metadata (fabric.mod.json / mods.toml), so the game ignores it — usually a wrongly placed file"
            ),
        }
    }
}

/// What one jar in the mods folder declares. Reading is separate from the
/// analysis so the analysis stays a pure function.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JarFacts {
    pub file: String,
    pub unreadable: bool,
    pub id: Option<String>,
    pub loader: Option<GameLoader>,
    pub required_deps: Vec<String>,
}

/// Scans every `.jar` in `mods_dir` and reports the issues that are provable
/// from the files alone. A missing folder scans as healthy.
pub async fn scan_mods_dir(mods_dir: &Path, cluster_loader: GameLoader) -> Vec<HealthIssue> {
    let mut jars = Vec::new();
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file = entry.file_name().to_string_lossy().into_owned();
        if !path.is_file() || !file.to_ascii_lowercase().ends_with(".jar") {
            continue;
        }
        jars.push(read_jar_facts(&path).await);
    }
    jars.sort_by(|left, right| left.file.cmp(&right.file));
    scan_mod_health(cluster_loader, &jars)
}

pub async fn read_jar_facts(jar: &Path) -> JarFacts {
    let file = jar
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| jar.display().to_string());

    let Some(entries) = read_manifest_entries(jar).await else {
        return JarFacts {
            file,
            unreadable: true,
            ..JarFacts::default()
        };
    };

    JarFacts {
        file,
        unreadable: false,
        id: mod_id_from_entries(&entries).map(|parsed| parsed.id),
        loader: loader_from_entries(&entries),
        required_deps: required_deps_from_entries(&entries),
    }
}

#[must_use]
pub fn scan_mod_health(cluster_loader: GameLoader, jars: &[JarFacts]) -> Vec<HealthIssue> {
    let mut issues = Vec::new();

    let mut by_id: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for jar in jars {
        if let Some(id) = jar.id.as_deref() {
            by_id.entry(id).or_default().push(jar.file.as_str());
        }
    }
    for (id, mut files) in by_id {
        if files.len() < 2 {
            continue;
        }
        files.sort_unstable();
        issues.push(HealthIssue::DuplicateMod {
            id: id.to_owned(),
            files: files.into_iter().map(str::to_owned).collect(),
        });
    }

    if cluster_loader.is_modded() {
        for jar in jars {
            let Some(loader) = jar.loader else {
                continue;
            };
            if !loader_fits(cluster_loader, loader) {
                issues.push(HealthIssue::WrongLoader {
                    file: jar.file.clone(),
                    loader,
                });
            }
        }
    }

    let installed: HashSet<&str> = jars.iter().filter_map(|jar| jar.id.as_deref()).collect();
    let mut missing: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for jar in jars {
        let Some(owner) = jar.id.as_deref() else {
            continue;
        };
        for dep in &jar.required_deps {
            if ENV_MOD_IDS.contains(&dep.as_str()) || installed.contains(dep.as_str()) {
                continue;
            }
            missing.entry(dep).or_default().insert(owner);
        }
    }
    for (id, owners) in missing {
        issues.push(HealthIssue::MissingDependency {
            id: id.to_owned(),
            required_by: owners.into_iter().map(str::to_owned).collect(),
        });
    }

    for jar in jars {
        if jar.unreadable {
            issues.push(HealthIssue::Unreadable {
                file: jar.file.clone(),
            });
        } else if jar.id.is_none() && jar.loader.is_none() {
            issues.push(HealthIssue::NoMetadata {
                file: jar.file.clone(),
            });
        }
    }

    issues.sort_by_key(HealthIssue::sort_key);
    issues
}

#[must_use]
pub fn loader_fits(cluster: GameLoader, jar: GameLoader) -> bool {
    cluster == jar || matches!((cluster, jar), (GameLoader::Quilt, GameLoader::Fabric))
}

fn required_deps_from_entries(entries: &[(String, Vec<u8>)]) -> Vec<String> {
    let mut deps: BTreeSet<String> = BTreeSet::new();
    for wanted in MANIFESTS {
        let Some((_, bytes)) = entries.iter().find(|(name, _)| name == wanted) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        deps.extend(required_deps(wanted, text));
    }
    deps.into_iter()
        .filter(|dep| {
            !dep.is_empty() && !dep.contains('$') && !dep.contains('{') && !dep.contains('}')
        })
        .collect()
}

#[must_use]
pub fn required_deps(manifest: &str, text: &str) -> Vec<String> {
    match manifest {
        "fabric.mod.json" => {
            let Ok(json) = serde_json::from_str::<Value>(text) else {
                return Vec::new();
            };
            json.pointer("/depends")
                .and_then(Value::as_object)
                .map(|depends| depends.keys().cloned().collect())
                .unwrap_or_default()
        }
        "quilt.mod.json" => {
            let Ok(json) = serde_json::from_str::<Value>(text) else {
                return Vec::new();
            };
            json.pointer("/quilt_loader/depends")
                .and_then(Value::as_array)
                .map(|depends| {
                    depends
                        .iter()
                        .filter_map(|dep| match dep {
                            Value::String(id) => Some(id.clone()),
                            other => other.get("id")?.as_str().map(str::to_owned),
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        "META-INF/neoforge.mods.toml" | "META-INF/mods.toml" => {
            let Ok(toml) = toml::from_str::<toml::Value>(text) else {
                return Vec::new();
            };
            let Some(dependencies) = toml.get("dependencies").and_then(toml::Value::as_table)
            else {
                return Vec::new();
            };
            dependencies
                .values()
                .filter_map(toml::Value::as_array)
                .flatten()
                .filter(|dep| {
                    dep.get("mandatory")
                        .and_then(toml::Value::as_bool)
                        .unwrap_or(true)
                })
                .filter_map(|dep| dep.get("modId")?.as_str().map(str::to_owned))
                .collect()
        }
        _ => {
            let Ok(json) = serde_json::from_str::<Value>(text) else {
                return Vec::new();
            };
            json.pointer("/0/requiredMods")
                .and_then(Value::as_array)
                .map(|deps| {
                    deps.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn facts(file: &str, id: Option<&str>, loader: Option<GameLoader>, deps: &[&str]) -> JarFacts {
        JarFacts {
            file: file.to_string(),
            unreadable: false,
            id: id.map(str::to_owned),
            loader,
            required_deps: deps.iter().map(|dep| (*dep).to_string()).collect(),
        }
    }

    #[test]
    fn clean_folders_scan_clean() {
        let jars = [
            facts("a.jar", Some("alpha"), Some(GameLoader::Fabric), &["beta"]),
            facts("b.jar", Some("beta"), Some(GameLoader::Fabric), &[]),
        ];
        assert_eq!(scan_mod_health(GameLoader::Fabric, &jars), Vec::new());
    }

    #[test]
    fn duplicate_mod_ids_are_reported_with_every_file() {
        let jars = [
            facts(
                "newer/sodium.jar",
                Some("sodium"),
                Some(GameLoader::Fabric),
                &[],
            ),
            facts("sodium.jar", Some("sodium"), Some(GameLoader::Fabric), &[]),
            facts("other.jar", Some("other"), Some(GameLoader::Fabric), &[]),
        ];
        let issues = scan_mod_health(GameLoader::Fabric, &jars);
        assert_eq!(
            issues,
            vec![HealthIssue::DuplicateMod {
                id: "sodium".to_string(),
                files: vec!["newer/sodium.jar".to_string(), "sodium.jar".to_string()],
            }]
        );
    }

    #[test]
    fn jars_without_an_id_never_count_as_duplicates() {
        let jars = [
            facts("a.jar", None, None, &[]),
            facts("b.jar", None, None, &[]),
        ];
        let issues = scan_mod_health(GameLoader::Fabric, &jars);
        assert!(
            issues
                .iter()
                .all(|issue| !matches!(issue, HealthIssue::DuplicateMod { .. }))
        );
    }

    #[test]
    fn quilt_runs_fabric_mods_but_others_do_not() {
        assert!(loader_fits(GameLoader::Quilt, GameLoader::Fabric));
        assert!(loader_fits(GameLoader::Quilt, GameLoader::Quilt));
        assert!(loader_fits(GameLoader::Fabric, GameLoader::Fabric));
        assert!(!loader_fits(GameLoader::Fabric, GameLoader::Forge));
        assert!(!loader_fits(GameLoader::Forge, GameLoader::NeoForge));
        assert!(!loader_fits(GameLoader::NeoForge, GameLoader::Forge));
    }

    #[test]
    fn a_forge_jar_in_a_fabric_instance_is_a_wrong_loader() {
        let jars = [facts("jei.jar", Some("jei"), Some(GameLoader::Forge), &[])];
        let issues = scan_mod_health(GameLoader::Fabric, &jars);
        assert_eq!(
            issues,
            vec![HealthIssue::WrongLoader {
                file: "jei.jar".to_string(),
                loader: GameLoader::Forge,
            }]
        );
    }

    #[test]
    fn vanilla_instances_do_not_report_wrong_loaders() {
        let jars = [facts("jei.jar", Some("jei"), Some(GameLoader::Forge), &[])];
        let issues = scan_mod_health(GameLoader::Vanilla, &jars);
        assert!(
            issues
                .iter()
                .all(|issue| !matches!(issue, HealthIssue::WrongLoader { .. }))
        );
    }

    #[test]
    fn missing_dependencies_are_grouped_and_environment_ids_ignored() {
        let jars = [
            facts(
                "alpha.jar",
                Some("alpha"),
                Some(GameLoader::Fabric),
                &["beta", "minecraft", "java", "fabricloader"],
            ),
            facts(
                "gamma.jar",
                Some("gamma"),
                Some(GameLoader::Fabric),
                &["beta", "alpha"],
            ),
        ];
        let issues = scan_mod_health(GameLoader::Fabric, &jars);
        assert_eq!(
            issues,
            vec![HealthIssue::MissingDependency {
                id: "beta".to_string(),
                required_by: vec!["alpha".to_string(), "gamma".to_string()],
            }]
        );
    }

    #[test]
    fn unreadable_and_metadata_less_jars_are_reported() {
        let mut broken = facts("broken.jar", None, None, &[]);
        broken.unreadable = true;
        let jars = [broken, facts("junk.jar", None, None, &[])];
        let issues = scan_mod_health(GameLoader::Fabric, &jars);
        assert_eq!(
            issues,
            vec![
                HealthIssue::Unreadable {
                    file: "broken.jar".to_string(),
                },
                HealthIssue::NoMetadata {
                    file: "junk.jar".to_string(),
                },
            ]
        );
    }

    #[test]
    fn fabric_dependencies_come_from_the_depends_object() {
        let deps = required_deps(
            "fabric.mod.json",
            r#"{"id":"alpha","depends":{"fabric-api":"*","minecraft":">=1.20"}}"#,
        );
        assert_eq!(
            deps,
            vec!["fabric-api".to_string(), "minecraft".to_string()]
        );
    }

    #[test]
    fn quilt_dependencies_come_from_the_loader_list() {
        let deps = required_deps(
            "quilt.mod.json",
            r#"{"quilt_loader":{"id":"alpha","depends":[{"id":"beta","versions":">=1.0"},{"id":"minecraft"}]}}"#,
        );
        assert_eq!(deps, vec!["beta".to_string(), "minecraft".to_string()]);
    }

    #[test]
    fn forge_dependencies_respect_mandatory() {
        let deps = required_deps(
            "META-INF/mods.toml",
            "[[dependencies.alpha]]\nmodId = \"required\"\nmandatory = true\n\n[[dependencies.alpha]]\nmodId = \"also-required\"\n\n[[dependencies.alpha]]\nmodId = \"optional\"\nmandatory = false\n",
        );
        assert_eq!(
            deps,
            vec!["required".to_string(), "also-required".to_string()]
        );
    }

    async fn write_jar(path: &Path, entries: &[(&str, &[u8])]) {
        let file = tokio::fs::File::create(path).await.unwrap();
        let mut writer = async_zip::tokio::write::ZipFileWriter::with_tokio(file);
        for (name, data) in entries {
            let builder =
                async_zip::ZipEntryBuilder::new((*name).into(), async_zip::Compression::Deflate);
            writer.write_entry_whole(builder, data).await.unwrap();
        }
        writer.close().await.unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oneclient-health-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir must be creatable");
        dir
    }

    #[tokio::test]
    async fn scan_reads_real_jars() {
        let dir = temp_dir("real-jars");
        write_jar(
            &dir.join("alpha.jar"),
            &[(
                "fabric.mod.json",
                br#"{"id":"alpha","depends":{"beta":"*"}}"# as &[u8],
            )],
        )
        .await;
        write_jar(
            &dir.join("beta.jar"),
            &[("fabric.mod.json", br#"{"id":"beta"}"# as &[u8])],
        )
        .await;
        write_jar(&dir.join("junk.jar"), &[("readme.txt", b"hello" as &[u8])]).await;
        std::fs::write(dir.join("broken.jar"), b"not a zip").unwrap();
        std::fs::write(dir.join("notes.txt"), b"ignored").unwrap();

        let issues = scan_mods_dir(&dir, GameLoader::Fabric).await;
        assert_eq!(
            issues,
            vec![
                HealthIssue::Unreadable {
                    file: "broken.jar".to_string(),
                },
                HealthIssue::NoMetadata {
                    file: "junk.jar".to_string(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn scan_reports_the_missing_dependency_of_a_real_pair() {
        let dir = temp_dir("real-missing");
        write_jar(
            &dir.join("alpha.jar"),
            &[(
                "fabric.mod.json",
                br#"{"id":"alpha","depends":{"beta":"*"}}"# as &[u8],
            )],
        )
        .await;

        let issues = scan_mods_dir(&dir, GameLoader::Fabric).await;
        assert_eq!(
            issues,
            vec![HealthIssue::MissingDependency {
                id: "beta".to_string(),
                required_by: vec!["alpha".to_string()],
            }]
        );
    }

    #[tokio::test]
    async fn a_missing_mods_folder_scans_clean() {
        let dir = temp_dir("absent").join("not-created");
        assert_eq!(scan_mods_dir(&dir, GameLoader::Fabric).await, Vec::new());
    }
}
