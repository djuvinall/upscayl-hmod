# Lessons

Things learned the hard way. One entry per lesson. Add the why — a lesson without
its reasoning gets overruled by the next plausible-sounding idea.

## 2026-10-08 — A Linux shell over this Windows checkout lies about what changed

**What happened:** from the Linux side, nine upstream files showed as modified; Windows git showed a clean tree.
**Why:** Windows git has `core.autocrlf=true` and checks files out CRLF; a git without that setting compares CRLF on disk to LF in the index and reports every line changed.
**Do instead:** run git from Windows. `.gitattributes` (`* text=auto eol=lf`) now makes new checkouts LF on every platform.

## 2026-10-08 — `core.hooksPath` protects nothing on a branch that lacks `.githooks/`

**What happened:** after wiring `core.hooksPath .githooks`, a test commit on `hmod/main` was accepted — before the PR that adds `.githooks/` had merged into it.
**Why:** hooks are read from the checked-out tree. A branch without the directory (`main`, or `hmod/main` before this PR) runs no hooks at all, silently.
**Do instead:** never commit on `main` at all (it is synced with `gh repo sync`). After this PR merges, `hmod/main` carries the hooks and the check holds.

## 2026-10-08 — A running module locks its own executable on Windows

**What happened:** `cargo build` failed with `Access is denied (os error 5)` while the hosted test instance had `upscayl` running, so the core kept serving the old binary and the new tool answered 404.
**Why:** Windows will not replace an executable a process has open; `{bin:upscayl}` resolves to `target/debug/upscayl.exe`, which is exactly what is running.
**Do instead:** `POST /api/modules/upscayl/disable`, then `cargo build`, then `POST /api/modules/upscayl/enable`. `cargo test` and `cargo clippy` do not rebuild that executable at all, so run `cargo build` before any hosted check.

## 2026-10-08 — A long-lived branch named `hmod` would block every `hmod/...` branch

**What happened:** the first branch-layout idea was a `hmod` branch with `hmod/feat/*` work branches.
**Why:** git stores refs as paths; `refs/heads/hmod` (a file) and `refs/heads/hmod/feat` (a directory) cannot coexist.
**Do instead:** the long-lived branch is `hmod/main`; work branches are `<type>/<issue>-<slug>`.

## 2026-10-08 — `gh` in this checkout talks to upstream unless told otherwise

**What happened:** `gh issue list` with no `-R` listed upscayl/upscayl's issues, not the fork's.
**Why:** with an `upstream` remote and no default set, `gh` picks the parent repo of a fork. An `issue create` or `pr create` would have landed on upstream, in public.
**Do instead:** `gh repo set-default djuvinall/upscayl-hmod` once per clone (now in CLAUDE.md's after-cloning step), and still pass `-R djuvinall/upscayl-hmod` in scripts.
