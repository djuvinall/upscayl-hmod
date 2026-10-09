# upstream-engine

**Responsibility:** turn one image (or a directory of images) into an upscaled image using an NCNN model on a Vulkan GPU. Upstream's, built from [upscayl/upscayl-ncnn](https://github.com/upscayl/upscayl-ncnn) (AGPL-3.0, a fork of Real-ESRGAN-ncnn-vulkan).

## Public interface

The CLI. Verified 2026-10-08 by running `resources\win\bin\upscayl-bin.exe -h` (Upscayl 2.15.0 tree):

| Flag | Meaning |
|---|---|
| `-i <path>` | input image (jpg/png/webp) or directory |
| `-o <path>` | output image or directory |
| `-z <2\|3\|4>` | the model's native scale (default 4). **Must match the model** or the output is corrupted |
| `-s <n>` | output scale; any integer 1–16 works (help text says 2–4); defaults to the `-z` value |
| `-r <WxH>[:filter]` | resize to exact dimensions, aspect not kept; `-r help` prints filters |
| `-w <width>[:filter]` | resize to a width, aspect kept. Never combine with `-s` or `-r` (neither is honoured) |
| `-c <0-100>` | compression, **WebP only**; higher = smaller (PNG/JPEG ignore it) |
| `-t <n>` | tile size, >=32 or 0 = auto; `0,0,0` per GPU |
| `-m <dir>` | folder holding the models; its last component **must be named `models`** |
| `-n <name>` | model name: `<name>.param` + `<name>.bin` in the `-m` folder |
| `-g <id>` | GPU id, default auto; `0,1,2` for multi-GPU |
| `-j <l:p:s>` | load:proc:save thread counts (default 1:2:2) |
| `-x` | TTA mode |
| `-f <fmt>` | output format; **the `-o` extension wins** when they disagree |
| `-v` | verbose |

`-h` exits with status 1. Every flag above verified 2026-10-08 (`docs/results/engine-probe.md`).

## Data crossing the boundary

**In:** argv only. A model is a `<name>.param` + `<name>.bin` pair in one folder.
**Out:** the output file(s); everything else on **stderr**: eight Vulkan capability lines per device, progress lines `NN.NN%` ending in a bare `100.00`, then `Upscayled Successfully!`, or an `Error: …` line.

**Success is not the exit code.** An unreadable input exits 0 with an `Error:` line; a missing model file crashes with `0xC0000409`; argument errors exit −1. Treat a run as successful only when stderr has `Upscayled Successfully!`, no `Error:` line, and the output exists.

## Depends on

- A Vulkan driver. Linux additionally needs `libvulkan.so.1`.
- Windows: `vcomp140.dll` beside the exe (shipped in `resources/win/bin/`).

## Invariants

- Binaries live at `resources/{win,linux,mac}/bin/upscayl-bin[.exe]`; models at `resources/models/` (7 Upscayl models) and `models/` (3 `realesr-animevideov3` scales). The module's sync script depends on these paths; an upstream move breaks the sync loudly, never silently.
- The engine does not read a model's scale from the model. Pass `-z` on every run, from the name (`x2`/`2x` → 2, `x3`/`3x` → 3, else 4) or from the caller's `model_scale`.
- Check that `<name>.param` and `<name>.bin` both exist before spawning; a missing one crashes the engine after seconds of start-up.
- The engine overwrites existing outputs and creates missing output folders.

## Notes

- `.bin` model files are 1–32 MB each and tracked in upstream's history; never re-commit them elsewhere in this repo.
