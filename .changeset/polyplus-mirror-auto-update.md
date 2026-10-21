---
default: minor
---

PolyPlus now updates itself in the launcher. The launcher used to ask Modrinth for the newest PolyPlus release, and that release never carries the patched build, so the mod stayed on whatever version it was installed with. The mirror manifest decides instead: the newest build for the instance's game version is offered as a normal mod update, it is applied with the launcher's own auto-update mode before a launch, and the patched jar is the one that lands in the instance.
