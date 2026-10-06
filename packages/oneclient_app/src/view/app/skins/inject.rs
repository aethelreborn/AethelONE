use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use oneclient_cluster::Cluster;
use serde_json::Value;

use super::library::Library;

const CSL_API: &str = "https://api.modrinth.com/v2/project/customskinloader/version";
// Modrinth lags behind upstream releases (15.1 fixing the startup NPE is
// GitHub-only), so the latest GitHub release is the primary source
const CSL_GITHUB_LATEST: &str =
    "https://api.github.com/repos/xfl03/MCCustomSkinLoader/releases/latest";
const CSL_PREFIX: &str = "CustomSkinLoader_Universal-";
const CSL_CACHE_SUBDIR: &str = "skins/csl";
/// 15.0.1 is tagged for 26.x on Modrinth but crashes Fabric on startup with a
/// known mixin conflict (upstream fixed it in 15.1) — never fall back to it
const CSL_KNOWN_BAD: &[&str] = &["CustomSkinLoader_Universal-15.0.1.jar"];

/// Keeps CustomSkinLoader and the active local skin in sync with a cluster
/// right before launch. Minecraft reads its mods and profile files once at
/// startup, so the work has to happen before the process starts.
pub async fn sync_before_launch(
    state: &oneclient_core::LauncherState,
    cluster_id: oneclient_db::models::ClusterId,
    account: &oneclient_auth::MinecraftAccount,
) {
    if let Err(err) = sync_inner(state, cluster_id, account).await {
        tracing::warn!(cluster_id, %err, "skin sync skipped");
    }
}

async fn sync_inner(
    state: &oneclient_core::LauncherState,
    cluster_id: oneclient_db::models::ClusterId,
    account: &oneclient_auth::MinecraftAccount,
) -> Result<(), String> {
    let library = Library::load();
    let cluster = state
        .clusters
        .get(cluster_id)
        .await
        .map_err(|err| err.to_string())?;
    let mods_dir = oneclient_common::paths::cluster_mods_dir(&cluster.folder_name)
        .map_err(|err| err.to_string())?;

    let Some((entry, skin)) = library.active_bytes() else {
        remove_csl_jars(&mods_dir)?;
        return Ok(());
    };

    let build = match csl_build_for(&cluster.mc_version).await {
        Ok(Some(build)) => build,
        Ok(None) => {
            // No build supports this game version yet, and a jar that cannot
            // load could take the whole launch down with it
            tracing::info!(
                mc_version = %cluster.mc_version,
                "no CustomSkinLoader build supports this game version"
            );
            remove_csl_jars(&mods_dir)?;
            return Ok(());
        }
        Err(err) => return offline_fallback(&mods_dir, &cluster, account, &skin, &err),
    };

    let cached = match fetch_jar(&build).await {
        Ok(path) => path,
        Err(err) => return offline_fallback(&mods_dir, &cluster, account, &skin, &err),
    };
    install_jar(&cached, &build.filename, &mods_dir)?;
    write_skin(&cluster, account, &skin)?;
    tracing::info!(
        skin = %entry.name,
        mc_version = %cluster.mc_version,
        jar = %build.filename,
        "active skin synced for launch"
    );
    Ok(())
}

/// The Modrinth check failed (offline, transient error) or the download broke:
/// a jar from an earlier launch still delivers the active skin, so only skip
/// when there is nothing installed to fall back on.
fn offline_fallback(
    mods_dir: &Path,
    cluster: &Cluster,
    account: &oneclient_auth::MinecraftAccount,
    skin: &[u8],
    reason: &str,
) -> Result<(), String> {
    if has_csl_jar(mods_dir) {
        tracing::warn!(
            %reason,
            "customskinloader availability check failed, reusing the installed jar"
        );
        write_skin(cluster, account, skin)
    } else {
        tracing::warn!(%reason, "customskinloader unavailable, skipping skin sync");
        Ok(())
    }
}

fn write_skin(
    cluster: &Cluster,
    account: &oneclient_auth::MinecraftAccount,
    skin: &[u8],
) -> Result<(), String> {
    if !account.is_offline() {
        return Ok(());
    }
    let game_dir = cluster.game_dir().map_err(|err| err.to_string())?;
    let dir = game_dir
        .join("CustomSkinLoader")
        .join("LocalSkin")
        .join("skins");
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let file_name = sanitize_username(&account.username);
    std::fs::write(dir.join(file_name), skin).map_err(|err| err.to_string())
}

fn sanitize_username(name: &str) -> String {
    name.chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c })
        .collect()
}

struct Build {
    filename: String,
    url: String,
}

static VERSIONS: OnceLock<Vec<Value>> = OnceLock::new();

/// Picks a build of CustomSkinLoader for the cluster's game version: the
/// latest GitHub release first, Modrinth's newest supported build as fallback.
async fn csl_build_for(mc_version: &str) -> Result<Option<Build>, String> {
    match github_build().await {
        Ok(Some(build)) => return Ok(Some(build)),
        Ok(None) => {
            tracing::warn!("latest github release ships no universal jar, using modrinth");
        }
        Err(err) => {
            tracing::warn!(%err, "github release lookup failed, using modrinth");
        }
    }
    modrinth_build_for(mc_version).await
}

/// The newest stable upstream release on GitHub. Its Universal jar covers
/// 1.8 through 26.3, so it is used for every cluster without a version gate.
async fn github_build() -> Result<Option<Build>, String> {
    let text = github_client()?
        .get(CSL_GITHUB_LATEST)
        .send()
        .await
        .map_err(|err| format!("github request failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("github responded with an error: {err}"))?
        .text()
        .await
        .map_err(|err| format!("github body read failed: {err}"))?;
    let release: Value = serde_json::from_str(&text)
        .map_err(|err| format!("github response was not JSON: {err}"))?;
    let Some(assets) = release.get("assets").and_then(Value::as_array) else {
        return Err("github release carried no assets".to_string());
    };
    for asset in assets {
        let filename = asset.get("name").and_then(Value::as_str).unwrap_or_default();
        let url = asset
            .get("browser_download_url")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if filename.starts_with(CSL_PREFIX)
            && filename.ends_with(".jar")
            && !url.is_empty()
            && !CSL_KNOWN_BAD.contains(&filename)
        {
            return Ok(Some(Build {
                filename: filename.to_string(),
                url: url.to_string(),
            }));
        }
    }
    Ok(None)
}

fn github_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("AethelONE/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| err.to_string())
}

/// Picks the newest Modrinth build of CustomSkinLoader that both ships a
/// Universal jar and claims support for the cluster's game version.
async fn modrinth_build_for(mc_version: &str) -> Result<Option<Build>, String> {
    if VERSIONS.get().is_none() {
        let text = reqwest::get(CSL_API)
            .await
            .map_err(|err| format!("modrinth request failed: {err}"))?
            .error_for_status()
            .map_err(|err| format!("modrinth responded with an error: {err}"))?
            .text()
            .await
            .map_err(|err| format!("modrinth body read failed: {err}"))?;
        let versions: Vec<Value> = serde_json::from_str(&text)
            .map_err(|err| format!("modrinth response was not JSON: {err}"))?;
        let _ = VERSIONS.set(versions);
    }
    let versions = VERSIONS
        .get()
        .ok_or_else(|| "modrinth version list unavailable".to_string())?;

    let mut fallback: Option<Build> = None;
    for version in versions {
        let supported = version
            .get("game_versions")
            .and_then(Value::as_array)
            .is_some_and(|list| {
                list.iter().any(|entry| {
                    version_matches(mc_version, entry.as_str().unwrap_or_default())
                })
            });
        if !supported {
            continue;
        }
        let Some(files) = version.get("files").and_then(Value::as_array) else {
            continue;
        };
        for file in files {
            let filename = file.get("filename").and_then(Value::as_str).unwrap_or_default();
            let url = file.get("url").and_then(Value::as_str).unwrap_or_default();
            if !filename.ends_with(".jar") || url.is_empty() || CSL_KNOWN_BAD.contains(&filename) {
                continue;
            }
            let build = || Build {
                filename: filename.to_string(),
                url: url.to_string(),
            };
            if filename.starts_with(CSL_PREFIX) {
                return Ok(Some(build()));
            }
            // Any other CustomSkinLoader build is only acceptable when it also
            // lists fabric, so a forge-only file never lands on our clusters
            let fabric_ok = version
                .get("loaders")
                .and_then(Value::as_array)
                .is_some_and(|list| {
                    list.iter().any(|entry| entry.as_str() == Some("fabric"))
                });
            if fabric_ok && filename.to_lowercase().contains("customskinloader") && fallback.is_none()
            {
                fallback = Some(build());
            }
        }
    }
    Ok(fallback)
}

/// Modrinth lists exact game versions; the dotted prefix rules keep "1.2"
/// from matching "1.21" while still letting "1.8" cover "1.8.9".
fn version_matches(mc_version: &str, supported: &str) -> bool {
    if mc_version == supported {
        return true;
    }
    let covered = |longer: &str, shorter: &str| {
        longer.starts_with(shorter)
            && longer.as_bytes().get(shorter.len()) == Some(&b'.')
    };
    covered(mc_version, supported) || covered(supported, mc_version)
}

async fn fetch_jar(build: &Build) -> Result<PathBuf, String> {
    let base = oneclient_common::paths::data_dir().map_err(|err| err.to_string())?;
    let dir = base.join(CSL_CACHE_SUBDIR);
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let cached = dir.join(&build.filename);
    if std::fs::metadata(&cached).is_ok_and(|meta| meta.len() > 0) {
        return Ok(cached);
    }
    let bytes = github_client()?
        .get(&build.url)
        .send()
        .await
        .map_err(|err| format!("jar download failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("jar download failed: {err}"))?
        .bytes()
        .await
        .map_err(|err| format!("jar download failed: {err}"))?;
    if bytes.is_empty() {
        return Err("downloaded jar was empty".to_string());
    }
    let temp = dir.join(format!("{}.part", build.filename));
    std::fs::write(&temp, &bytes).map_err(|err| err.to_string())?;
    std::fs::rename(&temp, &cached).map_err(|err| err.to_string())?;
    Ok(cached)
}

fn install_jar(cached: &Path, filename: &str, mods_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(mods_dir).map_err(|err| err.to_string())?;
    remove_csl_jars_except(mods_dir, Some(filename))?;
    let target = mods_dir.join(filename);
    let cached_len = std::fs::metadata(cached).map(|meta| meta.len()).unwrap_or(0);
    let installed_len = std::fs::metadata(&target).map(|meta| meta.len()).ok();
    if installed_len != Some(cached_len) {
        std::fs::copy(cached, &target).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn remove_csl_jars(mods_dir: &Path) -> Result<(), String> {
    remove_csl_jars_except(mods_dir, None)
}

fn remove_csl_jars_except(mods_dir: &Path, keep: Option<&str>) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let lowered = name.to_string_lossy().to_lowercase();
        if !lowered.starts_with("customskinloader") || !lowered.ends_with(".jar") {
            continue;
        }
        if keep.is_some_and(|keep| name == std::ffi::OsStr::new(keep)) {
            continue;
        }
        let _ = std::fs::remove_file(entry.path());
    }
    Ok(())
}

fn has_csl_jar(mods_dir: &Path) -> bool {
    std::fs::read_dir(mods_dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            name.starts_with("customskinloader") && name.ends_with(".jar")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{sanitize_username, version_matches};

    #[test]
    fn matches_exact_and_broader_version_lines() {
        assert!(version_matches("1.8.9", "1.8.9"));
        assert!(version_matches("1.8.9", "1.8"));
        assert!(version_matches("1.8", "1.8.9"));
        assert!(version_matches("26.1.2", "26.1.2"));
    }

    #[test]
    fn never_crosses_unrelated_version_numbers() {
        assert!(!version_matches("26.3", "26.2"));
        assert!(!version_matches("1.21.10", "1.2"));
        assert!(!version_matches("1.21", "1.211"));
    }

    #[test]
    fn keeps_usable_usernames_but_strips_path_separators() {
        assert_eq!(sanitize_username("Darkie_Krish"), "Darkie_Krish");
        assert_eq!(sanitize_username("../evil"), ".._evil");
        assert_eq!(sanitize_username("a\\b:c"), "a_b_c");
    }
}
