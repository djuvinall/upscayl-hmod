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

## 2026-10-08 — A long-lived branch named `hmod` would block every `hmod/...` branch

**What happened:** the first branch-layout idea was a `hmod` branch with `hmod/feat/*` work branches.
**Why:** git stores refs as paths; `refs/heads/hmod` (a file) and `refs/heads/hmod/feat` (a directory) cannot coexist.
**Do instead:** the long-lived branch is `hmod/main`; work branches are `<type>/<issue>-<slug>`.
