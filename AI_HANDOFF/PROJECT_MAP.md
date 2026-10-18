# Project map — where everything lives

Paths relative to repo root. `app` = `packages/oneclient_app/src`.
The goal: a fresh session should never need to `grep -r` blindly for the
things that matter daily.

## Release plumbing

- `.github/workflows/` — `nightly.yml`, `upstream_sync.yml` (release entry:
  `gh workflow run upstream_sync.yml -R aethelreborn/AethelONE --ref main -f release=true`),
  release build + publish + changelog workflows.
- `.changeset/` — one markdown per feature; consumed by release commits.
- `AI_HANDOFF/` — this folder (delete when no longer needed).
- Version lives in `packages/oneclient_app/Cargo.toml` (bumped by CI).

## Database (sqlx)

- `packages/oneclient_db/migrations/` — timestamped SQL, additive only.
  Recent: `20261018120000_session_mod_snapshot.sql`,
  `20261018150000_saved_servers.sql`, `20261018170000_cluster_groups.sql`.
- `packages/oneclient_db/src/dao/` — one file per table group.
  **Runtime-query new code only**: `saved_server.rs`, `cluster_group.rs`,
  and `recent_sessions()` in `game_session.rs`. Everything older uses checked
  macros — DO NOT EDIT.
- `packages/oneclient_db/src/models/` — row structs (`#[derive(FromRow)]`),
  re-exported from `models/mod.rs`. Examples: `SavedServerRow`,
  `RecentSessionRow`, `NewCluster`.
- Test pattern: in-memory pool + `sqlx::migrate!()` inside the db crate only.

## Core (business logic)

- `packages/oneclient_core/src/servers/mod.rs` — saved servers + SLP
  re-exports + `host_port`/`matches_saved` normalisers (tests create table
  manually).
- `packages/oneclient_core/src/clusters/groups.rs` — instance groups
  (trim, 60-char cap, DAO wrapper).
- `packages/oneclient_core/src/game/analytics.rs` — `PlaytimeStats`,
  `ServerStat`, `Analytics`, `PlaySession`, `recent_sessions`,
  `group_sessions_by_day`, `Aggregate` helpers. Re-exported via
  `game/mod.rs`.
- `packages/oneclient_core/src/error.rs` — `LauncherError` enum (all error
  conversions live here; add a variant + `#[from]` when a new module needs
  one).
- `packages/oneclient_core/src/clusters/` — instance lifecycle: create,
  modpack ops, configs live in `oneclient_cluster` crate.

## Net

- `packages/oneclient_net/src/slp.rs` — Server List Ping client
  (`ping_server`, `ServerStatus`, `parse_address`, `DEFAULT_PORT`,
  `PingError`); tests include a local canned-listener round trip.

## App hooks (`app/hooks/`)

- `queries/` — one module per feature (`servers.rs`, `cluster_groups.rs`,
  `analytics.rs`, `configs.rs`, `bundles.rs`, …) each exporting
  `use_x()` + `try_x(&query)`; wired in `queries/mod.rs` (module + pub use)
  and re-exported in `hooks/mod.rs`.
- Mutations/actions: `queries/mutations.rs` (`ClusterAction` enum drives
  `use_cluster_mutation`), `actions.rs` + `actions/` (`Actions` dispatcher:
  launch, notify, panels…).
- `queries/state.rs` — `settled_or_loading`, `query_error`,
  `query_is_loading`, `query_is_busy`.

## App views (`app/view/app/`)

- `home/` — `index.rs` (panel + RecentsRow + `WhatsNew`),
  `whats_new.rs` (unread changelog card).
- `clusters/` — `page.rs` (Versions page: filters, sorting, groups,
  context menus, modals), `instance/` (create/edit/duplicate/group modals,
  shared shell/rail/details), `delete_instance.rs`.
- `cluster/` — per-instance shell: `overview.rs` (analytics body),
  `configs.rs` (config editor), `logs.rs`, `mods.rs`, `shaders.rs`,
  `textures.rs`, `worlds.rs`, `screenshots.rs`, `datapacks.rs`,
  `package_manager/` (install/browse tabs, biggest file — careful),
  `modpack_settings.rs`, `settings` etc.
- `analytics/` — `mod.rs` (`analytics_body(analytics, cluster_id)` builds the
  whole stats page), `charts.rs` (DailyChart, PlaytimeChart,
  `distribution_card`), `heatmap.rs`, `sessions.rs` (timeline),
  `servers.rs`, `tiles.rs`.
- `servers.rs` — Servers page (add form, rows with ping + history).
- `stats.rs` — global analytics page wrapper; `home/index.rs` for Home.
- `settings/` — settings pages incl. `changelog.rs` (Markdown release cards,
  sets `seen_changelog_version`), `storage.rs` (storage report).
- `browser/`, `skins/` — browse & skins features.

## Wiring files you touch for a new page/route

- `app/routes.rs` — route DSL, `Route::…` + title() match arm.
- `app/view/app/mod.rs` — re-export view components.
- `app/components/navbar/app_navbar.rs` — NavLink entries.
- `app/components/command_palette.rs` — palette entries (icon + route).

## UI helpers worth knowing

- `app/ui/` — `border_all_color`, `fixed_grid`,
  `grid_columns_for_width`, `centered_note`, `window_logical_size`,
  `last_played_label`, `columns_for`.
- `app/utils.rs` — formatting & cluster helpers (see FREYA_NOTES.md);
  tests live in the same file (`mod tests`).
- `app/theme/colors.rs` — palette functions; `app/theme/mod.rs` —
  paddings (`HOME_PADDING_PX` etc.).

## Audit & memory

- Old machine: `/home/darkie/Desktop/AUDIT.txt` (append per wave, include a
  LESSON), `/home/darkie/ass/WORKFLOW.md` (release pipeline details).
- Copies of both are in this folder — keep one canonical copy going forward.
