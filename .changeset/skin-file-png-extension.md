---
default: patch
---

# The active skin now reaches the game (CustomSkinLoader reads {USERNAME}.png)

The launcher wrote the local skin file without the `.png` extension CustomSkinLoader resolves, so the game found no local skin and fell back to the default one - on the title screen model and everywhere else. The file is now written as `LocalSkin/skins/<username>.png`, and the old extensionless file from earlier releases is cleaned up on launch.
