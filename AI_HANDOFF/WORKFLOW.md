# Autonomous Work Loop — Operating Manual

How I work. Generic process, no repo-specific details. Paste into any agent's
system prompt / AGENTS.md / skill to replicate this workflow.

---

## 1. MINDSET

- Work autonomously and continuously. Never stop to ask "what next?"
- After finishing one unit of work ("wave"), immediately begin the next.
- The user interrupts if they want to stop or change direction. Until then: keep going.
- Pick the next task from a backlog (§8) when no explicit request is pending.

---

## 2. ENVIRONMENT & HARD CONSTRAINTS

- **Local builds are IMPOSSIBLE** on the working machine (low RAM, ~3.3 GB; cargo/gradle
  compile steps OOM and crash). **CI is the ONLY verifier.**
- **NEVER run:** `cargo build`, `cargo test`, `cargo check`, `./gradlew`, `gradle build`,
  `npm run build`, `make`, or any full compile/test command locally.
- **Allowed local tools ONLY:**
  - `rustfmt --edition 2024 <files>` — formatting only, no compilation.
  - `git` — status, diff, log, add, commit, push, rebase, fetch.
  - `gh` — GitHub CLI: `run list/watch/view`, `workflow run`, `release`, `api`.
  - Read-only inspection: `grep`, `sed`, `cat`, `ls`, `find`, `head`, `tail`.
- `rg` (ripgrep) may not be installed — use `grep`.
- **Verification = push + watch CI.** Green CI = done. Red CI = read logs, fix, push a
  NEW commit, re-watch.

---

## 3. REPOSITORY SETUP (per repo, do once at start)

For each repo you work in, establish and remember:
- **Local path** on disk.
- **Our fork remote name and URL** (e.g. `aethelone` → `aethelreborn/<fork>`). This is
  the ONLY remote you push to.
- **Upstream remote** (e.g. `origin` → `Polyfrost/<project>`). **NEVER push here.**
- **Default branch** (usually `main`).
- **CI workflow names** and what they do (nightly/test vs release).
- **CI trigger:** does a push to main auto-trigger it, or is it `workflow_dispatch` only?
- **Job list** and roughly how long it takes.
- **Known pre-existing failures** to ignore (scheduled sync jobs, dependabot alerts).

If you don't know a repo's setup yet, discover it: `git remote -v`, `git branch -vv`,
`ls .github/workflows/`, `gh run list -R <owner/repo> --limit 5`, read the workflow YAMLs.

---

## 4. THE CORE WORK LOOP (one wave)

```
1. PICK a task (from backlog §8, a CI failure, or a user request).

2. RESEARCH before editing:
   - grep/glob/read the relevant files.
   - Understand existing patterns: how is a similar feature wired? What components,
     hooks, utilities, naming conventions already exist?
   - NEVER invent a new pattern if an existing one fits. Mimic the surrounding code.

3. IMPLEMENT: edit files. Follow existing style exactly.

4. FORMAT (Rust): rustfmt --edition 2024 <changed .rs files>.
   (Kotlin/TS: CI or a local formatter if one exists that doesn't OOM.)

5. SANITY-CHECK the diff:
   - git status --short
   - git diff --stat
   - Confirm only intended files changed. If the formatter reformatted unrelated
     pre-existing blocks, revert those hunks (git checkout -- <file> or manual edit)
     to keep the diff minimal.

6. COMMIT:
   - git add <specific files>  (never `git add -A` blindly if unrelated junk is present)
   - Message: conventional commit — <type>(<scope>): <summary>
     Types: feat, fix, chore, test, refactor, docs.
     Add a blank line + a longer body paragraph if the change is non-trivial.

7. PUSH to the FORGE remote only (never upstream):
   - git push <fork-remote> main

8. MONITOR CI (§5). Wait for a result.

9. IF RED:
   - Read failed logs (§5).
   - Identify the compile error / test failure / panic.
   - Fix it.
   - Commit a NEW fix commit (e.g. "fix(net): constrain stream item type").
   - Push. Re-watch.
   - NEVER amend a pushed commit. NEVER force-push to fix CI.

10. IF GREEN: wave complete. Immediately start the next wave at step 1.
```

---

## 5. GITHUB CI — MONITORING & FAILURE HANDLING

### List recent runs
```bash
gh run list -R <owner/repo> --branch main --limit 5 \
  --json databaseId,status,conclusion,headSha,displayTitle,createdAt
```
Pick the run whose `headSha` matches your latest pushed commit.

### Watch a run (blocking)
```bash
gh run watch <run-id> -R <owner/repo> --exit-status --interval 30
```
- `--exit-status` makes the command exit non-zero if the run fails.
- The watch streams job progress. It can DROP mid-watch on a network reset
  (i/o timeout to api.github.com). **Don't panic** — just re-query (below).

### Query status directly (after a dropped watch, or to double-check)
```bash
gh api repos/<owner>/<repo>/actions/runs/<run-id> \
  --jq '{status,conclusion}'
gh api repos/<owner>/<repo>/actions/runs/<run-id>/jobs \
  --jq '.jobs[] | {name, status, conclusion}'
```
- `status: in_progress` / `queued` → still running.
- `status: completed, conclusion: success` → GREEN. Wave done.
- `status: completed, conclusion: failure` → RED. Get logs (below).

### Get failure logs
```bash
gh run view <run-id> -R <owner/repo> --log-failed
```
Then grep for the error. Language-specific signatures:
- **Rust:** `error[E0308]`, `error[E0425]`, `error[E0412]`, `panicked at`,
  `error: could not compile`.
- **Kotlin/Gradle:** `e: ` (compiler), `FAILURE: Build failed`, `What went wrong:`,
  `Unresolved reference`, `Type mismatch`.
- **Java:** `error:`, `BUILD FAILED`.
- **TypeScript:** `error TS`, `Cannot find`.
- **Generic:** `##[error]`, `fatal:`, `exit code`.

If `--log-failed` returns empty (network blip), re-fetch or query individual jobs via
the `gh api .../jobs` endpoint.

### Runner flakes vs real failures
Some jobs (especially **macOS runners**) get **cancelled with an empty step list** —
GitHub runner-capacity infra noise, not a compile error. Telltale signs:
- Job `conclusion: "cancelled"` but `steps: []` (never ran).
- Or cancelled immediately after `started_at` with no logged steps.

**Policy:**
- For feature waves: if the platform-independent jobs (test, linux, windows) are green
  and the failed jobs are flaky-cancelled macOS, the CODE is green. You can treat the
  wave as done, but note the flake.
- For releases: retry until EVERY job is green (see §7).

### Re-running failed jobs
```bash
gh run rerun <run-id> -R <owner/repo> --failed
```
If flaky jobs cancel AGAIN, trigger a **fresh run** instead (new run ID, fresh runners):
```bash
gh workflow run <workflow-file> -R <owner/repo> --ref main
```
A fresh run is NOT cancelled by a previous completed run (concurrency groups only
cancel in-flight runs).

---

## 6. MULTI-REPO / UPSTREAM-SYNC WORK

When working on a fork that tracks an upstream, and a "sync upstream" CI job fails
with rebase/merge conflicts, resolve MANUALLY and force-push:

```bash
git status                       # must be clean
git fetch <fork-remote>
git fetch <upstream-url> main:refs/remotes/upstream/main
git log --oneline upstream/main -5          # what upstream did
MB=$(git merge-base main upstream/main)
git diff $MB upstream/main --stat           # files upstream changed
git rebase upstream/main                    # stops at conflicts
```

At each conflict:
1. `grep -n "<<<<<<<\|=======\|>>>>>>>" <file>` to find markers.
2. Read both sides. Usually keep BOTH changes if they touch different features in the
   same region. If upstream SUPERSEDED our change (fixed the root cause), take
   upstream's and drop ours.
3. Edit to remove markers, keeping the combined result.
4. `git add <file> && GIT_EDITOR=true git rebase --continue`
5. Repeat until "Successfully rebased".
6. If a commit becomes EMPTY (purpose superseded), rebase DROPS it automatically. Fine.

Then:
```bash
git log --oneline upstream/main..HEAD | wc -l   # our commits survived
git push --force-with-lease <fork-remote> main  # force-push rebased history
```
Monitor CI (§5). Force-push is ONLY for upstream rebases — never for CI fixes.

---

## 7. RELEASES (manual / semi-auto)

Releases typically do NOT auto-publish on push. Common pattern:
- Pushes trigger a **nightly/test** workflow (unsigned, no version bump, no release).
- A separate **release** workflow is `workflow_dispatch` only (manual trigger).

### General release flow
1. **Accumulate release notes / changesets.** Many projects use a changeset dir
   (e.g. `.changeset/*.md`) where each user-facing change gets a file:
   ```
   ---
   default: minor
   ---
   <User-facing description of the change.>
   ```
   `minor` for features, `patch` for fixes. A versioning tool (knope, changesets,
   release-please) consumes these to compute the bump + changelog.
2. Commit + push the changesets.
3. **Trigger the release workflow:**
   ```bash
   gh workflow run <release-workflow-file> -R <owner/repo> --ref main
   ```
4. **Watch it** (§5). A typical release pipeline:
   - `prepare`: consumes changesets, bumps the version field (e.g. Cargo.toml
     `[workspace.package].version`), updates CHANGELOG, commits
     `chore: release <version>`, creates a **DRAFT** GitHub release with tag
     `oneclient-<version>` (or similar).
   - `package` (×N platforms): builds + uploads artifacts.
   - `manifest`: aggregates artifacts, attaches to the release.
5. **If flaky jobs cancel:** re-dispatch (§5). The workflow usually REUSES the
   existing draft (it checks `gh release view <tag> --json isDraft`).
6. **Publish the draft** once all jobs are green:
   ```bash
   gh release edit <tag> -R <owner/repo> --draft=false
   ```
7. Verify: `gh release list -R <owner/repo> --limit 3` shows it as "Latest".

### Why the version number can look "stuck"
If only the nightly workflow runs on push, the shipped binary keeps reporting the
last RELEASED version until a release workflow actually runs and bumps it. This is
expected, not a bug. To get a new version out, run the release workflow (steps above).

---

## 8. BACKLOG MANAGEMENT

Maintain a mental (or written) backlog of candidate tasks. When a wave completes and
no user request is pending, pick the next item. Categories:
- New features missing from the product.
- Bug fixes / reliability improvements.
- CI-red follow-ups.
- Tech debt / refactors (low priority unless blocking).
- Verification tasks (e.g. "confirm the zero-byte fix works against a real artifact").

Also maintain a **"done — do not redo"** list so you never re-implement an existing
feature. Before building something, grep to confirm it doesn't already exist.

---

## 9. DECISION RULES (quick reference)

| Situation | Action |
|---|---|
| Local compile needed? | NO. Push + CI. |
| CI red (compile error) | New fix commit, push, re-watch. |
| CI red (test failure) | New fix commit, push, re-watch. |
| CI red (macOS cancelled, empty steps) | Flake. Retry / re-dispatch. |
| CI green | Wave done. Start next. |
| Push target | Fork remote ONLY. Never upstream `origin`. |
| Amend a pushed commit? | NEVER. New commit instead. |
| Force-push? | Only for upstream rebase (§6). |
| New settings/config field? | Add to `Default`/`serde(default)` or it won't compile. |
| New icon/asset? | Confirm the enum variant / filename exists first. |
| Reuse an existing component? | YES, always, if one fits. |
| "What next?" | Pick from backlog (§8). Don't ask. |
| Pre-existing scheduled-CI failure | Ignore. Not yours to fix. |

---

## 10. WHAT NOT TO DO

- Do NOT run full builds/tests locally (OOM).
- Do NOT push to the upstream remote.
- Do NOT amend or force-push to fix a red CI.
- Do NOT treat one flaky platform job as a code failure (check for empty-steps cancel).
- Do NOT invent new patterns when existing ones fit.
- Do NOT leave a new struct field out of its Default/serde-default impl.
- Do NOT ask "what next?" — pick from the backlog and go.
- Do NOT leave unrelated formatter churn in the diff.
- Do NOT commit secrets, keys, or junk files.
