# Lessons — do/don't list (consolidated from AUDIT.txt)

Every entry below was learned the hard way during this project. Following them
saves hours.

## Build & environment

- Local builds: `-j 1` + long timeouts (old box: 3 GB RAM / 2 cores). App test
  compile after touching views can take 10–15 min — grep results need a timeout
  >= 30 min budget for the first run.
- Release/tauri builds are CI-only. Doubt it? Check `git log` — all releases
  were cut by the Upstream Sync workflow, never locally.
- Never push `origin` (upstream). Only `aethelone` / `aetheloneplus`.
- Never amend pushed commits; fix-forward.

## sqlx / database

- No `cargo sqlx` CLI, no DATABASE_URL. Committed `.sqlx/` metadata CANNOT be
  regenerated. **Never touch existing checked macros** (`sqlx::query!`,
  `query_as!`, `query_scalar!`).
- All NEW queries are runtime: `sqlx::query("…").bind(…)`, or
  `sqlx::query_as::<_, T>("…").bind(…)` with `#[derive(FromRow)]` structs.
- `sqlx::migrate!()` resolves paths relative to crate root — only usable inside
  `oneclient_db`. Core/app tests that need a table must create it with raw SQL
  in the test (pattern in `packages/oneclient_core/src/servers/mod.rs` tests).
- Migrations: timestamped file `migrations/YYYYMMDDHHMMSS_name.sql`, additive
  (ALTER TABLE ADD COLUMN, CREATE TABLE). Never edit old migrations.
- DAO test pattern: `SqlitePool::connect("sqlite::memory:")` + `sqlx::migrate!()`.

## Formatting & patching

- `rustfmt --edition 2024`. Formatting a `mod.rs` recurses into children —
  expect collateral formatting-only diffs in sibling files; stage them.
- **After scripted string replaces, VERIFY the patch landed** (grep the new
  code). rustfmt re-wraps code so a later blind `.replace()` can silently
  no-op. Two real incidents: the slp.rs JSON-slice fix never got applied, and
  an insert landed between `#[derive(...)]` and its struct causing duplicate
  derive conflicts.
- Insert-before-anchor patches can land in the wrong place (e.g. between an
  attribute and the item it decorates). Re-read the region after every
  scripted insert.
- When a scripted replace might fail, `assert old in s` first (python) so the
  failure is loud, not silent.

## Rust / Freya-specific

- `Option::then` consumes the Option — when you still need the payload, use
  `as_ref().map(...)`.
- `Actions` is **Clone, not Copy** — clone it into every event closure that
  needs it. `State<T>` IS Copy but `.set()` still needs a mutable binding.
- `Button::on_press` takes `FnMut(Event<PressEventData>)` — to use a plain
  `FnMut()`, wrap: `.on_press(move |_| on_toggle())` and mark the captured
  closure parameter `mut`.
- Check `theme::colors` before using a color; e.g. `fg_tertiary` does NOT
  exist (use `fg_secondary().with_a(u8)`).
- `spawn` inside Actions expects a future resolving to `()` — wrap fallible
  calls: `spawn(async move { let _ = thing().await; })`.
- Match exhaustively in `routes.rs` title() — adding a route requires an arm.
- Private module trap: a page module (`mod servers;`) may be private while its
  component is re-exported (`pub use servers::Servers;`) — import through the
  re-export, not the private path.

## Testing

- Before blaming a red test on your change: `git stash -u` and rerun on clean
  HEAD. `a_disabled_override_on_an_unlinked_pin_offers_enable` fails upstream
  independently (app lib tests are not run in CI).
- Wire-protocol features (SLP etc.) are best tested with a canned local
  listener — it caught a real varint-slicing bug that unit logic missed.
- Day/time grouping: use each timestamp's own offset so tests don't depend on
  machine timezone.
- CI runs crate tests with `--test-threads=1` where needed.

## Process

- `.changeset/` is consumed by release commits: always `mkdir -p .changeset`.
- Heredoc failures don't stop `&&`-chained git commands — verify with
  `git status` before assuming things staged.
- Nightly must be green BEFORE dispatching `upstream_sync.yml` with
  `-f release=true`; otherwise the release builds old code.
- The publish workflow double-firing on releases is harmless.
- Verify releases: `gh release view oneclient-X.Y.Z --json isDraft,assets`
  → draft=false, 18 assets (incl. `latest.json`, `changelog.json`, `.deb`,
  `.deb.sig`). On Linux also `dpkg-deb --info` the downloaded deb.
- Keep committing audit entries with a LESSON — they are the institutional
  memory for the next session.
