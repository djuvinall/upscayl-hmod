# upstream-electron

**Responsibility:** upstream's desktop GUI. In this fork it is **reference material only** — the record of how each GUI setting becomes engine arguments, and of the features the GUI adds on top of the engine. The module ports that behaviour; it never runs this code.

## Public interface

None that the module uses. The relevant seams, for porting:

- Argument building: `electron/utils/get-arguments.ts` — single, batch, and the two passes of double upscayl.
- Scale inference: `common/check-model-scale.ts`.
- Bundled model list: `common/models-list.ts`.
- Payload shapes (every GUI setting): `ImageUpscaylPayload`, `BatchUpscaylPayload`, `DoubleUpscaylPayload` in `electron/types` / `common/types`.

## Data crossing the boundary

**In:** GUI settings. **Out:** a spawned `upscayl-bin` process (`electron/utils/spawn-upscayl.ts`, empty args filtered out).

## Behaviour the GUI adds over the CLI (what the module must re-implement)

| Feature | What upstream does |
|---|---|
| Scale omission | sends `-s` only when it differs from the model's native scale and no custom width is set. **Never passes `-z`**, which is safe only for 4x models: a custom 2x/3x model is processed as 4x and comes out corrupted (`docs/results/engine-probe.md`, finding 1). The module does **not** port this rule |
| Double upscayl | pass 1 without `-c`/`-w`/`-x`; pass 2 runs the output through again with them |
| Copy metadata | after the run, `exiftool -tagsFromFile <in> -overwrite_original_in_place <out>` (via `exiftool-vendored`) |
| Output naming | `<name>_upscayl_<scale>x_<model>.<fmt>` or `<width>px` when a custom width is used; `overwrite` setting. Double upscayl keeps the single-pass scale in the name (the module names it `scale x scale`) |
| Progress / failure | parses stderr: `%` lines for progress, `Error`/`failed` for failure |
| Custom models | a user-picked folder whose `.param`/`.bin` stems become model names |

## Invariants

- GUI scale range is 1–16 (`select-image-scale.tsx`), and the engine honours all of it. Compression 0–100 applies to WebP only.

## Notes

- Files here are upstream's. Do not edit them on `hmod/main` (CLAUDE.md, *Fork rules*).
