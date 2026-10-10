# Decisions

Newest first. Entries separated by `---`.

## 2026-10-09 — One view per interface; views share state through the module's server

Devon's call on #14: the image preview and the run panel are separate views (and so are the models), so people build their own workspaces, as HollowDeck intends. Three panels: `upscayl:main`, `upscayl:preview`, `upscayl:models`.

INTEROP gives panels no channel to each other (`hdeck:open-panel` carries no payload, and there is no panel-to-panel message), so the shown result lives on the module's server: `POST api/select`, announced on a chunked `text/event-stream` at `api/live`, the pattern the Orchestrator's views use.

A file dropped on a HollowDeck view arrives as a browser `File`, never a path, because the shell disables Tauri's drop handler so in-page drag and drop works. Dropped files are uploaded into the module's data dir and their results go to `<data>/outputs/` unless *Save to* names a folder.

**Rejected — `BroadcastChannel` between the views.** Same-origin frames could use it, but INTEROP does not mention it, a second window or a restarted shell would not see past state, and the server already knows the jobs.

**Rejected — asking HollowDeck for paths on drop.** It would mean re-enabling Tauri's drop handler, which breaks in-page drag and drop for every module (HollowDeck `shell/src-tauri/tests/windows_config.rs`).

**Consequence:** an open Preview keeps the module from being idle-stopped; dropped files are copies, pruned after 7 days.

---

## 2026-10-09 — No CI for now

Devon's call on #6. The local gate (`cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and a hosted check) stays the verification, recorded in each PR's *How it was verified*. #6 is closed; reopen it when CI earns a credential or the SDK becomes public.

**Rejected for now — a PAT secret, and vendoring the SDK.** Same reasons as the entry below.

**Consequence:** upstream's workflows stay dormant on the fork. If Actions is ever enabled, disable `stale.yml`, `main.yml` and `build-windows.yml` in the same sitting (CLAUDE.md, *Fork rules*).

---

## 2026-10-08 — The module runs its own accept loop, around the SDK's guard

`GET/DELETE api/assets/{id}` is part of the asset surface every owner serves, and the `hollowdeck-module` SDK matches exact paths only.

`src/server.rs` replaces `Module::serve`: it binds `127.0.0.1` on the handed port, reads each request with the SDK's `http::read_request`, refuses through the SDK's `Module::refuse` (both guard modes, unchanged), answers the two `{id}` routes itself, and hands everything else to `Module::answer`.

**Rejected — add prefix routing to the SDK.** The right long-term fix, but it is a change to HollowDeck's crate made from another repo; this keeps the module unblocked and is easy to delete if the SDK grows it.

**Rejected — `api/assets/get?id=`.** The Library and `asset/reference` call `/m/<owner>/api/assets/<id>`; a different spelling would not be an asset owner.

**Consequence:** a future streaming route (job progress) is added in `server.rs`, since `Module::stream` routes are only reachable through `Module::serve`.

---

## 2026-10-08 — upscale_image: effects writes, a 280 s budget, custom models stay in place

Built in #9. Three calls the issue left open:

**`effects: writes`.** What a person sees the node do is write a file. It does run a binary, but a fixed one the module ships, with arguments the module builds; `executes` would tell a graph author it runs arbitrary commands, which it cannot.

**A 280 s budget per call**, below the Orchestrator's 300 s default, so a long run fails with a sentence naming `start_job` rather than the caller timing out with none. `UPSCAYL_TOOL_BUDGET_SECS` overrides it.

**A custom `.param` must already sit in a folder named `models`.** The engine refuses any other folder name (engine probe, finding 8). Copying the pair into a temporary `models` folder on each call was the alternative.

**Rejected — copy custom models into a temporary `models` folder per call.** Invisible to the user, but copies up to ~32 MB per run and hides a rule the user will meet anyway when importing; imported models (#12) are stored in a `models` folder by design.

**Consequence:** the refusal names the rule and the fix. Upstream's scale omission is not ported: `-z` and `-s` are always sent.

---

## 2026-10-08 — CI is deferred until the SDK question is answered

The module will build against `hollowdeck-module`, which lives in the private `djuvinall/HollowDeck` repo and is not on crates.io. This repo is public.

No `ci.yml` is installed yet. It lands with the issue that scaffolds the crate, once one of the options below is chosen.

**Rejected for now — a PAT secret that checks out the private HollowDeck repo in CI.** It works and publishes nothing, but it adds a long-lived credential to a public repo's Actions; worth it only if CI earns its keep.

**Rejected for now — vendoring the SDK into this repo.** It is INTEROP.md's own "snapshot, not link" pattern, but it would publish private, unreleased code.

**Consequence:** until then, the verification gate is local (`cargo build`, `cargo test`, `cargo clippy`) and the PR template's *How it was verified* section carries it.

---

## 2026-10-08 — Fork merge settings: rebase for work, merge commits for upstream syncs

The skill default is rebase-only. A rebase merge of an upstream-sync PR replays upstream's commits under new hashes, so the next sync conflicts with commits that are "the same" but no longer identical.

Feature PRs into `hmod/main` use **Rebase and merge**. Upstream-sync PRs (`chore/sync-upstream-<date>`) use **Create a merge commit**. Squash is disabled. Head branches are deleted on merge.

**Rejected — rebase-only.** Correct for a repo with one lineage; wrong for a fork that has to keep merging another.

**Rejected — pushing syncs straight to `hmod/main` with `ALLOW_PUSH_DEFAULT=1`.** No review, and it normalises bypassing the hook.

**Consequence:** reviewers must pick the right merge button; CLAUDE.md says which.

---

## 2026-10-08 — `main` mirrors upstream; `hmod/main` is the default branch

Devon's choice. Docs and module work live only on `hmod/main`. `main` is synced server-side (`gh repo sync -b main`) and never committed to. `hmod/main` is the GitHub default, so the verbatim hooks (which guard `origin/HEAD`) protect it, and visitors land on the module.

**Rejected — module on `main`.** Simpler, but every upstream sync becomes a merge into Devon's work, and there is no clean upstream reference.

**Rejected — separate repo, fork as reference only.** Cleanest history, but loses the in-repo engine files and the upstream merge path the sync script relies on.

**Consequence:** `hmod/main` adds files and edits upstream's only for the README banner. Tags are `hmod-v*`. Upstream's stale, release and Windows-build workflows stay off: they are dormant today (GitHub registers no workflows on a fork until Actions is enabled for it, so `gh workflow disable` answered 404), and they get disabled explicitly the moment Actions is enabled for our own CI.

---

## 2026-10-08 — Models carry their own licenses; module code is AGPL-3.0

Devon's call: license depends on the asset. Module code defaults to the fork's AGPL-3.0, since its argument logic is ported from upstream. Each model asset carries a license record (SPDX id, URL, file, source, author, commercial use), and every engine run logs the model and its license, so conformance can be checked later from the event log.

**Rejected — Apache-2.0 for the module to match HollowDeck.** Only clean if nothing were ported from upstream's TypeScript; the argument logic is.

**Consequence:** a model with no recorded license is never bundled into a `.hmod`. A research issue establishes the bundled models' licenses before the first pack.

---

## 2026-10-08 — Long jobs: sync tools, job tools, and Orchestrator-side timeout and triggers

A tool call blocks for at most 300 s (`HDECK_TOOL_TIMEOUT`); big batches and TTA exceed that.

Devon's design: synchronous tools stay for small work; job tools (`start_job`, `job_status`, `wait_job`, `cancel_job`) handle long work; the global timeout becomes an Orchestrator **setting** that each new graph copies and can override; and one graph can hold several independently **triggered** pipelines, so a graph can act as a project space.

**Rejected — synchronous only with a raised timeout.** The core's brokered hop caps at 900 s regardless.

**Consequence:** the timeout setting and per-job triggers are Orchestrator work (`djuvinall/Orchestrator`), tracked there, not here.

---

## 2026-10-08 — Rust module, wrapping `upscayl-bin`, with every feature in v1

Upscayl's Electron app is a GUI over the `upscayl-bin` CLI, so the module wraps the binary and ports the few features the GUI adds (scale omission, double upscayl, metadata copy, output naming, progress parsing). Rust is HollowDeck's default build path and Devon's choice. v1 carries everything — custom model socket, double upscayl, copy metadata, batch, CLI-only flags, models as assets, async jobs; v2 is performance, stability, and custom additions.

**Rejected — Python.** Its vendored `assets.py` gives the asset routes for free, but adds FastAPI and a `.venv` to a fork that otherwise needs neither. The Rust module re-implements four asset routes and tests them against `assets.py`.

**Rejected — CLI Apps mapping of `upscayl-bin`.** Zero code, but no assets, double pass, metadata, model picker, or jobs.

**Consequence:** exiftool is bundled into the module's `engine/` by the sync script; all three platforms' binaries are wired and only Windows is tested.
