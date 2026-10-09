# Architecture

Upstream's Electron app is a GUI over one CLI binary, `upscayl-bin` (the upscayl-ncnn engine). This fork adds a HollowDeck module that wraps the **same binary** directly, re-implements the few features the Electron layer adds on top of the CLI, and publishes the NCNN models as module-owned assets. Nothing in the module imports or runs the Electron app.

## Modules

| Module | Responsibility | Status | Detail |
|---|---|---|---|
| upstream-engine | `upscayl-bin` CLI and the NCNN model files it loads | upstream, real | [docs/arch/upstream-engine.md](docs/arch/upstream-engine.md) |
| upstream-electron | Upstream's GUI; the reference for how settings become CLI arguments | upstream, reference only | [docs/arch/upstream-electron.md](docs/arch/upstream-electron.md) |
| upscayl-module | The HollowDeck module (`modules/upscayl/`, id `upscayl`): tools, panel, process supervision | planned | [docs/arch/upscayl-module.md](docs/arch/upscayl-module.md) |
| model-assets | Bundled and imported NCNN models as `ncnn_model` assets, each with its own license | planned | [docs/arch/model-assets.md](docs/arch/model-assets.md) |
| jobs | Long-running upscale work that outlives a single tool call | planned | [docs/arch/jobs.md](docs/arch/jobs.md) |

## Cross-cutting

- **Contract:** HollowDeck `INTEROP.md`, `api_version 1`. Loopback HTTP+JSON, per-spawn secret guard, relative URLs, state only under `HDECK_MODULE_DATA_DIR`, module files resolved against `HDECK_MODULE_DIR`.
- **Logging:** every engine run is one event-log line (`POST {HDECK_CORE_URL}/m/event_log/api/events`) carrying the model id and that model's license, so licensing conformance can be audited from the log later.
- **Paths:** tool path inputs follow INTEROP.md §12's four-step rule (trim/unquote, resolve against `base_dir`, refuse drive-relative, say which happened in any refusal).

## Boundaries that matter

- **Module ↔ engine** is argv in, files + stderr out. The engine's flags are the contract (see upstream-engine). The module never depends on Electron code at runtime.
- **Module ↔ Orchestrator** is the tool declaration in `module.json`. Tool ids and socket names are stored in saved graphs; renaming either is a breaking change.
- **Fork ↔ upstream** is a merge boundary. `hmod/main` adds files and does not edit upstream's (CLAUDE.md, *Fork rules*). The module reads engine files from upstream's `resources/` only through its sync script, so an upstream engine update arrives by syncing, never by hand-copying.

## Known unknowns

- Whether the three non-commercial (CC BY-NC-SA) models ship inside the `.hmod` or as an optional download (`docs/results/model-licenses.md`).
- Per-graph timeout override and per-job triggers are **Orchestrator features**, not this repo's (see `docs/arch/jobs.md`). They need issues on `djuvinall/Orchestrator`.
- CI: the SDK lives in a private repo; how a public fork's CI builds against it is undecided (`decisions.md`).
