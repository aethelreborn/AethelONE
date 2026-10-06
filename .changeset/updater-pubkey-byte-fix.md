---
default: patch
---

# Fix updater public key corruption that broke every auto-update since 2.8.3

The `UPDATER_PUBKEY` constant was damaged by a single-byte transcription error when the
double-encoded value was unwrapped in 2.8.3. The key still decoded fine, and its `key_id`
still matched, but the actual public key bytes no longer matched the release signing key —
so every update attempt failed with "The signature verification failed" (issue #3).
Older pre-2.8.3 builds failed earlier with "Invalid encoding in minisign data" from the
double encoding itself. Release signatures are verified against the corrected key and a
key-material pin test now guards against this class of regression.
