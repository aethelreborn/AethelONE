---
default: minor
---

Add a Configs tab to every instance: browse and edit the text files mods read at startup (everything under `config/` plus `options.txt`, `optionsof.txt` and `iris.properties`) without leaving the launcher. Writes go through an atomic replace, paths that escape those folders are rejected, and unsaved edits are guarded by a discard prompt before switching files.
