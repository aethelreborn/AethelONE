#![cfg(test)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A single shared data directory for every file-backed test in this crate.
/// Tests run in parallel threads, so the launcher's global data dir must be
/// pointed at the same root no matter which test calls in first.
pub fn data_dir() -> &'static Path {
    TEST_DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!(
            "oneclient-cluster-tests-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        oneclient_common::paths::set_data_dir(dir.clone());
        dir
    })
}

static TEST_DIR: OnceLock<PathBuf> = OnceLock::new();
