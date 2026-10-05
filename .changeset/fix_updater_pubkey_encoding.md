---
default: patch
---

# Fix automatic updates failing with "Invalid encoding in minisign data"

The updater public key was stored double-base64-encoded, but
cargo-packager-updater decodes it exactly once before handing it to
minisign's public key parser, which then found a single unwrapped line
instead of the expected comment-plus-key text and aborted every install
with "Invalid encoding in minisign data" — on every platform, for every
update since the fork's release pipeline was introduced. The constant now
carries the single encoding the verifier expects, and a test pins it to
the minisign-verify contract so the layers cannot drift apart again.
