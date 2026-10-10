# upscayl-module (built: tools #9–#13, panel #14)

**Responsibility:** expose every engine capability to HollowDeck as tool nodes, supervise engine processes, and own the model assets. Rust, `kind: process`, id `upscayl`, at `modules/upscayl/`.

Decided 2026-10-08 (`decisions.md`); each "Built (#n)" paragraph below is the shipped contract.

## Public interface

Tools in `module.json` (node types `upscayl/<id>`). Ids and socket names are permanent once a graph stores them.

| Tool | Does | `effects` |
|---|---|---|
| `upscale_image` | one image, synchronous; **built (#9)** | `writes` (decided: the effect a person sees is a written file; the binary it runs is fixed, not caller-chosen) |
| `upscale_batch` | a directory, synchronous; **built (#11)** | `writes` |
| `start_job` / `job_status` / `wait_job` / `cancel_job` | asynchronous form of the above (see `jobs.md`); **built (#13)** | `writes` / `reads` / `reads` / `writes` |
| `list_models` | bundled + imported models with their licenses; **built (#12)** | `reads` |
| `import_model` | copy a `.param`/`.bin` pair into the module's data dir and create its asset; **built (#12)** | `writes` |

Inputs shared by the upscale tools (one socket per GUI setting and CLI flag): `input` (path), `output` (path, optional), `model` (`select` of the bundled models), `custom_model` (`str`, overrides `model` when non-empty: a `.param` path or a model token from an asset), `model_scale` (`-z`), `scale` (`-s`, 1–16), `width` (`-w`), `resize` (`-r`, `WxH`), `resize_filter`, `format`, `compression` (WebP only), `gpu_id`, `tile_size`, `threads` (`-j`), `tta`, `double_upscayl`, `copy_metadata`, `overwrite`.

**Defaults (#30):** every tool fills an input the caller left out (absent, null or blank) from the default `module.json` declares for it (`Call::parse_for`). Graphs send every socket; a direct `/api/tools` call need not, and the manifest stays the only place a default is written. A test checks the manifest's defaults and the code's fallbacks produce the same settings.

Argument rules, from `docs/results/engine-probe.md`: `-z` and `-s` are always sent; `width` and `resize` each replace `-s` and are mutually exclusive (a call setting both is refused); the output extension is derived from `format`; model files are checked before spawning; success is read from stderr, not the exit code.

Built (#11): `POST tools/upscale_batch` with `input_folder`, `output_folder` and every `upscale_image` setting. Writes into `upscayl_<format>_<model>_<size>` inside the output folder (upstream's naming), files keeping their stems. Uses the engine's directory mode when it can (one model load, parallel files), per-file runs for double upscayl or when existing outputs must be skipped. Outputs `output_folder`, `outputs` (list), `failed` (list of "name: reason"), `complete`, `elapsed_ms`, `model`, `license`. Inputs that share a stem (`a.jpg`, `a.png`) are refused up front. A budget run-out lists the unprocessed files under `failed` rather than failing the node.

Built (#10): `double_upscayl` and `copy_metadata` sockets. Double runs pass 1 at the requested scale into a lossless PNG under the data dir's `tmp/` (removed afterwards) and pass 2 with compression, width/resize and TTA; a double run is named with the scale it really produces (`scale x scale`), where upstream names it with the single-pass scale. `copy_metadata` runs the staged exiftool (`-overwrite_original_in_place -tagsFromFile <in> <out>`). Outputs larger than the format allows (WebP 16383 px, JPEG 65535 px a side) are refused before running.

Built (#9): `POST tools/upscale_image`. Outputs `output`, `width`, `height`, `elapsed_ms`, `model` (token or `.param` path), `license`. Refusals: 400 for a bad input (naming the socket), 409 for an existing output with `overwrite` off, 503 when the engine is not staged, 500 for an engine failure (its `Error:` sentence), 504 when the run passes the call budget (280 s, `UPSCAYL_TOOL_BUDGET_SECS`). One `upscayl.upscaled` / `upscayl.failed` event-log line per run with model, license and the full argv.

Built (#5): `GET api/status` → `{version, platform, engine_staged, engine_path, licenses_present, data_dir}`, which the panel renders; `GET /module.json` read from the file; `--selfcheck` (the manifest's `verify`) checks `module.json`, `static/index.html` and `licenses/models.json` exist.

Built routes beyond the contract's: `GET/POST api/assets`, `GET/DELETE api/assets/{id}` (see `model-assets.md`) and `GET api/jobs` (see `jobs.md`).

Built (#14): three views, one per interface, so people compose their own workspaces (Devon, 2026-10-09). `upscayl:main` (*Upscayl*, `static/index.html`): every tool setting, a drop target, and the results with progress, cancel and *Preview*. `upscayl:preview` (*Upscayl Preview*, `preview.html`): the selected result with a before/after slider (pointer drag and arrow/Page/Home/End keys), Fit or 100% zoom, the original as pixels, and a file picker with `[`/`]` for a batch. `upscayl:models` (*Upscayl Models*, `models.html`): models with license, commercial use and staged state, and the import form. The extra pages are served at the module root so their relative URLs resolve like the main view's. All work goes through `start_job`; job rows update in place; a poll error clears on the next good poll.

View routes (`src/views.rs`): `GET api/image?job&n&side=before|after` serves only paths a finished job recorded (a batch's input found by stem); `POST api/upload?drop&name&offset` takes a dropped file in slices of at most 16 MB (the SDK's body limit) into `<data>/inbox/<drop>/`, pruned after 7 days at start; `GET/POST api/select` holds the shown result, and `GET api/live` is a chunked `text/event-stream` announcing each change, answered by the accept loop after the guard. A drop in a HollowDeck view is a browser `File`, never a path (the shell turns Tauri's drop handler off), hence the upload; results of dropped files go to `<data>/outputs/` unless *Save to* names a folder. An open Preview holds `api/live`, which keeps the module from being idle-stopped while someone is looking.

**SDK limit, handled:** `hollowdeck-module` routes match exact paths only, so `src/server.rs` runs this module's own accept loop for `GET/DELETE api/assets/{id}`: `http::read_request`, `Module::refuse` (the SDK's guard, unchanged), the prefix routes, then `Module::answer`. A streaming route added later goes through this loop too.

## Data crossing the boundary

**In:** tool calls `{inputs, base_dir, base_dir_source}` via the core's broker.
**Out:** `{outputs: {...}}`; files on disk; event-log lines.

## Depends on

- `hollowdeck-module` SDK (path dependency on the sibling HollowDeck checkout).
- Engine files staged into `modules/upscayl/engine/` (gitignored) by `modules/upscayl/scripts/sync-engine.ps1` (#8): `engine/bin/<win|linux|mac>/`, `engine/models/` (only models whose license is recorded; `-NoNonCommercial` also drops CC BY-NC-SA ones), `engine/exiftool/exiftool.exe` (pinned 13.59, sha256-checked), and `engine/manifest.json` (every file's sha256 and source, the upstream commit, what was skipped). Run it before a hosted test or a pack.

## Invariants

- Bind `127.0.0.1:HDECK_MODULE_PORT` only; guard in two modes; answer `/health` before slow setup.
- Never hold state in memory that must survive; the core may stop the module at any idle moment. Running jobs hold it alive through `GET /lifecycle`.
- Never write inside the module directory at run time (breaks `modules verify`).

## Notes

- Examples and acceptance (#15): `modules/upscayl/examples/` holds one graph per capability, a project file so their relative paths resolve there, and `run-acceptance.ps1`, which records every run into `docs/results/acceptance-v1/`.

- Performance (#7, `docs/results/perf-baseline.md`): keep `tile_size` 0 as the default; large models take about 4.5 s per input megapixel at 4x (about 16.5 s with TTA). A double run with a large model and TTA over about 1 MP belongs in a job.

- `.hmod` size is dominated by models (~150 MB for the bundled set). Whether every model ships bundled is decided per model by its license.
