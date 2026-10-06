---
default: patch
---

# Stop the launch crash by installing CustomSkinLoader 15.1 from GitHub

With an active skin, the launcher installs CustomSkinLoader into the cluster before launch; Modrinth still serves 15.0.1, which crashes Fabric on startup with a known mixin conflict (upstream fixed it in 15.1 and published it only on GitHub). The sync now sources the newest stable jar from the upstream GitHub release first — covering 1.8 through 26.3 — keeps Modrinth as a fallback, and never installs the known-bad 15.0.1 build.
