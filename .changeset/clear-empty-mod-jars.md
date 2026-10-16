---
default: patch
---

Clear zero-byte mod and pack files at launch: an empty jar in the mods folder makes Fabric abort with "zip file is empty", so the launch-time repair sweep now removes empty content files (mods, resource packs, shaders) alongside the zero-filled files left by unclean shutdowns. Config files are left alone.
