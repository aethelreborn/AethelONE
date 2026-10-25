## 2.38.0 (2026-10-25)

### Features

- Added an Updates tab to the content pages

## 2.37.1 (2026-10-25)

### Fixes

- Fixed wardrobe layout in the cosmetics page

## 2.37.0 (2026-10-24)

### Features

- Added a cosmetics wardrobe to the launcher

## 2.36.0 (2026-10-22)

### Features

- The recent sessions timeline can now be read over a range. It always showed the last eighty finished sessions, which is a lot to scroll when the last week is what you want to look at - the card now carries a picker for the last seven, thirty or ninety days, or all of them, and says which range it is showing. The range applies to the Stats page and to an instance's overview alike.

## 2.35.0 (2026-10-22)

### Features

- The launcher now says what it changed before a launch. When a bundle's content is synced - a mod updated, one added, one taken away - the changes used to happen under the user and the mods simply appeared, or vanished, between one launch and the next. The launch now carries a toast that names every file that was updated, added or removed, so a mod that is suddenly gone has an answer.
- The launcher now says when the game is being drawn by the processor. A machine with no working graphics driver - a laptop whose driver was never installed, or a card the game fell back to software for - draws the whole game on the processor, and then no setting can make it fast: the frames are made by the same processor that runs the game. The mod can see this while the game runs, but the launcher can say it before the next launch: it reads the renderer out of the log the last launch left, and when that renderer is a software one the launch carries a toast that names it and says the machine needs a working graphics driver. On a laptop that driver usually comes from the maker of the machine.

## 2.34.0 (2026-10-21)

### Features

- The storage page now shows what is using the space as a picture: the categories and the clusters are drawn as blocks whose widths are their share of the whole, and the bigger the block the stronger the colour, so the one that eats the disk is the one the eye lands on. The numbers are still there underneath, and a block too small for its name stays empty instead of spilling over its neighbour.

## 2.33.0 (2026-10-21)

### Features

- The launcher now says when the game is asking for more memory than the machine can give. On a small laptop the system starts paging once the game takes more than it can afford, and paging shows up as stutter that looks like the game's fault - so the first launch says how much memory this machine can give the game before the system starts swapping, and the Performance Mode page repeats it under the mode. The hint is said once per session, not on every launch.

## 2.32.0 (2026-10-21)

### Features

- Added an ability to export your OneClient clusters, compatible with other launchers too
- A saved server can now be given a name. The servers page grew a pencil button next to every row: it turns the name into a field you can type in, and saving it keeps the name for good - the row then shows the name with the address underneath it. A blank name takes the name away again, so a server goes back to showing its address.
- Added syncing up dedicated clusters with shared directory data (settings, worlds, config etc.) by [rozwader](https://github.com/rozwader)

- Clusters can import modpacks

- Added importing instances from other launchers, such as: Modrinth App and Prism Launcher

### Fixes

- Added organizations showing up in mods pages in browser by [rozwader](https://github.com/rozwader)
- Added warning when a user is trying to install a different version of already bundled mod
- Added a "Deleted" tab in mods list, where deleted bundled mods are stored by [rozwader](https://github.com/rozwader)
- Added changelog button for mods in the update modal by [rozwader](https://github.com/rozwader)
- Disabled ability to search CurseForge on Orinthe clusters (CurseForge doesn't provide Orinthe compatible mods) by [rozwader](https://github.com/rozwader)
- Fixed mods auto enabling (after being disabled) when added from declined bundles (eg. skyblock bundle) by [rozwader](https://github.com/rozwader)
- Added option to choose from dedicated and shared dir while creating OneClient cluster instance by [rozwader](https://github.com/rozwader)
- Added hidden packages filter turning on when user searches packages thru searchbar, by [rozwader](https://github.com/rozwader)
- Added open link button for new OneClient version toasts by [rozwader](https://github.com/rozwader)
- Added a button for opening logs from launcher by [rozwader](https://github.com/rozwader)
- Added a dropdown for opengl/vulkan graphic backend in cluster with version 26.2 and above by [rozwader](https://github.com/rozwader)
- Changed the order of settings in cluster settings by [rozwader](https://github.com/rozwader)
- Fixed text wrapping up weirdly in the new instance creator by [rozwader](https://github.com/rozwader)

- Added a cluster reset button, that resets cluster back to it's factory settings: mods, config, packages.

## 2.31.0 (2026-10-21)

### Features

- Every instance can now pick its own Performance Mode: the instance settings carry a Performance Mode row that offers the global setting or a mode of its own, so a heavy pack can keep the memory it needs while a plain instance on the same machine runs the low-spec mode. An instance that overrides one setting now inherits the rest of the global ones instead of losing them.
- The Mods page of an instance now opens with a Performance set: the five mods that raise the frame rate the most - Sodium, Lithium, ImmediatelyFast, FerriteCore and Entity Culling - each with its own install button and one button that installs everything the instance is missing. The set only offers what the instance's loader and game version can actually run, the mods it already carries are marked as installed, and nothing is installed without a click.
- PolyPlus now updates itself in the launcher. The launcher used to ask Modrinth for the newest PolyPlus release, and that release never carries the patched build, so the mod stayed on whatever version it was installed with. The mirror manifest decides instead: the newest build for the instance's game version is offered as a normal mod update, it is applied with the launcher's own auto-update mode before a launch, and the patched jar is the one that lands in the instance.

## 2.30.0 (2026-10-21)

### Features

- Performance Mode makes the launcher tune the game for the machine it runs on instead of only promising to. Pick Off, Balanced or Low-spec under Settings → Minecraft: the launcher tells the garbage collector how long a frame may wait, hands the game a heap the system can afford so a small laptop stops paging, and writes the video settings the mode promises - render distance, simulation distance, particles, entity distance, clouds and, on the low-spec mode, fast graphics - into the game's own options file before every launch. The settings page names your machine and says which mode fits it, and lists every change the mode makes.

## 2.29.1 (2026-10-19)

### Fixes

- Closing AethelONE while a game is playing no longer cuts the game off from the local cosmetics unlock, playtime tracking or Discord presence: the window hides instead of quitting while a launch is in flight or a game is running. The tray menu's "Quit AethelONE" still closes everything for real.

## 2.29.0 (2026-10-18)

### Features

- Home now shows a "What's new" card whenever the newest release notes have not been opened yet: the latest changelog entry renders right on the landing page with a link into the full changelog, and the card disappears by itself once you have read it in Settings.

## 2.28.0 (2026-10-18)

### Features

- Stats now opens with a Recent sessions timeline: your last 80 finished sessions, grouped by day (Today, Yesterday, then dates), each showing the instance, start and end clock times, duration and a bar scaled to the day's longest session, with a day total beside the header. The same card appears on each instance's overview, filtered to that instance.

## 2.27.0 (2026-10-18)

### Features

- Instances can now be organised into groups (folders) on the Versions page: right-click an instance and choose "Move to group…" to file it under a name or take it back out. Groups render as collapsible sections with instance counts, sit after your AethelONE versions, respect search/filters and pinning, and existing group names are one click away when refiling.

## 2.26.0 (2026-10-18)

### Features

- Saved servers now remember you: each row shows your visit count, total time played and how long ago you last joined, matched against session history even when the address is spelled differently (`host` vs `host:25565`, stray FQDN dots, mixed case). The instance picker also preselects the version you last joined with, so one click puts you back where you left off.

## 2.25.0 (2026-10-18)

### Features

- New Servers page: save Minecraft servers and see them live. Each row performs a real Server List Ping (the 1.7+ status protocol) showing a status dot, MOTD, version, player count and round-trip latency, and can launch any instance straight onto the server with Quick Play. Addresses are deduplicated between `host` and `host:25565` spellings, and removed servers disappear from the list immediately.

## 2.24.0 (2026-10-18)

### Features

- Statistics gains a playtime calendar: a GitHub-style 26-week heatmap where every day is a cell shaded by how long you played it (level thresholds computed from your own quartiles), with month and weekday labels, a Less/More legend, and a hover readout naming the exact date and duration. Streaks, dry spells and your personal best day at a glance.

## 2.23.0 (2026-10-18)

### Features

- Add a Configs tab to every instance: browse and edit the text files mods read at startup (everything under `config/` plus `options.txt`, `optionsof.txt` and `iris.properties`) without leaving the launcher. Writes go through an atomic replace, paths that escape those folders are rejected, and unsaved edits are guarded by a discard prompt before switching files.

## 2.22.0 (2026-10-18)

### Features

- Crash notifications now say what changed since the last launch. Every session records the enabled mod set, and a crash compares against the previous run to name mods that were added, removed or updated (with version arrows) right in the notification — the question after a crash is always "what changed", and now the launcher answers it without the user counting jars.

## 2.21.0 (2026-10-17)

### Features

- Crash triage now offers a fix: when the named suspect is a mod the cluster has installed, the crash flow asks whether to disable it so the game can start again — one click instead of hunting through the mod list — and the Mods page picks the change up on its own.

## 2.20.0 (2026-10-17)

### Features

- Crash triage: when a game crashes, the launcher now reads the newest crash report, pulls out Minecraft's own description line and names the mods the report blames — NeoForge/Forge "Suspected Mod" sections or the first stack frames on Fabric — right in the crash notification, so finding the culprit no longer means reading a stack trace. Reports from before the session are ignored, only enabled mods are considered, and generic classpath words are filtered.

## 2.19.0 (2026-10-17)

### Features

- Launch Guard: worlds are now snapshotted into the version's launcher folder before any launch that would change them. Snapshots are zip files kept per world with a configurable retention (5 by default, opt-in in Launcher settings), and the Worlds page gained a Snapshots dialog to browse, create, restore and delete them — no launcher has automatic world backups today, and MultiMC/Prism users have been asking for exactly this for years.

## 2.18.0 (2026-10-17)

### Features

- The Mods page now scans the instance's jars offline and warns about duplicate mod ids (which stop the game from starting), jars built for a different loader, required dependencies that are not installed, and jars that are unreadable or carry no mod metadata — problems that otherwise surface as a crash or a silent no-op after launch.

## 2.17.0 (2026-10-17)

### Features

- Local capes and elytras now also apply to premium accounts: CustomSkinLoader keeps the skin and cape Mojang serves for a signed-in account, and a local cape or elytra fills in when the account has none of its own. CustomSkinLoader is now installed for premium accounts too, but only when a cape or elytra is equipped.

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
