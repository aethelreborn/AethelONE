use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use oneclient_common::domain::ContentType;

const PROBE_LEN: usize = 4096;

const MAX_PROBE_BYTES: u64 = 32 * 1024 * 1024;

const BULK_NAMES: [&str; 4] = ["cache", "repo", "backup", "log"];

const NEVER_PROBED: [&str; 8] = [
    "db", "sqlite", "sqlite3", "wal", "shm", "journal", "lock", "idx",
];

const SWEPT_DIRS: [&str; 1] = ["config"];

const SWEPT_CONTENT: [(ContentType, bool); 3] = [
    (ContentType::Mod, true),
    (ContentType::ResourcePack, false),
    (ContentType::Shader, false),
];

#[tracing::instrument(level = "debug")]
pub async fn clear_zeroed_mods(cluster_dir: &Path) -> usize {
    let mods_dir = cluster_dir.join(ContentType::Mod.folder_name());

    tokio::task::spawn_blocking(move || {
        // remove_empty: a zero-byte jar is never valid and makes the loader
        // abort with "zip file is empty" (AethelOnePLUS issue #1).
        let cleared = sweep_dir(&mods_dir, true, true);

        if cleared > 0 {
            tracing::info!(
                cleared,
                mods_dir = %mods_dir.display(),
                "cleared empty or zero-filled files; affected mods will regenerate defaults"
            );
        }

        cleared
    })
    .await
    .unwrap_or(0)
}

#[tracing::instrument(level = "debug")]
pub async fn clear_zeroed_files(game_dir: &Path) -> usize {
    let game_dir = game_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        // The general sweep leaves zero-byte files alone: an empty config file
        // can be meaningful, but empty content (mods, packs, shaders) cannot.
        let mut cleared = sweep_dir(&game_dir, false, false);

        for dir in SWEPT_DIRS {
            cleared += sweep_dir(&game_dir.join(dir), true, false);
        }

        for (content_type, recurse) in SWEPT_CONTENT {
            cleared += sweep_dir(&game_dir.join(content_type.folder_name()), recurse, true);
        }

        if cleared > 0 {
            tracing::info!(
                cleared,
                game_dir = %game_dir.display(),
                "cleared empty or zero-filled files; affected mods will regenerate defaults"
            );
        }

        cleared
    })
    .await
    .unwrap_or(0)
}

fn sweep_dir(root: &Path, recurse: bool, remove_empty: bool) -> usize {
    let mut cleared = 0usize;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            let path = entry.path();

            if file_type.is_dir() {
                if recurse && !names_bulk_data(&path) {
                    stack.push(path);
                }
                continue;
            }

            if !file_type.is_file() || !worth_probing(&path) {
                continue;
            }

            let zeroed = is_zeroed(&path);
            let empty = remove_empty && is_empty_file(&path);

            if !zeroed && !empty {
                continue;
            }

            let reason = if zeroed {
                "zero-filled file left by an unclean shutdown"
            } else {
                "empty file, most likely a failed or interrupted download"
            };

            match fs::remove_file(&path) {
                Ok(()) => {
                    cleared += 1;
                    tracing::warn!(file = %path.display(), "removed {reason}");
                }
                Err(err) => tracing::warn!(
                    file = %path.display(),
                    error = %err,
                    "failed to remove {reason}"
                ),
            }
        }
    }

    cleared
}

fn names_bulk_data(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.split(|c: char| !c.is_ascii_alphanumeric())
                .any(|word| {
                    let word = word.strip_suffix(['s', 'S']).unwrap_or(word);
                    BULK_NAMES
                        .iter()
                        .any(|needle| word.eq_ignore_ascii_case(needle))
                })
        })
}

fn worth_probing(path: &Path) -> bool {
    !path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            NEVER_PROBED
                .iter()
                .any(|skip| ext.eq_ignore_ascii_case(skip))
        })
}

fn is_empty_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.len() == 0)
        .unwrap_or(false)
}

fn is_zeroed(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };

    let mut buf = [0u8; PROBE_LEN];
    let mut read_so_far = 0u64;

    loop {
        let read = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => return false,
        };

        if buf[..read].iter().any(|&b| b != 0) {
            return false;
        }

        read_so_far += read as u64;

        if read_so_far > MAX_PROBE_BYTES {
            return false;
        }
    }

    read_so_far > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_removes_empty_files_only_when_asked_and_always_removes_zero_filled() {
        let dir = std::env::temp_dir().join(format!("heal-sweep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let empty = dir.join("mod.jar");
        fs::write(&empty, b"").unwrap();

        let zero_filled = dir.join("corrupt.jar");
        fs::write(&zero_filled, [0u8; 128]).unwrap();

        let good = dir.join("fine.jar");
        fs::write(&good, b"not empty").unwrap();

        // Empty files survive a sweep that does not ask for their removal;
        // zero-filled ones never do.
        assert_eq!(sweep_dir(&dir, true, false), 1);
        assert!(empty.exists());
        assert!(!zero_filled.exists());

        // When asked, the empty file goes too and the healthy one stays.
        fs::write(&zero_filled, [0u8; 128]).unwrap();
        assert_eq!(sweep_dir(&dir, true, true), 2);
        assert!(!empty.exists());
        assert!(!zero_filled.exists());
        assert!(good.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bulk_names_match_whole_words_only() {
        for bulk in [
            "cache",
            "logs",
            "backups",
            "catharsis-repo-cache",
            "meowdding-repo-cache",
            "config_backup.json",
        ] {
            assert!(names_bulk_data(Path::new(bulk)), "{bulk} should be skipped");
        }

        for swept in [
            "NoChatReports",
            "NCR-Common.json",
            "crash-reports",
            "catalog.json",
        ] {
            assert!(
                !names_bulk_data(Path::new(swept)),
                "{swept} should be swept"
            );
        }
    }
}
