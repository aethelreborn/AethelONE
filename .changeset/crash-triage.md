---
default: minor
---

Crash triage: when a game crashes, the launcher now reads the newest crash report, pulls out Minecraft's own description line and names the mods the report blames — NeoForge/Forge "Suspected Mod" sections or the first stack frames on Fabric — right in the crash notification, so finding the culprit no longer means reading a stack trace. Reports from before the session are ignored, only enabled mods are considered, and generic classpath words are filtered.
