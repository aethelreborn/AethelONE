//! PolyPlus is mirrored from the `aethelreborn/AethelOnePLUS` rolling release
//! so installs carry the cracked-uuid namebadge patch. Downloads and bundle
//! update identity consult [`POLYPLUS_MIRROR_MANIFEST_URL`] and fall back to
//! the official Modrinth file whenever the manifest is unreachable, unparsable
//! or has no entry for the requested mc version, so the mirror can disappear
//! entirely without breaking PolyPlus installs.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;

use oneclient_common::constants::{POLYPLUS_MIRROR_MANIFEST_URL, POLYPLUS_MODRINTH_PROJECT_ID};
use oneclient_common::domain::ProviderId;
use oneclient_common::paths;
use oneclient_net::{EtagPolicy, fetch_cached};

use crate::bundles::{BundleFile, BundleFileKind};
use crate::ctx::ContentCtx;
use crate::packages::types::VersionFile;

/// Tracked bundle version id written for a mirrored install stands for the
/// mirrored bytes rather than any provider version id
pub const MARKER_PREFIX: &str = "mirror:";

const MEMORY_TTL: Duration = Duration::from_secs(5 * 60);
const FAILURE_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Deserialize)]
pub struct MirrorEntry {
    pub mc_version: String,
    pub file_name: String,
    pub url: String,
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MirrorManifest {
    #[serde(default)]
    pub versions: Vec<MirrorEntry>,
}

impl MirrorManifest {
    #[must_use]
    pub fn entry_for_mc(&self, mc: &str) -> Option<&MirrorEntry> {
        self.versions.iter().find(|entry| entry.mc_version == mc)
    }
}

/// `mods/polyplus-1.2.57+26.1.jar` and `polyplus-1.2.57+1.21.11.jar` both
/// yield `26.1` / `1.21.11`; anything without the `+<mc>.jar` suffix yields
/// `None` and the caller keeps the official file
#[must_use]
pub fn mc_from_jar_name(name: &str) -> Option<&str> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let mc = base.rsplit_once('+')?.1.strip_suffix(".jar")?;
    (!mc.is_empty()).then_some(mc)
}

/// What a bundle-tracked file should be recorded as, and which bytes that id
/// stands for when the mirror governs the file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedIdentity {
    pub version_id: String,
    /// `Some` when a mirror entry applies: the installed artifact hash is
    /// compared against it too, so a manifest that cannot be reached never
    /// re-downloads identical bytes
    pub sha1: Option<String>,
}

impl ExpectedIdentity {
    #[must_use]
    pub fn matches(&self, installed_version_id: &str, installed_hash: &str) -> bool {
        installed_version_id == self.version_id
            || self
                .sha1
                .as_deref()
                .is_some_and(|sha1| installed_hash.eq_ignore_ascii_case(sha1))
    }
}

/// A missing manifest resolves to the id from the bundle manifest itself
#[must_use]
pub fn expected_identity(manifest: Option<&MirrorManifest>, file: &BundleFile) -> ExpectedIdentity {
    let BundleFileKind::Managed {
        provider,
        project_id,
        version_id,
        ..
    } = &file.kind
    else {
        return ExpectedIdentity {
            version_id: file.kind.bundle_version_id(),
            sha1: None,
        };
    };
    if *provider != ProviderId::Modrinth || project_id != POLYPLUS_MODRINTH_PROJECT_ID {
        return ExpectedIdentity {
            version_id: version_id.clone(),
            sha1: None,
        };
    }

    let entry = mc_from_jar_name(&file.path)
        .and_then(|mc| manifest.and_then(|manifest| manifest.entry_for_mc(mc)));
    match entry {
        Some(entry) => ExpectedIdentity {
            version_id: format!("{MARKER_PREFIX}{}", entry.sha1),
            sha1: Some(entry.sha1.clone()),
        },
        None => ExpectedIdentity {
            version_id: version_id.clone(),
            sha1: None,
        },
    }
}

/// `None` whenever the mirror cannot serve this file: the caller keeps the
/// official one
#[must_use]
pub fn rewrite_with(manifest: &MirrorManifest, file: &VersionFile) -> Option<VersionFile> {
    let entry = manifest.entry_for_mc(mc_from_jar_name(&file.file_name)?)?;
    Some(VersionFile {
        sha1: entry.sha1.clone(),
        url: entry.url.clone(),
        file_name: entry.file_name.clone(),
        size: entry.size,
        primary: file.primary,
        fingerprint: file.fingerprint.clone(),
    })
}

/// Applies only to PolyPlus files; every other download reaches this on the
/// fast path and stays untouched
pub async fn rewritten_version_file(
    ctx: &ContentCtx,
    provider: ProviderId,
    project_id: &str,
    file: &VersionFile,
) -> Option<VersionFile> {
    if provider != ProviderId::Modrinth || project_id != POLYPLUS_MODRINTH_PROJECT_ID {
        return None;
    }
    let manifest = load(ctx).await?;
    rewrite_with(&manifest, file)
}

struct Memo {
    at: Instant,
    manifest: Option<Arc<MirrorManifest>>,
}

static MEMORY: OnceLock<Mutex<Option<Memo>>> = OnceLock::new();

/// A successful manifest is reused for [`MEMORY_TTL`]; a failed fetch is not
/// retried for [`FAILURE_TTL`] so an offline launcher does not stall on its
/// own timeouts
pub async fn load(ctx: &ContentCtx) -> Option<Arc<MirrorManifest>> {
    let cell = MEMORY.get_or_init(|| Mutex::new(None));
    {
        let guard = cell.lock().unwrap();
        if let Some(memo) = guard.as_ref() {
            let ttl = if memo.manifest.is_some() {
                MEMORY_TTL
            } else {
                FAILURE_TTL
            };
            if memo.at.elapsed() < ttl {
                return memo.manifest.clone();
            }
        }
    }

    let manifest = fetch_manifest(ctx).await;
    *cell.lock().unwrap() = Some(Memo {
        at: Instant::now(),
        manifest: manifest.clone(),
    });
    manifest
}

async fn fetch_manifest(ctx: &ContentCtx) -> Option<Arc<MirrorManifest>> {
    let dir = match paths::caches_dir() {
        Ok(dir) => dir,
        Err(err) => {
            tracing::debug!(%err, "no cache dir for the PolyPlus mirror manifest");
            return None;
        }
    };
    if let Err(err) = polyio::create_dir_all(&dir).await {
        tracing::debug!(%err, "could not open the cache dir for the PolyPlus mirror manifest");
        return None;
    }
    let path = dir.join("polyplus-mirror.json");

    let fetched = match fetch_cached(
        &ctx.net,
        POLYPLUS_MIRROR_MANIFEST_URL,
        &path,
        EtagPolicy::CommitNow,
    )
    .await
    {
        Ok(Some(fetched)) => fetched,
        Ok(None) => {
            tracing::debug!("PolyPlus mirror manifest unreachable with no cached copy");
            return None;
        }
        Err(err) => {
            tracing::debug!(%err, "PolyPlus mirror manifest fetch failed");
            return None;
        }
    };

    match fetched.json::<MirrorManifest>() {
        Ok(manifest) if !manifest.versions.is_empty() => Some(Arc::new(manifest)),
        Ok(_) => {
            tracing::debug!("PolyPlus mirror manifest has no entries");
            None
        }
        Err(err) => {
            tracing::warn!(%err, "PolyPlus mirror manifest could not be parsed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> MirrorManifest {
        serde_json::from_str(
            r#"{
                "versions": [
                    {
                        "mc_version": "1.21.11",
                        "file_name": "polyplus-1.2.57+1.21.11.jar",
                        "url": "https://example.test/polyplus-1.2.57%2B1.21.11.jar",
                        "sha1": "aaa111",
                        "size": 42
                    },
                    {
                        "mc_version": "26.1",
                        "file_name": "polyplus-1.2.57+26.1.jar",
                        "url": "https://example.test/polyplus-1.2.57%2B26.1.jar",
                        "sha1": "bbb222",
                        "size": 43
                    }
                ]
            }"#,
        )
        .expect("test manifest parses")
    }

    fn polyplus_file(path: &str, version_id: &str) -> BundleFile {
        BundleFile {
            enabled: true,
            hidden: false,
            path: path.into(),
            size: 0,
            file_type: Default::default(),
            kind: BundleFileKind::Managed {
                provider: ProviderId::Modrinth,
                project_id: POLYPLUS_MODRINTH_PROJECT_ID.into(),
                version_id: version_id.into(),
                sha1: "official1".into(),
            },
        }
    }

    fn version_file(file_name: &str) -> VersionFile {
        VersionFile {
            sha1: "official1".into(),
            url: format!("https://cdn.modrinth.test/{file_name}"),
            file_name: file_name.into(),
            primary: true,
            size: 7,
            fingerprint: None,
        }
    }

    #[test]
    fn mc_comes_from_the_jar_name_in_both_path_shapes() {
        assert_eq!(
            mc_from_jar_name("mods/polyplus-1.2.57+26.1.jar"),
            Some("26.1")
        );
        assert_eq!(
            mc_from_jar_name("polyplus-1.2.57+1.21.11.jar"),
            Some("1.21.11")
        );
        assert_eq!(
            mc_from_jar_name("mods/polyplus-1.2.57+1.8.9.jar"),
            Some("1.8.9")
        );
        assert_eq!(mc_from_jar_name("mods/sodium.jar"), None);
        assert_eq!(mc_from_jar_name("polyplus-1.2.57+26.1"), None);
        assert_eq!(mc_from_jar_name("polyplus+.jar"), None);
    }

    #[test]
    fn entries_resolve_by_exact_mc_version() {
        let manifest = manifest();
        assert_eq!(
            manifest.entry_for_mc("26.1").map(|e| e.sha1.as_str()),
            Some("bbb222")
        );
        assert!(manifest.entry_for_mc("26.1.2").is_none());
        assert!(manifest.entry_for_mc("").is_none());
    }

    #[test]
    fn a_bundled_polyplus_file_with_a_mirror_entry_gets_the_marker() {
        let identity = expected_identity(
            Some(&manifest()),
            &polyplus_file("mods/polyplus-1.2.57+1.21.11.jar", "KLs3nCTR"),
        );
        assert_eq!(identity.version_id, "mirror:aaa111");
        assert_eq!(identity.sha1.as_deref(), Some("aaa111"));
    }

    #[test]
    fn the_bundle_can_name_a_newer_mc_than_the_mirror_builds() {
        // The 26.1.2 bundle ships the `+26.1` jar while an official bump the
        // mirror has not caught up to yields no entry: identity stays official
        let identity = expected_identity(
            Some(&manifest()),
            &polyplus_file("mods/polyplus-1.2.58+26.3.jar", "KLs3nCTR"),
        );
        assert_eq!(identity.version_id, "KLs3nCTR");
        assert_eq!(identity.sha1, None);
    }

    #[test]
    fn without_a_manifest_polyplus_keeps_the_official_identity() {
        let identity =
            expected_identity(None, &polyplus_file("mods/polyplus-1.2.57+26.1.jar", "v1"));
        assert_eq!(identity.version_id, "v1");
        assert_eq!(identity.sha1, None);
    }

    #[test]
    fn packages_the_mirror_does_not_care_about_keep_their_bundle_id() {
        let file = BundleFile {
            path: "mods/polyplus-1.2.57+26.1.jar".into(),
            kind: BundleFileKind::Managed {
                provider: ProviderId::Modrinth,
                project_id: "AibBIVmj".into(),
                version_id: "other".into(),
                sha1: "official2".into(),
            },
            ..polyplus_file("ignored", "ignored")
        };
        let identity = expected_identity(Some(&manifest()), &file);
        assert_eq!(identity.version_id, "other");
        assert_eq!(identity.sha1, None);

        let external = BundleFile {
            path: "mods/other.jar".into(),
            kind: BundleFileKind::External {
                file: crate::packages::types::ExternalFile {
                    name: "other.jar".into(),
                    url: "https://example.test/other.jar".into(),
                    sha1: "external1".into(),
                    size: 1,
                    content_type: oneclient_common::domain::ContentType::Mod,
                },
                id: None,
                meta: None,
            },
            ..polyplus_file("ignored", "ignored")
        };
        assert_eq!(
            expected_identity(Some(&manifest()), &external).version_id,
            "external1"
        );
    }

    #[test]
    fn identity_matches_on_bytes_or_marker_but_neither_when_official() {
        let mirrored = ExpectedIdentity {
            version_id: "mirror:aaa111".into(),
            sha1: Some("aaa111".into()),
        };
        assert!(mirrored.matches("mirror:aaa111", "aaa111"));
        assert!(mirrored.matches("KLs3nCTR", "AAA111"));
        assert!(!mirrored.matches("KLs3nCTR", "official1"));

        let official = ExpectedIdentity {
            version_id: "KLs3nCTR".into(),
            sha1: None,
        };
        assert!(official.matches("KLs3nCTR", "anything"));
        assert!(!official.matches("mirror:aaa111", "aaa111"));
    }

    #[test]
    fn a_download_for_the_mirrored_mc_is_rewritten() {
        let file = version_file("polyplus-1.2.57+26.1.jar");
        let rewritten = rewrite_with(&manifest(), &file).expect("entry exists");
        assert_eq!(rewritten.sha1, "bbb222");
        assert_eq!(
            rewritten.url,
            "https://example.test/polyplus-1.2.57%2B26.1.jar"
        );
        assert_eq!(rewritten.file_name, "polyplus-1.2.57+26.1.jar");
        assert_eq!(rewritten.size, 43);
        assert!(rewritten.primary);
        assert_eq!(rewritten.fingerprint, None);
    }

    #[test]
    fn a_download_without_a_mirror_entry_stays_official() {
        let file = version_file("polyplus-1.2.58+26.3.jar");
        assert!(rewrite_with(&manifest(), &file).is_none());

        let unparsable = version_file("PolyPlus.jar");
        assert!(rewrite_with(&manifest(), &unparsable).is_none());
    }
}
