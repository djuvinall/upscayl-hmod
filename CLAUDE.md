# upscayl-hmod

Devon's fork of [upscayl/upscayl](https://github.com/upscayl/upscayl), carrying a HollowDeck module that drives Upscayl's upscaling engine (`upscayl-bin`) as Orchestrator tool nodes. The Electron app in this tree is upstream's and is not the product here.

**North star:** Every Upscayl capability (all GUI settings, CLI-only flags, double upscayl, metadata copy, batch, async jobs) runs as HollowDeck Orchestrator tool nodes from a Rust module, with bundled and custom NCNN models published as module-owned assets that each carry their own license.
**Visibility:** public (a fork of a public AGPL-3.0 repo). Everything committed on any branch is public.

## Working agreement

- `ARCHITECTURE.md` is the module index; open the matching `docs/arch/<module>.md` before changing a module's boundary, and update both in the same change.
- The module contract is HollowDeck's `INTEROP.md` (`..\HollowDeck\INTEROP.md`, private repo). When this repo's docs and `INTEROP.md` disagree, `INTEROP.md` wins and this repo's doc is the bug.
- Decisions go in `decisions.md` (newest first). Record non-obvious lessons in `tasks/lessons.md` with the why. `tasks/scratchpad.md` is local-only and disposable.

## Fork rules

- **`main` is a pristine mirror of `upstream/main`.** Never commit to it. Sync it server-side: `gh repo sync djuvinall/upscayl-hmod -b main`.
- **`hmod/main` is the default branch** and holds all of Devon's work. It changes only by PR.
- **Add files; don't edit upstream's.** Every edit to an upstream-owned file is a future merge conflict. The one sanctioned edit is the fork banner at the top of `README.md`. Module ignores go in nested `.gitignore` files, never the root one.
- **Upstream sync into `hmod/main`** is a PR from `chore/sync-upstream-<YYYY-MM-DD>` (branched from `hmod/main`, then `git merge upstream/main`), merged with **Create a merge commit** — never rebase-merged, because rebasing replays upstream's commits under new hashes and breaks every later sync.
- **Tags are `hmod-v<semver>`, never `v*`.** Upstream's release workflow fires on `v*` (it is disabled on this fork, but the rule stands if it is ever re-enabled).
- Upstream's `.github/workflows/` (stale, release builds) are dormant: GitHub registers no workflows on a fork until Actions is enabled for it. The moment it is (for our own `ci.yml`), run `gh workflow disable` on `stale.yml`, `main.yml` and `build-windows.yml` — `stale.yml` closes issues after 30 days and `main.yml` publishes releases.

## Environment

- Windows is the primary and only tested platform. Run git from Windows (global `core.autocrlf=true`); a Linux shell over the same folder reports CRLF noise.
- The Rust module needs the HollowDeck checkout as a sibling (`..\HollowDeck`) for the `hollowdeck-module` SDK, which is not on crates.io. Rust 1.89 is the floor (INTEROP.md).
- Engine binaries and models are **not re-committed**: they live in upstream's `resources/` and `models/` and are copied into the module by its sync script (planned, see `docs/arch/upscayl-module.md`).

## Repository etiquette

- `hmod/main` changes only by merging a PR with **Rebase and merge** (upstream-sync PRs excepted, above). Work on `<type>/<issue>-<slug>` — `gh issue develop <n> --name <branch> --base hmod/main --checkout` links the branch to its issue.
- After cloning: `git config core.hooksPath .githooks` and `git remote add upstream https://github.com/upscayl/upscayl.git` then `git remote set-url --push upstream DISABLE`. The hooks refuse commits and pushes on the default branch and enforce the commit format. Never `--no-verify`.
- Commits: Conventional Commits subject, 72 chars max; a body saying what changed and why; `Refs #n`. No pasted file lists. Autosquash `fixup!` commits before a PR is marked ready.
- PRs target `hmod/main`, follow `.github/pull_request_template.md`, and say `Closes #n`.
- CI is not installed yet (see `decisions.md`, CI deferred). Once `ci.yml` exists it runs only on PRs labelled `ci`; label when the diff touches: `modules/upscayl/**`, `.github/workflows/ci.yml`.
- Commit as the GitHub noreply address, never a work or personal one.

## Work queue

Work lives in GitHub issues. Startable now:

```bash
gh issue list --search "is:open -is:blocked label:status:ready"
```

Add `label:runner:agent` for what an agent can finish alone. New work is filed with
`project:issues-write`; `tasks/todo.md` holds only session-level notes too small for an issue.

## Constraints

- Module code is AGPL-3.0 (the fork's license; the argument logic is ported from upstream). Each model asset carries **its own** license; never ship or publish a model whose license is unrecorded.
- A tool call blocks for at most `HDECK_TOOL_TIMEOUT` (300 s default). Anything longer is a job (see `docs/arch/jobs.md`).
- Tool socket names are a public API: renaming one breaks every saved graph that wired it (INTEROP.md §12).
- Model sockets are typed `str`, never `model` — `model` means an LLM value in the Orchestrator.

## Out of scope

- Changing upstream's Electron app or contributing upstream from this fork.
- Video upscaling, and non-NCNN runtimes (PyTorch/ONNX) — v2 at the earliest.
- macOS and Linux testing. Their binaries are wired, not verified.
