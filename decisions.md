# Decisions

Newest first. Entries separated by `---`.

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
