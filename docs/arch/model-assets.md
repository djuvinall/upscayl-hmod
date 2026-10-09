# model-assets (planned)

**Responsibility:** make every NCNN model — bundled or imported — a HollowDeck asset owned by the `upscayl` module, so a graph can pick one with an Asset node and wire it into `custom_model`, and so every model's license travels with it.

Intent as of 2026-10-08 (`decisions.md`). The envelope is HollowDeck's (`docs/arch/assets.md` in both HollowDeck and Orchestrator); this file only says how this module fills it.

## Public interface

The standard asset surface on relative URLs: `GET/POST api/assets`, `GET/DELETE api/assets/{id}`. The Rust SDK has no asset helper, so the module implements these four routes; a test validates the module's records against HollowDeck's vendored `assets.py` so the two shapes cannot drift.

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

- Whether the Asset Library view shows `properties` well enough to surface a license, or the panel must.
