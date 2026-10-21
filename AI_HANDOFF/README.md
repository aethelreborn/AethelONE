# AethelONE — AI Handoff (opencode session)

> **Read this file top to bottom before doing anything.**
> Written 2026-10-18 near the end of a long autonomous session on Linux
> (`/home/darkie/ass/OneLauncher`). The user is switching OS; this folder is
> copied into the repo so a fresh opencode session on the new machine can
> continue the exact same work with zero rediscovery. The folder is safe to
> delete later (it ships no code).
>
> Companion files in this folder:
> - `WORKFLOW.md` — the release pipeline details (copy of the original)
> - `AUDIT.txt` — full audit log of every wave with LESSON entries
> - `ITERATIONS.md` — everything shipped, commit hashes, release numbers
> - `LESSONS.md` — consolidated do/don't list (read this too)
> - `FREYA_NOTES.md` — component/API facts for this codebase's UI framework
> - `PROJECT_MAP.md` — where every important piece of the codebase lives

---

## 1. The mission (user's standing orders)

The user wants a **never-stopping improvement loop** on this fork:

1. Research features from famous Minecraft launchers (Prism, MultiMC,
   ATLauncher, GDLauncher, CurseForge, Modrinth App, …) and from comparable
   non-Minecraft apps, **and** invent genuinely novel features no launcher has.
2. For each candidate: **check the codebase first** — if the feature already
   exists, skip it (a "covered gaps" list is in `ITERATIONS.md` §4).
3. Pick one iteration, make a todo list, implement it, verify locally, ship it,
   then immediately start the next one. **Never pause to ask for status or
   direction. Never end a message with "want me to continue?" — just continue.**
4. Every release cycle must end with: pushed commit → green CI → cut release →
   append an audit entry with a LESSON.

The user also explicitly said: *"If you stop I will be mad. NEVER STOP."* and
*"I asked u to never stop"*. The only acceptable reasons to stop are: hitting a
blocker only the user can resolve (credentials, hardware), or the user
redirecting the work.

---

## 2. Repos, remotes, and what to never touch

| Repo | Local path (old OS) | Remote to push | Notes |
|---|---|---|---|
| OneLauncher (AethelONE) | `/home/darkie/ass/OneLauncher` | `aethelone` → `github.com/aethelreborn/AethelONE` | main work repo |
| PolyPlus | `/home/darkie/ass/PolyPlus` | `aetheloneplus` → `github.com/aethelreborn/AethelOnePLUS` | keep healthy; at `0175d8f`; keep `deps.polydium = "1.1.2"` pinned; sync + Gradle green as of handoff |

- **`origin` in both repos is the upstream project (Polyfrost/OneLauncher).**
  **NEVER push to `origin`.** Only push `aethelone` / `aetheloneplus`.
- Never amend or force-push already-pushed commits. Fix-forward only.
- CI lives in the repos: `AethelONE Nightly (Unsigned)`, `Upstream Sync`,
  `AethelONE Release Build`, `Publish Changelog`.

## 3. New-OS setup checklist

```text
1. git clone https://github.com/aethelreborn/AethelONE   (or copy the old checkout)
2. gh auth status                     # GitHub CLI must be logged in with push rights
   gh repo view aethelreborn/AethelONE
3. rustup show                        # stable toolchain; edition 2024 needs Rust >= 1.85
   cargo --version
4. git remote -v                      # verify: origin = upstream (never push), aethelone = fork
5. Test the build:  cargo check -p oneclient_app --lib -j 1
   (First build is slow. If the machine has more RAM, drop `-j 1`.)
6. Where audit lives: the old machine kept it at /home/darkie/Desktop/AUDIT.txt.
   A full copy is in this folder — keep appending to whichever copy you keep.
   Same for WORKFLOW.md (old: /home/darkie/ass/WORKFLOW.md; copy here).
7. Read LESSONS.md + FREYA_NOTES.md + PROJECT_MAP.md in this folder.
8. Current state: see §8 "Exact resume point" below.
9. Restart the loop: pick iteration 13 from the backlog and go.
```

> **Windows box (2026-10-21):** this machine has no MSVC toolchain (no
> `link.exe`, no `cl.exe`, no cmake) and no rustup, so **cargo cannot compile
> anything here** — not even a proc macro. `rustfmt` is installed and works
> (`rustfmt --edition 2024 <file>`); use it on every touched file. The CI is
> the compile gate: `cargo test --workspace` plus the four platform builds,
> ~20 minutes per push. The crates.io download took 35 minutes here (825
> crates, 48 MB), so budget an hour before the first build starts.

OS notes: everything in the pipeline is cross-platform except the `.deb`
verification (`dpkg-deb --info`) — on non-Linux verify a release with
`gh release view oneclient-X.Y.Z -R aethelreborn/AethelONE --json isDraft,assets`.
Local release/tauri builds are CI-only regardless.

---

## 4. The per-iteration loop (what "how you work" means here)

```text
  ┌─ research candidate feature (web search / known launchers / novelty)
  │
  ├─ grep the codebase; if it exists → note in covered list; pick next
  │
  ├─ todowrite the plan (3–6 concrete steps)
  │
  ├─ implement bottom-up: db → core → hooks → view → wiring
  │     (database migration first if schema changes)
  │
  ├─ cargo check -p oneclient_app --lib -j 1          (must be error-free)
  ├─ cargo test -p <touched crates> -j 1               (must be green)
  ├─ rustfmt --edition 2024 <every touched .rs file>
  │     (re-check afterwards: reformatting can hide patches — verify!)
  │
  ├─ mkdir -p .changeset && write .changeset/<slug>.md  (minor/patch)
  ├─ git add <exact files> && git commit -m "feat(area): …"
  ├─ git push aethelone main
  │
  ├─ wait for Nightly green:  gh run list -R aethelreborn/AethelONE
  ├─ dispatch release:  gh workflow run upstream_sync.yml -R aethelreborn/AethelONE --ref main -f release=true
  ├─ wait for sync green (polls in ~60s steps; 10–20 min typical)
  ├─ verify:  gh release view oneclient-X.Y.Z -R aethelreborn/AethelONE --json isDraft,assets
  │     expect: draft=false, 18 assets, incl. latest.json + changelog.json + .deb + .sig
  │     Linux: curl the .deb and dpkg-deb --info to sanity check
  ├─ git pull aethelone main            (release commit lands on main)
  ├─ append AUDIT.txt entry: what shipped + a LESSON
  └─ immediately start next iteration (never report "done" and idle)
```

Time budget: a small iteration is ~45 min wall-clock; large ones 1.5–2 h.
The user expects ~12–16 releases/day capacity during long sessions. Do not
batch multiple features into one release unless they are genuinely coupled.

### Commit / changeset style

- Commit: `feat(area): short imperative summary` (or `fix(...)`), body explains
  what and why in plain words. Match existing git log style.
- Changeset `.changeset/<slug>.md`:
  ```md
  ---
  default: minor
  ---

  User-facing paragraph(s). First line is the headline the changelog shows.
  ```
  `minor` for features, `patch` for fixes. `.changeset/` is **consumed by the
  release commit** — always `mkdir -p .changeset` before writing.

---

## 5. Hard constraints and traps (these caused real bugs)

1. **sqlx checked macros are untouchable.** There is no `cargo sqlx` CLI and
   no DATABASE_URL here; committed `.sqlx/` metadata cannot be regenerated.
   - NEVER edit existing `sqlx::query!` / `query_as!` / `query_scalar!` calls.
   - New tables/queries MUST use runtime `sqlx::query` / `query_as::<_, T>`.
     Precedents: `dao/saved_server.rs`, `dao/cluster_group.rs`,
     `dao::game_session::recent_sessions`.
   - `sqlx::migrate!()` only resolves inside `oneclient_db`; core tests must
     `CREATE TABLE` by hand (see `core/src/servers/mod.rs` tests).
2. **rustfmt**: always `--edition 2024`. Formatting a `mod.rs` recurses into
   its children (touches sibling files — fine when formatting-only, but stage
   them). After any scripted string-replace + rustfmt, **re-read the file** —
   rustfmt re-wraps code and a later blind `.replace()` silently no-ops or
   lands between an attribute and its item (duplicate `#[derive]` conflict).
3. **Zsh**: `echo ===` breaks (equals expansion) — avoid bare `=` in echo;
   prefer quotes or `printf`.
4. **Small machine**: 3 GB RAM / 2 cores on the old box. Always `-j 1` and
   generous timeouts for cargo (check ~2 min, app test compile ~10–15 min
   after view changes). Release builds are CI-only, never local.
5. **A pre-existing failing test** (not ours): `view::app::browser::tests::
   a_disabled_override_on_an_unlinked_pin_offers_enable` fails on clean HEAD;
   CI does not run app-lib tests. Verify by `git stash -u` + rerun before
   ever "fixing" it — don't chase it.
6. **Transient GitHub flakiness** during release asset uploads happens
   (`gh run rerun <id> --failed` and/or manual publish of the draft).
   The publish workflow double-firing is harmless (concurrency cancels).

---

## 6. Architecture map (short version — full detail in PROJECT_MAP.md)

```
packages/
  oneclient_app/      Slint? No — Freya (custom Rust UI). All views, hooks, actions.
    src/view/app/     pages: home, clusters, cluster/*, browser, servers, stats, analytics, settings, skins
    src/hooks/        query/mutation wrappers (freya::query), Actions dispatcher
    src/components/   Button, TextInput, Dropdown, ScrollArea, OverlayPopup, Icon(IconType), Markdown, …
  oneclient_core/     business logic; wraps db + net + content; LauncherError hub
  oneclient_db/       sqlite (sqlx), migrations/, dao/, models/
  oneclient_content/  Modrinth/CurseForge, bundles (custom modpacks), packages
  oneclient_cluster/  per-instance file ops: configs, logs, worlds, screenshots, export
  oneclient_net/      HTTP, downloads, and slp.rs (Server List Ping protocol)
  oneclient_common/   shared types (GameLoader, Patch, mc version parsing…)
```

Data flow for a new feature: migration (db) → DAO (runtime queries) → core
wrapper with tests → app hook (`hooks/queries/*.rs` + mod.rs wiring) →
component (`view/app/**`) → route/nav/palette wiring if it is a page.

---

## 7. Where everything shipped (summary)

Iterations 1–18 are complete: **2.18.0 → 2.32.0**. 2.30.0 shipped
Performance Mode (the launcher now tunes the JVM, the heap and the game's own
video settings instead of only promising to, commit `e7ee5d4d`), 2.31.0
shipped the performance set and the per-instance performance mode, and 2.32.0
carries the upstream merge and the named saved servers. Full table with
commits, versions, test counts and lessons: `ITERATIONS.md`. Audit trail with
per-wave lessons: `AUDIT.txt`.

## 8. Exact resume point

- Local `main`: the upstream merge (`863679fc`) plus the fixes that made it
  compile (`cf0d0864`, `e1118c54`, `9b8808e7`, `e418b4ef`, `d6a82702`). Run
  `git pull origin main` to be current.
- PolyPlus: untouched, healthy, `0175d8f`, polydium pinned `1.1.2`.
- **The upstream sync should work again**: the merge resolved the conflicts
  that stopped it for days. The next scheduled run (every 6 hours) merges
  upstream cleanly.
- **Next iteration = 19.** Candidate backlog (in rough priority order):
  1. Finish the 2.32.0 release (the draft needs a rebuild with the merge
     fixes, then publish it and refresh changelog.json).
  2. Pending bundle/package updates preview ("what will change") before launch sync.
  3. Storage page: visual size treemap / top consumers.
  4. Mods page: grouping/tabs polish (Updates-first view).
  5. Session timeline follow-ups if users want zoom/range control.
  6. Editable saved-server labels are done - the server names shipped.
  Research fresh candidates too — the loop allows any scope.
- Also polish candidates: server rows drag-to-reorder; groups on Home;
  relative-time refresh on servers page; heatmap tooltip improvements.

## 9. Communication style the user expects

- Extremely terse status lines between actions ("2.27.0 shipped. Auditing…").
- No preambles, no "Let me know if…" endings. Ship, then silently continue.
- In-app notifications/toasts for user-visible errors; brand wording, no emoji.
- Never commit secrets; never touch `origin`; never `git config` changes.

## 10. If you are unsure about anything

Read `LESSONS.md`, then `AUDIT.txt` tail (last entries describe the most recent
state), then `git log --oneline -15`. Everything is deliberate; the history is
the documentation.
