---
default: patch
---

Fix several reliability issues: premium accounts no longer load customskinloader (it can shadow Mojang auth), zero-byte downloads are rejected instead of writing an empty file, per-cluster bundle update locks are released when an instance is deleted, and the download speed limiter no longer fails to compile on some toolchains.
