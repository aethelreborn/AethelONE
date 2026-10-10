---
default: minor
---

Serve PolyPlus from the AethelOnePLUS rolling mirror: downloads rewrite to the mirrored (cracked-uuid namebadge patched) jar when a manifest entry matches, bundle update checks track mirrored builds by their own identity, and everything falls back to the official Modrinth file whenever the mirror is unreachable.
