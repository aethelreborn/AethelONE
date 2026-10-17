---
default: minor
---

Launch Guard: worlds are now snapshotted into the version's launcher folder before any launch that would change them. Snapshots are zip files kept per world with a configurable retention (5 by default, opt-in in Launcher settings), and the Worlds page gained a Snapshots dialog to browse, create, restore and delete them — no launcher has automatic world backups today, and MultiMC/Prism users have been asking for exactly this for years.
