# Iterations shipped (complete history)

All feature work is on `main` of `aethelreborn/AethelONE`, released via the
Upstream Sync workflow. Commit hashes are the feature commits (release commits
are separate `chore: release X.Y.Z` commits — `git log` shows both).

| # | Feature | Version | Feature commit | Verified |
|---|---|---|---|---|
| 1 | Mod health check (broken/missing deps surfaced) | 2.18.0 | (early session) | nightly+release green |
| 2 | Launch-guard world snapshots | 2.19.0 | (early session) | nightly+release green |
| 3 | Crash triage (parse crash reports, plain-language cause) | 2.20.0 | (early session) | nightly+release green |
| 4 | Suspect-disable offer + sync hardening | 2.21.0 | (early session) | nightly+release green |
| 5 | "What changed since last launch" (mods snapshot diff) | 2.22.0 | migration `20261018120000_session_mod_snapshot.sql` | nightly+release green |
| 6 | Per-instance config editor (two-pane, allowlist, atomic writes) | 2.23.0 | `267a0208` | 18 assets, deb ok |
| 7 | Playtime calendar heatmap (26-week GitHub-style grid) | 2.24.0 | `edb79f93` | 18 assets, deb ok |
| 8 | Servers page with live SLP status | 2.25.0 | `8bc6c863` | 18 assets, deb ok |
| 9 | Server visit history on saved servers (visits/duration/last played) | 2.26.0 | `28481900` | 18 assets, deb ok |
| 10 | Instance groups (collapsible folders on Versions page) | 2.27.0 | `1113b9bf` | 18 assets, deb ok |
| 11 | Recent sessions timeline (analytics + per-instance overview) | 2.28.0 | `f113bf4e` | 18 assets, deb ok |
| 12 | Home "What's new" card for unread changelog | 2.29.0 | `4cbba9a5` (release `1409a1ca`) | 18 assets, deb ok |
| 13 | Keep the launcher running while a game plays (cosmetics proxy stays up) | 2.29.1 | `640432fa` (release `dbb9d9b9`) | 18 assets, deb ok |

Latest release commits at handoff: `1ba6b296` (2.28.0), `cae690bf` (2.27.0),
`59b19f19` (2.26.0), `ab300828` (2.25.0).

## What each feature added (for orientation)

1. **Mod health**: flags mismatched/broken mod dependencies in an instance.
2. **Launch-guard snapshots**: before first launch after a change, world files
   are snapshotted so a bad update can be rolled back.
3. **Crash triage**: when a game exits non-zero, the launcher parses the crash
   report and explains the likely cause in plain language.
4. **Suspect-disable offer**: after a crash, offer to disable the mod most
   likely responsible; hardened sync so partial syncs retry.
5. **What changed**: diff of enabled mods since last launch (runtime queries,
   `mods_snapshot` table).
6. **Config editor** (`view/app/cluster/configs.rs`, core `cluster/configs`):
   browse & edit instance config files (`config/*.cfg` etc. allowlist, 1 MiB
   cap, atomic tmp+rename, symlink-skipping).
7. **Heatmap**: `view/app/analytics/heatmap.rs`; 26-week grid, quartile
   thresholds, month/weekday labels, hover readout.
8. **Servers page**: `oneclient_net/src/slp.rs` SLP ping implementation
   (handshake protocol -1, status request, chat-component MOTD flattening);
   `saved_servers` table; page with status/MOTD/latency/players; Quick Play
   join of any instance; address normalisation (host vs host:25565 dedupe).
9. **Server history**: joins/playtime/last-played per saved server matched
   against session history with the same normaliser; dropdown preselects the
   last-used cluster.
10. **Instance groups**: nullable `group_name` on clusters; runtime DAO;
    collapsible sections on the Versions page; "Move to group…" in the
    instance context menu; quick-pick chips of existing groups.
11. **Sessions timeline**: DAO join of `game_sessions` × `clusters` (newest
    first, optional cluster filter); core `PlaySession` +
    `group_sessions_by_day`; day-grouped card with bars scaled to the day's
    longest session; appears globally (Stats) and per-instance (overview).
12. **What's new**: Home card shows the newest changelog entry while unread;
    "Read it all" → `Route::SettingsChangelog {}`; disappears once seen
    (`seen_changelog_version`).

## Confirmed-covered gaps (do NOT build these — they already exist)

- Quick-join multiplayer (`launch_server` action + analytics/servers.rs)
- Quick-play singleplayer into a specific world (`launch_world`, worlds page)
- Crash triage / instance repair, log upload (mclogs), log viewer w/ search
- Migration from other launchers (import), instance export/import (.zip)
- Duplicate instance, rename, per-instance covers/tags/descriptions
- World rename/duplicate/backup/snapshots/datapacks, world deletion
- Screenshot manager incl. upload/share, per-instance screenshots
- Modrinth/CurseForge browse with compat filtering, package updates
  (mods/shaders/textures via package_updates), modpack install from file
- Java runtime management, per-instance Java overrides
- Accounts (Microsoft/offline), skins tab, cosmetics integration
- Cluster sorting (4 modes) + favorites pinning; view layout toggle
- Storage report (settings page) with reclaimable entries + cleanup actions
- Changelog page (settings), seen-version tracking (dot in settings sidebar)
- Launching with Quick Play (Multiplayer) requires MC >= 1.20
  (`supports_quick_play` guard)

## Test counts at handoff (local, all green)

- `oneclient_db`: 32 tests (`cargo test -p oneclient_db -j 1`)
- `oneclient_core`: 219 tests (`cargo test -p oneclient_core --lib -j 1`)
- `oneclient_cluster`: 34 tests (`cargo test -p oneclient_cluster --lib -- --test-threads=1`)
- `oneclient_net`: 28 tests (includes 5 slp tests)
- `oneclient_app` lib tests: 272 pass, 1 pre-existing failure unrelated to us
  (see LESSONS.md). Update 2026-10-19: CI now runs app tests too, skipping
  that one failure (`--skip a_disabled_override_on_an_unlinked_pin_offers_enable`),
  plus the full workspace suite - see oneclient_nightly.yml.
