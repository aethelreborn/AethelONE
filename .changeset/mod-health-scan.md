---
default: minor
---

The Mods page now scans the instance's jars offline and warns about duplicate mod ids (which stop the game from starting), jars built for a different loader, required dependencies that are not installed, and jars that are unreadable or carry no mod metadata — problems that otherwise surface as a crash or a silent no-op after launch.
