use std::cell::Cell;

use cargo_packager_updater::{Config, Update, check_update};
use oneclient_events::{Choice, EventBus, Prompt};
use uuid::Uuid;

use crate::constants::{RELEASES_URL, UPDATER_ENDPOINT, UPDATER_PUBKEY};
// Only the Linux package-manager install path builds release URLs itself.
#[cfg(target_os = "linux")]
use anyhow::Context;
#[cfg(target_os = "linux")]
use crate::constants::RELEASES_DOWNLOAD_BASE;

pub const UPDATE_CHOICE_INSTALL: &str = "update.install";

enum UpdateAnswer {
    Install,
}

/// How the running install can be replaced by a new release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// The package-manager kinds are only ever selected on Linux.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
enum InstallKind {
    /// In-place formats cargo-packager-updater installs itself: NSIS, macOS .app, AppImage.
    Bundled,
    /// `.deb` managed by apt/dpkg.
    Deb,
    /// `.rpm` managed by dnf/yum/zypper.
    Rpm,
    /// Source builds, AUR extractions, anything with no automated install path.
    Unsupported,
}

/// Polkit exit codes for "user dismissed the auth dialog" / "not authorized".
#[cfg(target_os = "linux")]
#[derive(Debug)]
struct UpdateCancelled;

#[cfg(target_os = "linux")]
impl std::fmt::Display for UpdateCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("installation was cancelled")
    }
}

#[cfg(target_os = "linux")]
impl std::error::Error for UpdateCancelled {}

fn update_prompt(version: &str) -> Prompt<UpdateAnswer> {
    Prompt::new(
        "Update available",
        format!("OneClient {version} is ready to install. Download and install it now?"),
    )
    .option(
        Choice::primary(UPDATE_CHOICE_INSTALL, "Install"),
        UpdateAnswer::Install,
    )
    .dismiss("Not now")
}

const PROGRESS_STEP: u64 = 256 * 1024;

pub fn spawn_update_check(auto_install: bool, events: EventBus) {
    tokio::spawn(async move {
        if let Err(err) = run_check(auto_install, events).await {
            tracing::warn!("update check failed: {err:#}");
        }
    });
}

/// Debug-only drives the full auto-update UX
pub fn spawn_simulated_update() {
    tokio::spawn(async move {
        if let Err(err) = run_simulated_update().await {
            tracing::warn!("simulated update failed: {err:#}");
        }
    });
}

async fn run_simulated_update() -> anyhow::Result<()> {
    const FAKE_VERSION: &str = "9999.9999.9999";
    const FAKE_TOTAL: u64 = 48 * 1024 * 1024;

    let events = crate::launcher::state()?.services.events.clone();

    if events.ask(update_prompt(FAKE_VERSION)).await?.is_none() {
        tracing::info!("user declined simulated update");
        return Ok(());
    }

    let progress_id = Uuid::new_v4();
    let label = format!("Downloading OneClient {FAKE_VERSION}");

    let mut downloaded = 0u64;
    events.progress(progress_id, &label, downloaded, FAKE_TOTAL);
    while downloaded < FAKE_TOTAL {
        downloaded = (downloaded + PROGRESS_STEP * 8).min(FAKE_TOTAL);
        events.progress(progress_id, &label, downloaded, FAKE_TOTAL);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    events.finish_progress(
        progress_id,
        "Finished Downloading",
        format!("OneClient {FAKE_VERSION} is ready. Restart to apply."),
    );

    Ok(())
}

async fn run_check(auto_install: bool, events: EventBus) -> anyhow::Result<()> {
    // `check_update` performs a blocking HTTP request so offload it to a thread pool
    let (update, kind) = tokio::task::spawn_blocking(check_for_update).await??;
    let Some(update) = update else {
        tracing::info!("no update available");
        return Ok(());
    };

    tracing::info!("update available: {} ({kind:?})", update.version);

    // cargo-packager-updater can only replace an AppImage in place deb/rpm installs live
    // under a package-managed path that an install would fail on or clobber
    if !can_self_update(kind) {
        tracing::info!("install is not self-updatable ({kind:?}); notifying only");
        events
            .notify("Update available")
            .body(format!(
                "OneClient {} is available. Download the latest package from {} to update.",
                update.version, RELEASES_URL
            ))
            .send();
        return Ok(());
    }

    if !auto_install && events.ask(update_prompt(&update.version)).await?.is_none() {
        tracing::info!("user declined update {}", update.version);
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        match kind {
            InstallKind::Deb => return download_and_install_package(update, "deb", events).await,
            InstallKind::Rpm => return download_and_install_package(update, "rpm", events).await,
            _ => {}
        }
    }

    download_and_install(update, events).await
}

fn can_self_update(kind: InstallKind) -> bool {
    if cfg!(debug_assertions) {
        return false;
    }

    if std::env::var_os("ONECLIENT_DISABLE_AUTOUPDATE").is_some_and(|val| val.eq_ignore_ascii_case("1"))
    {
        return false;
    }

    matches!(
        kind,
        InstallKind::Bundled | InstallKind::Deb | InstallKind::Rpm
    )
}

/// Decides how (or whether) this install can be replaced in place.
#[cfg(target_os = "linux")]
fn detect_install_kind() -> InstallKind {
    if std::env::var_os("APPIMAGE").is_some() {
        return InstallKind::Bundled;
    }

    // Releases only publish x86_64 packages; other arches get the notification path.
    if std::env::consts::ARCH != "x86_64" {
        return InstallKind::Unsupported;
    }

    let Ok(exe) = std::env::current_exe().and_then(|exe| exe.canonicalize()) else {
        return InstallKind::Unsupported;
    };

    if package_manages_file("dpkg", &["-S"], &exe) {
        return InstallKind::Deb;
    }
    if package_manages_file("rpm", &["-qf"], &exe) {
        return InstallKind::Rpm;
    }

    InstallKind::Unsupported
}

#[cfg(not(target_os = "linux"))]
fn detect_install_kind() -> InstallKind {
    InstallKind::Bundled
}

/// Whether `program` (dpkg/rpm) lists `file` as one of its own.
#[cfg(target_os = "linux")]
fn package_manages_file(program: &str, args: &[&str], file: &std::path::Path) -> bool {
    std::process::Command::new(program)
        .args(args)
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn check_for_update() -> anyhow::Result<(Option<Update>, InstallKind)> {
    let kind = detect_install_kind();
    let current = env!("CARGO_PKG_VERSION").parse()?;
    let config = Config {
        endpoints: vec![UPDATER_ENDPOINT.parse()?],
        pubkey: UPDATER_PUBKEY.into(),
        ..Default::default()
    };

    Ok((check_update(current, config)?, kind))
}

async fn download_and_install(update: Update, events: EventBus) -> anyhow::Result<()> {
    let progress_id = Uuid::new_v4();
    let version = update.version.clone();
    let label = format!("Downloading OneClient {version}");

    events.progress(progress_id, &label, 0, 0);

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let downloaded = Cell::new(0u64);
        let last_sent = Cell::new(0u64);

        let bytes = update.download_extended(
            |chunk, total| {
                let now = downloaded.get() + chunk as u64;
                downloaded.set(now);
                let total = total.unwrap_or(0);

                if now == chunk as u64
                    || (total > 0 && now >= total)
                    || now - last_sent.get() >= PROGRESS_STEP
                {
                    last_sent.set(now);
                    events.progress(progress_id, &label, now, total);
                }
            },
            || {},
        )?;

        let total = downloaded.get().max(1);
        events.progress(progress_id, &label, total, total);

        update.install(bytes)?;

        // Converts the same download card into its finished state rather than adding a second
        events.finish_progress(
            progress_id,
            "Finished Downloading",
            format!("OneClient {version} is ready. Restart to apply."),
        );
        Ok(())
    })
    .await??;

    tracing::info!("update installed; restart to apply");
    Ok(())
}

/// The `.deb`/`.rpm` are not cargo-packager updater formats, so they carry no entry in
/// `latest.json`; their download URL is derived from the version the manifest announced.
#[cfg(target_os = "linux")]
fn package_url(version: &str, ext: &str) -> String {
    format!("{RELEASES_DOWNLOAD_BASE}/oneclient-{version}/OneClient_{version}_linux_x86_64.{ext}")
}

/// Downloads a signed `.deb`/`.rpm` and hands it to the package manager via pkexec.
#[cfg(target_os = "linux")]
async fn download_and_install_package(
    update: Update,
    ext: &'static str,
    events: EventBus,
) -> anyhow::Result<()> {
    let version = update.version.clone();
    let progress_id = Uuid::new_v4();
    let label = format!("Downloading OneClient {version}");

    events.progress(progress_id, &label, 0, 0);

    let url = package_url(&version, ext);
    let sig_url = format!("{url}.sig");

    let events_for_download = events.clone();
    let label_for_download = label.clone();
    let downloaded = tokio::task::spawn_blocking(move || {
        download_package(
            update,
            &url,
            &sig_url,
            &events_for_download,
            progress_id,
            &label_for_download,
        )
    })
    .await?;

    let bytes = match downloaded {
        Ok(bytes) => bytes,
        Err(err) => {
            notify_package_failure(&events, progress_id, &version, &err);
            return Err(err);
        }
    };

    let installed = tokio::task::spawn_blocking(move || install_package_file(&bytes, ext)).await?;

    match installed {
        Ok(()) => {}
        Err(err) if err.downcast_ref::<UpdateCancelled>().is_some() => {
            events.finish_progress(
                progress_id,
                "Update cancelled",
                format!("OneClient {version} was not installed."),
            );
            tracing::info!("user cancelled the package update");
            return Ok(());
        }
        Err(err) => {
            notify_package_failure(&events, progress_id, &version, &err);
            return Err(err);
        }
    }

    events.finish_progress(
        progress_id,
        "Finished Downloading",
        format!("OneClient {version} is ready. Restart to apply."),
    );
    tracing::info!("package update installed; restart to apply");
    Ok(())
}

#[cfg(target_os = "linux")]
fn notify_package_failure(
    events: &EventBus,
    progress_id: Uuid,
    version: &str,
    err: &anyhow::Error,
) {
    tracing::warn!("package update failed: {err:#}");
    events.finish_progress(progress_id, "Update failed", format!("{err:#}"));
    events
        .notify("Update failed")
        .body(format!(
            "Could not install OneClient {version} automatically ({err:#}). Download it from {RELEASES_URL}"
        ))
        .error()
        .send();
}

/// Fetches the minisign `.sig` release asset that accompanies the package.
#[cfg(target_os = "linux")]
fn fetch_signature(url: &str) -> anyhow::Result<String> {
    let response = reqwest::blocking::Client::new()
        .get(url)
        .send()
        .with_context(|| format!("failed to fetch {url}"))?
        .error_for_status()
        .with_context(|| format!("signature request for {url} was rejected"))?;
    Ok(response.text()?.trim().to_string())
}

/// The manifest only describes the formats cargo-packager-updater installs itself
/// (nsis/app/appimage); a deb/rpm travels with its signature as a sibling `.sig`
/// release asset. Swapping url + signature on the checked update reuses the
/// crate's signature-verifying download path.
#[cfg(target_os = "linux")]
fn download_package(
    update: Update,
    url: &str,
    sig_url: &str,
    events: &EventBus,
    progress_id: Uuid,
    label: &str,
) -> anyhow::Result<Vec<u8>> {
    let mut download = update;
    download.download_url = url
        .parse()
        .with_context(|| format!("invalid update url {url}"))?;
    download.signature = fetch_signature(sig_url)?;

    let downloaded = Cell::new(0u64);
    let last_sent = Cell::new(0u64);

    let bytes = download.download_extended(
        |chunk, total| {
            let now = downloaded.get() + chunk as u64;
            downloaded.set(now);
            let total = total.unwrap_or(0);

            if now == chunk as u64
                || (total > 0 && now >= total)
                || now - last_sent.get() >= PROGRESS_STEP
            {
                last_sent.set(now);
                events.progress(progress_id, label, now, total);
            }
        },
        || {},
    )?;

    let total = downloaded.get().max(1);
    events.progress(progress_id, label, total, total);
    Ok(bytes)
}

/// Stages the package in the temp dir and installs it with elevated privileges.
#[cfg(target_os = "linux")]
fn install_package_file(bytes: &[u8], ext: &str) -> anyhow::Result<()> {
    let path = std::env::temp_dir().join(format!("oneclient-update-{}.{ext}", Uuid::new_v4()));
    std::fs::write(&path, bytes)?;

    let result = run_package_manager(&path, ext);
    let _ = std::fs::remove_file(&path);
    result
}

/// First candidate per package type resolves dependencies; later ones are last-ditch
/// fallbacks for minimal systems.
#[cfg(target_os = "linux")]
fn run_package_manager(path: &std::path::Path, ext: &str) -> anyhow::Result<()> {
    let file = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("update path is not valid UTF-8"))?;

    let candidates: Vec<Vec<&str>> = match ext {
        "deb" => vec![
            vec!["apt-get", "install", "-y", file],
            vec!["dpkg", "-i", file],
        ],
        _ => vec![
            vec!["dnf", "install", "-y", file],
            vec!["yum", "install", "-y", file],
            vec!["zypper", "--non-interactive", "install", file],
            vec!["rpm", "-Uvh", "--replacepkgs", file],
        ],
    };

    let mut failures: Vec<String> = Vec::new();
    for args in candidates {
        let (program, rest) = args
            .split_first()
            .expect("candidate command always has a program");
        match pkexec(program, rest) {
            Ok(()) => return Ok(()),
            Err(err) if err.downcast_ref::<UpdateCancelled>().is_some() => return Err(err),
            Err(err) => failures.push(format!("{program}: {err:#}")),
        }
    }

    anyhow::bail!(failures.join("; "))
}

#[cfg(target_os = "linux")]
fn pkexec(program: &str, args: &[&str]) -> anyhow::Result<()> {
    use std::process::{Command, Stdio};

    let output = Command::new("pkexec")
        .arg(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("could not start pkexec {program} - is polkit installed?"))?;

    if output.status.success() {
        return Ok(());
    }

    match output.status.code() {
        // polkit: 126 = dismissed by the user, 127 = not authorized / no auth agent
        Some(126) | Some(127) => Err(UpdateCancelled.into()),
        _ => anyhow::bail!("exited with {}{}", output.status, stderr_suffix(&output.stderr)),
    }
}

#[cfg(target_os = "linux")]
fn stderr_suffix(stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        String::new()
    } else {
        format!(": {stderr}")
    }
}
