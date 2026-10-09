# model-assets (built, #12)

**Responsibility:** make every NCNN model — bundled or imported — a HollowDeck asset owned by the `upscayl` module, so a graph can pick one with an Asset node and wire it into `custom_model`, and so every model's license travels with it.

Built in #12. The envelope is HollowDeck's (`docs/arch/assets.md` in both HollowDeck and Orchestrator); this file only says how this module fills it.

## Public interface

The standard asset surface on relative URLs: `GET api/assets` → `{owner, assets: [summary], broken}`, `POST api/assets` → `201 {asset}`, `GET api/assets/{id}` → `{asset}`, `DELETE api/assets/{id}` → `{ok, id}`. The SDK matches exact paths only, so `src/server.rs` runs the module's own accept loop: the SDK's guard (`Module::refuse`) on every request, the two `{id}` routes, then `Module::answer` for everything else. `vendor/assets.py` is HollowDeck's file carried verbatim (byte-compare test), and a test loads every record this module serves through its `Asset.from_dict` and checks it round-trips unchanged. `POST api/assets` normalises a body the way `Asset.from_dict` does.

Bundled models are derived from `licenses/models.json` on each read (ids `bundled-<name>`, never stored, delete answers 409) and carry `staged` in `properties`. Imported models are stored as `<data dir>/assets/<id>.json` with their files in `<data dir>/imported/<id>/models/`; deleting the asset deletes the files. Tools: `import_model` (`writes`) and `list_models` (`reads`).

## Data crossing the boundary

One asset per model:

| Envelope field | Value |
|---|---|
| `kind` | `ncnn_model` (`model` is reserved in practice for LLM values) |
| `origin` | `ingested` for bundled models, `authored` for imports |
| `interface` | one output, `model: str` |
| `payload.outputs.model` | a **model token** — `upscayl:bundled/<name>` or `upscayl:imported/<id>` — never an absolute path |
| `properties` | the license record, below |

### License record (required on every model asset)

Flat scalars, because `properties` takes JSON scalars only:

| Key | Example | Rule |
|---|---|---|
| `license` | `CC-BY-NC-SA-4.0` | SPDX id, or `LicenseRef-<name>`; `unknown` blocks bundling |
| `license_url` | link to the license text | required unless `license` is `unknown` |
| `license_file` | `licenses/remacri-4x.txt` | path relative to the module (bundled) or data dir (imported) |
| `source_url` | where the model came from | required |
| `author` | model author as credited | required when known |
| `commercial_use` | `allowed` / `forbidden` / `unknown` | derived from the license, shown in the UI |
| `attribution` | `4xHFA2k by Helaman (Phhofm), CC BY 4.0, …` | the credit line to show; required for CC BY models |

The bundled set's records live in one file the module ships (`modules/upscayl/licenses/models.json`, researched in `docs/results/model-licenses.md`), which also carries each model's `source_dir`, `native_scale`, `original_name`, `evidence` URLs and `confidence`. The sync script refuses to stage a model with no entry. Every engine run logs the model token and its `license` to the event log.

## Invariants

- A model with `license: unknown` may be imported for local use but is never bundled into a `.hmod`.
- A token resolves only through the module; a graph never holds a filesystem path to a model.

## Known unknowns

- The Asset Library view lists the models (checked 2026-10-08) but does **not** render `properties`, so a model's license is not visible there; the module's panel (#14) must show it.
