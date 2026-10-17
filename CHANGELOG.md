## 2.16.1 (2026-10-17)

### Fixes

- Fixed sort by not applying to OneClient versions in the instance list by [LynithDev](https://github.com/LynithDev)

## 2.16.0 (2026-10-17)

### Features

- Add local capes and elytras: the Skins page now manages cape and elytra PNGs next to skins, and the launcher writes the equipped ones into CustomSkinLoader's LocalSkin folder at launch, so offline accounts wear them on the title screen, in singleplayer and on servers (locally). Unequipping a texture removes its file again.

## 2.15.2 (2026-10-16)

### Fixes

- Fixed the browser replaying its page animation and losing the picked cluster when switching cluster or type by [LynithDev](https://github.com/LynithDev)
- Clear zero-byte mod and pack files at launch: an empty jar in the mods folder makes Fabric abort with "zip file is empty", so the launch-time repair sweep now removes empty content files (mods, resource packs, shaders) alongside the zero-filled files left by unclean shutdowns. Config files are left alone.
- Fixed too long names overflowing in the drag and drop window by [rozwader](https://github.com/rozwader)
- Fixed filters resetting when you change a tab in cluster view by [rozwader](https://github.com/rozwader)
- Added ctrl + f shortcut for focusing the searchbar in logs and live log views by [rozwader](https://github.com/rozwader)

## 2.15.1 (2026-10-16)

### Fixes

- Fix upgrading the Linux .deb when the 2.8.0 build (Debian package `aethel-one`) is installed: the 2.14.0 and 2.15.0 packages had lost the legacy `Replaces`/`Conflicts` fields, so dpkg refused to overwrite `/usr/bin/oneclient_app` and the install failed. Both packaging workflows now declare the `aethel-one` transition again, and a guard fails the build if a package ever Conflicts with itself.

## 2.15.0 (2026-10-16)

### Features

- Change the Minecraft version of an instance: its settings page now has a Minecraft Version row that opens a searchable picker of every game version and moves the instance over, keeping its folder, worlds, mods and settings. Instances installed from a modpack stay on their modpack's version.
- Quick play: right-click a world on the Worlds page of an instance and choose Play into this world to start the game and load that save in one step, or hover a server on the Analytics Servers page and press Play to rejoin it on the version you last used with it (both need Minecraft 1.20 and newer).

## 2.14.0 (2026-10-14)

### Features

- Add a Check for updates button to the Mods, Shaders and Textures toolbars: it scans the selected instance's installed content for newer versions without launching the game, and reports the result in a toast.
- Add a command palette (Ctrl+K / Cmd+K) for jumping to pages, instances, settings sections and common actions from anywhere in the launcher.
- Add a configurable download speed limit in Settings → API connections: set a cap in KB/s (0 means unlimited) that applies to game, Java and content downloads.
- Import a previously exported instance: Versions page → Import instance accepts the .zip backup created by Export and recreates the instance with its settings and installed content.
- Pin instances to the top of the Versions page: right-click an instance and choose Pin to top to keep it first in the grid and list regardless of sort order, and Unpin to put it back.
- Serve PolyPlus from the AethelOnePLUS rolling mirror: downloads rewrite to the mirrored (cracked-uuid namebadge patched) jar when a manifest entry matches, bundle update checks track mirrored builds by their own identity, and everything falls back to the official Modrinth file whenever the mirror is unreachable.

### Fixes

- Fix several reliability issues: premium accounts no longer load customskinloader (it can shadow Mojang auth), zero-byte downloads are rejected instead of writing an empty file, per-cluster bundle update locks are released when an instance is deleted, and the download speed limiter no longer fails to compile on some toolchains.

## 2.13.0 (2026-10-09)

### Features

- Right-clicking an instance on the Versions page now offers Export, saving the whole instance folder as a .zip backup you can archive or move to another machine.
- Running games are now visible where you look: a "Running" badge appears on Home recents and Versions cards while the instance plays, Stop buttons show up next to Launch on the Home hero and Versions sidebar, and double-clicking an instance card launches it.
- The skins page now has an Equip button directly under the player preview: click a skin in the library and press Equip skin to make it active (the button turns into a disabled "Equipped" state when it already is).

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
