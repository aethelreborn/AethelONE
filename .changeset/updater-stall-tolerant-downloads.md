---
default: patch
---

# Fix auto-updates timing out on slow connections

Self-update downloads failed with "error decoding response body: operation timed out" on
connections that stalled briefly mid-transfer: reqwest arms `TCP_USER_TIMEOUT` at 30 seconds
on Linux by default, and cargo-packager-updater builds its HTTP client with no way to widen
that, so every retry met the same wall. Downloads now run through a stall-tolerant client
(up to a 2-minute stall, 30-minute overall budget) that performs the same minisign signature
verification before installing, with the decode chain pinned by a test against a real
release signature.
