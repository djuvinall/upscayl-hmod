# upstream-engine

**Responsibility:** turn one image (or a directory of images) into an upscaled image using an NCNN model on a Vulkan GPU. Upstream's, built from [upscayl/upscayl-ncnn](https://github.com/upscayl/upscayl-ncnn) (AGPL-3.0, a fork of Real-ESRGAN-ncnn-vulkan).

## Public interface

The CLI. Verified 2026-10-08 by running `resources\win\bin\upscayl-bin.exe -h` (Upscayl 2.15.0 tree):

| Flag | Meaning |
|---|---|
| `-i <path>` | input image (jpg/png/webp) or directory |
| `-o <path>` | output image or directory |
| `-z <2\|3\|4>` | the model's native scale (default 4) |
| `-s <n>` | custom output scale (help text: 2, 3, 4; default 4) |
| `-r <WxH>` | resize output to dimensions (`-r help` for detail) |
| `-w <width>` | resize output to a width |
| `-c <0-100>` | output compression (default 0) |
| `-t <n>` | tile size, >=32 or 0 = auto; `0,0,0` per GPU |
| `-m <dir>` | folder holding the models |
| `-n <name>` | model name: `<name>.param` + `<name>.bin` in the `-m` folder |
| `-g <id>` | GPU id, default auto; `0,1,2` for multi-GPU |
| `-j <l:p:s>` | load:proc:save thread counts (default 1:2:2) |
| `-x` | TTA mode |
| `-f <fmt>` | output format jpg/png/webp (default: input's, else png) |
| `-v` | verbose |

`-h` exits with status 1.

## Data crossing the boundary

**In:** argv only. A model is a `<name>.param` + `<name>.bin` pair in one folder.
**Out:** the output file(s); progress and errors on **stderr** (upstream's Electron layer reads `NN.NN%` progress lines and treats any line containing `Error` or `failed` as a failure).

## Depends on

- A Vulkan driver. Linux additionally needs `libvulkan.so.1`.
- Windows: `vcomp140.dll` beside the exe (shipped in `resources/win/bin/`).

## Invariants

- Binaries live at `resources/{win,linux,mac}/bin/upscayl-bin[.exe]`; models at `resources/models/` (7 Upscayl models) and `models/` (3 `realesr-animevideov3` scales). The module's sync script depends on these paths; an upstream move breaks the sync loudly, never silently.
- Model scale is inferred by name only (`x2`/`2x` → 2, `x3`/`3x` → 3, else 4). A model whose name doesn't say must be given `-z`.

## Notes

- `.bin` model files are 1–32 MB each and tracked in upstream's history; never re-commit them elsewhere in this repo.
