use bytes::Bytes;
use futures_lite::Stream;
use futures_util::StreamExt;

use crate::error::RequestError;

/// Shared download pause state. Clones share one watch channel, so flipping
/// the gate through any handle - the app's copy for instance - is seen by
/// every in-flight transfer.
#[derive(Clone)]
pub struct PauseGate {
    tx: tokio::sync::watch::Sender<bool>,
    rx: tokio::sync::watch::Receiver<bool>,
}

impl Default for PauseGate {
    fn default() -> Self {
        let (tx, rx) = tokio::sync::watch::channel(false);
        Self { tx, rx }
    }
}

impl PauseGate {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        *self.rx.borrow()
    }

    pub fn pause(&self) {
        self.tx.send_replace(true);
    }

    pub fn resume(&self) {
        self.tx.send_replace(false);
    }

    /// Completes immediately while downloads are running, otherwise sleeps
    /// until the gate reopens. The `watch` version counter makes the check
    /// race-free: a resume landing between the flag read and the wait still
    /// wakes it, because `changed()` compares against the version seen by
    /// `borrow_and_update` rather than against "now".
    pub async fn wait_resumed(&self) {
        let mut rx = self.rx.clone();
        while *rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                return;
            }
        }
    }
}

/// Wraps a body stream so no chunk flows while the gate is closed.
/// Backpressure propagates: the underlying stream is not polled while
/// waiting, so the socket stalls instead of buffering the whole file.
pub fn gate<S>(stream: S, gate: PauseGate) -> impl Stream<Item = Result<Bytes, RequestError>> + Send
where
    S: Stream<Item = Result<Bytes, RequestError>> + Send,
{
    stream.then(move |item| {
        let gate = gate.clone();
        async move {
            gate.wait_resumed().await;
            item
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn a_running_gate_never_waits() {
        let gate = PauseGate::new();
        let start = tokio::time::Instant::now();

        gate.wait_resumed().await;

        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn resuming_releases_a_pending_wait() {
        let gate = PauseGate::new();
        gate.pause();
        assert!(gate.is_paused());

        let mut waiter = {
            let gate = gate.clone();
            tokio::spawn(async move { gate.wait_resumed().await })
        };
        // Give the waiter a chance to register its wait
        tokio::task::yield_now().await;

        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut waiter)
                .await
                .is_err(),
            "a paused gate must keep waiters blocked"
        );

        gate.resume();
        waiter
            .await
            .expect("waiter completes once the gate reopens");
        assert!(!gate.is_paused());
    }

    #[tokio::test(start_paused = true)]
    async fn a_closed_gate_stalls_stream_chunks() {
        let gate = PauseGate::new();
        gate.pause();

        let mut stream = Box::pin(super::gate(
            futures_util::stream::iter(vec![Ok::<_, RequestError>(Bytes::from_static(b"chunk"))]),
            gate.clone(),
        ));

        let reader = tokio::spawn(async move { stream.next().await.map(|item| item.is_ok()) });
        tokio::task::yield_now().await;

        assert!(
            !reader.is_finished(),
            "no chunk may flow while the gate is closed"
        );

        gate.resume();

        assert_eq!(
            reader.await.expect("reader task completes"),
            Some(true),
            "the held chunk arrives once the gate reopens"
        );
    }
}
