use std::path::Path;

use async_zip::tokio::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures_lite::io::AsyncWriteExt;
use tokio::io::{AsyncReadExt, AsyncWrite};

/// Streams `dir` into `writer` with entries at the archive root, skipping
/// symlinks. Errors carry the raw message so callers pick their own wording.
pub(crate) async fn zip_dir_recursive<W: AsyncWrite + Unpin>(
    writer: &mut ZipFileWriter<W>,
    rel: &str,
    dir: &Path,
) -> Result<(), String> {
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|err| err.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|err| err.to_string())? {
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type().await.map_err(|err| err.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        let rel_path = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        if file_type.is_dir() {
            Box::pin(zip_dir_recursive(writer, &rel_path, &entry.path())).await?;
        } else if file_type.is_file() {
            let builder = ZipEntryBuilder::new(rel_path.into(), Compression::Deflate);
            let mut file = tokio::fs::File::open(entry.path())
                .await
                .map_err(|err| err.to_string())?;
            let mut entry_writer = writer
                .write_entry_stream(builder)
                .await
                .map_err(|err| err.to_string())?;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = file
                    .read(&mut buffer)
                    .await
                    .map_err(|err| err.to_string())?;
                if read == 0 {
                    break;
                }
                entry_writer
                    .write_all(&buffer[..read])
                    .await
                    .map_err(|err| err.to_string())?;
            }
            entry_writer.close().await.map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}
