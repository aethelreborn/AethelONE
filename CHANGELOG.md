## 2.12.0 (2026-10-09)

### Features

- Right-clicking an instance on the Versions page now opens a context menu with quick navigation (Overview, Logs, Screenshots, Mods, Shaders, Textures, Settings), Open folder and Copy path, plus Edit, Duplicate and Delete actions.

### Fixes

- Bound all upstream requests made by the local cosmetics proxy (15s total per request, 15s websocket handshake) so a stalled backend can no longer hang the game's cosmetics screen, and serve the cached cosmetic catalog from disk instantly while revalidating it in the background.
- SkyBlock content installed without asking is removed from 26.3 clusters once, and turning an optional bundle down removes its mods

## 2.11.1 (2026-10-09)

### Fixes

- Fix auto-updates timing out on slow connections

## 2.11.0 (2026-10-07)

### Features

- Open content folders from the Mods, Shaders, and Textures toolbars

- Duplicate an instance

- GC presets for JVM arguments

- Update all outdated content at once

- Back up all worlds at once

- Worlds can be backed up to and restored from .zip archives

- Import worlds from the Worlds toolbar

- Select multiple worlds to back up or delete at once

- Sort worlds by name or by most recent

## 2.10.0 (2026-10-07)

### Features

- Bundle-managed content is now removable behind a new opt-in setting

- Recommended tab surfaces a curated, cluster-aware mod catalog

- Settings gains manual and automatic update controls

- Worlds can now be renamed and duplicated

## 2.9.4 (2026-10-06)

### Fixes

- Fix updater public key corruption that broke every auto-update since 2.8.3

## 2.9.3 (2026-10-06)

### Fixes

- The active skin now reaches the game (CustomSkinLoader reads {USERNAME}.png)

## 2.9.2 (2026-10-06)

### Fixes

- Stop the launch crash by installing CustomSkinLoader 15.1 from GitHub

- Upstream sync: custom game arguments, mods folder sync, and browser links

## 2.9.1 (2026-10-05)

### Fixes

- The Skins tab has working SET buttons again

## 2.9.0 (2026-10-05)

### Features

- Offline custom skins now live in their own Skins tab

## 2.8.3 (2026-10-05)

### Fixes

- Fix automatic updates failing with "Invalid encoding in minisign data"

## 2.8.2 (2026-10-05)

### Fixes

- Offline accounts can now finish Poly+ login and open the unlocked locker

## 2.8.1 (2026-10-04)

### Fixes

- Fix the 2.8.0 release feedback: Debian upgrade collision, updater 401/403 retries, cosmetics-unlock diagnostics

## 2.8.0 (2026-10-04)

### Features

- The app is now branded AethelONE: window and tray titles, onboarding, settings copy, update notifications, Discord presence, and installer product name

- AethelONE

## 2.7.0 (2026-10-04)

### Features

- Code blocks are selectable, and the open link confirmation has a copy button by [LynithDev](https://github.com/LynithDev)
- Linux deb and rpm installs can now update themselves: the package and its signature are downloaded from the release, verified, and installed through the system package manager
- Game logs hide account tokens, and the game output starts with the launch command ready to paste into a terminal by [LynithDev](https://github.com/LynithDev)
- The changelog page now shows each GitHub release's notes by [LynithDev](https://github.com/LynithDev)
- Offline accounts no longer require a Microsoft account: adding, defaulting and launching them works standalone, including from the account onboarding step
- Game settings have a Game Arguments field for passing extra arguments to Minecraft, globally or per cluster by [Wyvest](https://github.com/Wyvest)
- GitHub hosted mods have a "View in browser" option that opens their repository by [LynithDev](https://github.com/LynithDev)
- The mods folder now syncs with the launcher: jars added, renamed or replaced by hand show up in the Mods list, and jars deleted by hand are switched off instead of being put back by [Wyvest](https://github.com/Wyvest)
- Choose which GPU the game renders on by [LynithDev](https://github.com/LynithDev)

### Fixes
- Icons show on Linux systems with a comma decimal locale by [LynithDev](https://github.com/LynithDev)
- The "Uploaded to mclo.gs" notification no longer repeats when switching back to the Logs tab by [LynithDev](https://github.com/LynithDev)
- Open folder and open in browser work again on Linux by [LynithDev](https://github.com/LynithDev)
- Optional bundles are now offered before the first launch of every new version instead of being installed or skipped silently by [Wyvest](https://github.com/Wyvest)
- System information in the settings sidebar shows a tooltip explaining it can be clicked to copy by [LynithDev](https://github.com/LynithDev)
