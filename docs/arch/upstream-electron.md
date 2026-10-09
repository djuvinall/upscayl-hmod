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
| Scale omission | sends `-s` only when it differs from the model's native scale and no custom width is set |
| Double upscayl | pass 1 without `-c`/`-w`/`-x`; pass 2 runs the output through again with them |
| Copy metadata | after the run, `exiftool -tagsFromFile <in> -overwrite_original_in_place <out>` (via `exiftool-vendored`) |
| Output naming | `<name>_upscayl_<scale>x_<model>.<fmt>` or `<width>px` when a custom width is used; `overwrite` setting |
| Progress / failure | parses stderr: `%` lines for progress, `Error`/`failed` for failure |
| Custom models | a user-picked folder whose `.param`/`.bin` stems become model names |

## Invariants

- GUI scale range is 1–16 (`select-image-scale.tsx`), compression 0–100. Whether the engine honours scales outside 2/3/4 is unverified.

## Notes

- Files here are upstream's. Do not edit them on `hmod/main` (CLAUDE.md, *Fork rules*).
