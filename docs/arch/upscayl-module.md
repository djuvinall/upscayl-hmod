# upscayl-module (planned)

**Responsibility:** expose every engine capability to HollowDeck as tool nodes, supervise engine processes, and own the model assets. Rust, `kind: process`, id `upscayl`, at `modules/upscayl/`.

Everything below is **intent**, decided 2026-10-08 (`decisions.md`). Replace each section with the real contract as it is built.

## Public interface

Tools in `module.json` (node types `upscayl/<id>`). Ids and socket names are permanent once a graph stores them.

| Tool | Does | `effects` |
|---|---|---|
| `upscale_image` | one image, synchronous, honours the tool timeout | `writes` (open question: `executes`, since it runs a bundled binary) |
| `upscale_batch` | a directory, synchronous | `writes` |
| `start_job` / `job_status` / `wait_job` / `cancel_job` | asynchronous form of the above (see `jobs.md`) | `writes` / `reads` / `reads` / `writes` |
| `list_models` | bundled + imported models with their licenses | `reads` |
| `import_model` | copy a `.param`/`.bin` pair into the module's data dir and create its asset | `writes` |

Inputs shared by the upscale tools (one socket per GUI setting and CLI flag): `input` (path), `output` (path, optional), `model` (`select` of the bundled models), `custom_model` (`str`, overrides `model` when non-empty: a `.param` path or a model token from an asset), `model_scale` (`-z`), `scale` (`-s`, 1–16), `width` (`-w`), `resize` (`-r`, `WxH`), `resize_filter`, `format`, `compression` (WebP only), `gpu_id`, `tile_size`, `threads` (`-j`), `tta`, `double_upscayl`, `copy_metadata`, `overwrite`.

Argument rules, from `docs/results/engine-probe.md`: `-z` and `-s` are always sent; `width` and `resize` each replace `-s` and are mutually exclusive (a call setting both is refused); the output extension is derived from `format`; model files are checked before spawning; success is read from stderr, not the exit code.

HTTP routes beyond the contract's: `api/assets` (the asset surface, see `model-assets.md`) and the panel's job API with a chunked progress stream.

## Data crossing the boundary

**In:** tool calls `{inputs, base_dir, base_dir_source}` via the core's broker.
**Out:** `{outputs: {...}}`; files on disk; event-log lines.

## Depends on

- `hollowdeck-module` SDK (path dependency on the sibling HollowDeck checkout).
- Engine files staged into `modules/upscayl/engine/` (gitignored) by the sync script from upstream's `resources/` and `models/`, plus a bundled `exiftool`.

## Invariants

- Bind `127.0.0.1:HDECK_MODULE_PORT` only; guard in two modes; answer `/health` before slow setup.
- Never hold state in memory that must survive; the core may stop the module at any idle moment. Running jobs hold it alive through `GET /lifecycle`.
- Never write inside the module directory at run time (breaks `modules verify`).

## Notes

- `.hmod` size is dominated by models (~150 MB for the bundled set). Whether every model ships bundled is decided per model by its license.
