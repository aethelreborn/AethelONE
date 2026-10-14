use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bytes::Bytes;
use futures_lite::Stream;
use futures_util::StreamExt;

use crate::error::RequestError;

/// One-second fixed window; a boundary can briefly burst up to ~2x the limit,
/// which is well within what a launcher user will notice
const WINDOW: Duration = Duration::from_secs(1);

/// Shared download speed cap. A limit of `0` means unlimited. The limiter is
/// read on every chunk so a settings save mid-download takes effect immediately
/// without restarting the transfer.
pub struct DownloadLimiter {
    bytes_per_sec: AtomicU64,
    window: Mutex<Window>,
}

struct Window {
    start: tokio::time::Instant,
    used: u64,
}

impl DownloadLimiter {
    #[must_use]
    pub fn new(bytes_per_sec: u64) -> Self {
        Self {
            bytes_per_sec: AtomicU64::new(bytes_per_sec),
            window: Mutex::new(Window {
                start: tokio::time::Instant::now(),
                used: 0,
            }),
        }
    }

    pub fn set_limit(&self, bytes_per_sec: u64) {
        self.bytes_per_sec.store(bytes_per_sec, Ordering::Relaxed);
    }

    /// Waits until `bytes` fit inside the current one-second window. A single
    /// chunk larger than the whole limit is let through on an empty window so
    /// a limit below the transport chunk size cannot deadlock the transfer.
    pub async fn acquire(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);

        loop {
            let limit = self.bytes_per_sec.load(Ordering::Relaxed);
            if limit == 0 {
                return;
            }

            let wait = {
                let mut window = self.window.lock().unwrap_or_else(|e| e.into_inner());
                let now = tokio::time::Instant::now();
                if now.duration_since(window.start) >= WINDOW {
                    window.start = now;
                    window.used = 0;
                }

                if window.used == 0 || window.used.saturating_add(bytes) <= limit {
                    window.used = window.used.saturating_add(bytes);
                    None
                } else {
                    Some(window.start + WINDOW - now)
                }
            };

            match wait {
                None => return,
                Some(delay) => tokio::time::sleep(delay).await,
            }
        }
    }
}

/// Wraps a body stream so each chunk waits for its share of the speed limit
/// before being yielded downstream. Backpressure propagates: while a chunk is
/// waiting the underlying stream is not polled, so the socket stalls too.
pub fn throttle<S>(
    stream: S,
    limiter: std::sync::Arc<DownloadLimiter>,
) -> impl Stream<Item = Result<Bytes, RequestError>> + Send
where
    S: Stream<Item = Result<Bytes, RequestError>> + Send,
{
    stream.then(move |item| {
        let limiter = std::sync::Arc::clone(&limiter);
        async move {
            if let Ok(chunk) = &item {
                limiter.acquire(chunk.len()).await;
            }
            item
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn unlimited_never_waits() {
        let limiter = DownloadLimiter::new(0);
        let start = tokio::time::Instant::now();

        limiter.acquire(50 * 1024 * 1024).await;

        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn within_the_limit_does_not_wait() {
        let limiter = DownloadLimiter::new(1000);
        let start = tokio::time::Instant::now();

        limiter.acquire(600).await;
        limiter.acquire(400).await;

        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn over_the_limit_waits_for_the_next_window() {
        let limiter = DownloadLimiter::new(1000);
        let start = tokio::time::Instant::now();

        limiter.acquire(600).await;
        limiter.acquire(600).await;

        assert!(
            start.elapsed() >= WINDOW,
            "second acquire must wait for the window to roll over, waited {:?}",
            start.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_chunk_larger_than_the_limit_still_completes() {
        let limiter = DownloadLimiter::new(100);
        let start = tokio::time::Instant::now();

        limiter.acquire(10 * 1024).await;

        assert_eq!(
            start.elapsed(),
            Duration::ZERO,
            "an oversized first chunk must not deadlock"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn clearing_the_limit_releases_waiters() {
        let limiter = DownloadLimiter::new(1000);
        limiter.acquire(1000).await;

        let waiter = {
            let limiter = std::sync::Arc::new(limiter);
            let task = {
                let limiter = std::sync::Arc::clone(&limiter);
                tokio::spawn(async move { limiter.acquire(500).await })
            };
            // Give the task a chance to register its wait
            tokio::task::yield_now().await;
            limiter.set_limit(0);
            task
        };

        waiter
            .await
            .expect("waiter completes once the limit clears");
    }
}
