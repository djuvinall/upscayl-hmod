# Engine probe — upscayl-bin behaviour the module relies on

Issue #3. Run 2026-10-08 on Windows 11, NVIDIA GeForce RTX 5070 Ti (driver 617.14, device 0) plus an AMD Radeon iGPU (device 1), using the Upscayl 2.15.0 tree's `resources\win\bin\upscayl-bin.exe`.

**Reproduce:** `python docs/results/engine-probe/probe.py` (all groups) or `--only <group>`. Python 3.11 + Pillow. Outputs go to `%TEMP%\upscayl-probe`; nothing is written into the repo except the results below.

**Raw data:** `engine-probe/results.json` (groups scale…progress), `engine-probe/results-followup.json` (follow-up cases), `engine-probe/raw/<case>.txt` (full command, exit code, stdout, stderr per run). Paths in them are scrubbed to `<repo>` and `<work>`.

**Input:** `to_upscale.jpeg`, 256 × 256 JPEG. Model `upscayl-lite-4x` unless stated.

## Findings — rules the module must follow

1. **Always pass `-z <native scale>`.** Without it the engine assumes 4. A 2x model run without `-z 2` produces a visibly corrupted image (tiles misplaced and mis-coloured; mean absolute difference 63/255 per channel against the `-z 2` output) even when the output size is right. Upstream's Electron app never passes `-z`, so its custom 2x/3x models are affected — an upstream bug, not ours to fix here.
2. **Always pass `-s` explicitly.** `-s` defaults to the `-z` value, not to "no resize". Any integer from 1 to 16 works (`-s 1` returns the input size, `-s 16` gives 4096 × 4096): the engine runs the model at its native scale, then resizes. Upstream's "omit `-s` when it equals the model scale" rule is only safe because it never passes `-z`; the module does not port it.
3. **`-w` and `-s` must never be sent together.** With both, the engine honours neither (`-w 1000 -s 2` gave 1024 × 1024, the default 4x). Width takes precedence in the module: send `-w` alone.
4. **`-w` and `-r` must never be sent together** (`-w 1000 -r 800x600` gave 800 × 800, neither request). Expose them as one choice: scale, width, or exact size.
5. **`-w` keeps aspect; `-r WxH` does not** (`-r 1920x1080` on a square input gave 1920 × 1080, stretched). Both accept a `:filter` suffix: `default`, `box`, `triangle`, `cubicbspline`, `catmullrom`, `mitchell`, `pointsample` (from `-r help`, which prints to **stdout** and exits −1).
6. **Compression (`-c`) affects WebP only.** PNG and JPEG output were byte-identical at `-c 0`, `50` and `100`. WebP: 968 KB → 50 KB → 13 KB, so higher means smaller and lossier. The socket must say so.
7. **The output file's extension decides the format, not `-f`.** `-o x.png -f jpg` wrote a PNG. With no `-f`, the extension is used. The module derives the extension from `format` and refuses an explicit output path whose extension disagrees.
8. **The model folder must be named `models`.** `-m <dir>` whose last component is not `models` fails at once with `Unknown model dir type. Make sure that the model directory is called 'models'…`. Imported models must be stored as `<data dir>/imported/<id>/models/<name>.{param,bin}`. A relative `-m` resolves against the process's working directory; the module always passes an absolute path.
9. **A missing model crashes the engine.** A model name with no `.param`, or a `.param` with no `.bin`, prints `Error: Failed to open …` and then exits `0xC0000409` (3221226505) after 4–7 s. The module checks both files exist before spawning.
10. **Exit code 0 does not mean success.** A missing or unreadable input prints `Error: Couldn't read the image '<path>'!` and exits **0** with no output. Success is: stderr contains `Upscayled Successfully!`, contains no `Error:` line, and the output file exists. Exit −1 (4294967295) is argument errors (`-g 99`: `Error: Invalid GPU Device`; bad model dir).
11. **Progress format:** stderr lines `NN.NN%` (e.g. `0.00%`, `25.00%`), then a bare `100.00`, a blank line, and `🙌 Upscayled Successfully!`. With `-v`, a `✅ <in> -> <out> done` line follows. Before any progress, eight `[<n> <GPU name>] …` capability lines per run list every Vulkan device. Regex for progress: `^(\d{1,3}\.\d{2})%?$`.
12. **Directory mode works and does not fail on a bad file.** `-i <dir> -o <dir>` processed two good JPEGs (output named `<stem>.<fmt>`) and printed `Error: Couldn't read the image '<dir>/corrupt.jpeg'!` for the corrupt one, exiting 0. Progress lines from parallel files interleave, so per-file progress cannot be read from directory mode; batch progress is per-file-completed.
13. **The engine creates missing output folders** and **overwrites an existing output file without asking.** `overwrite: false` is the module's job.
14. **Paths with spaces and non-ASCII characters work** (`ünïcødé\图像.jpeg`).
15. **GPU:** `-g 0` (RTX), `-g 1` (iGPU) and `-g 0,1 -t 0,0` (both) all work; `-g 99` exits −1. Which device "auto" picks is not printed, even with `-v`. Threads `-j 1:2:2` / `-j 2:4:4` and tiles `-t 0|32|256` all work; `-t 32` was slowest. TTA (`-x`) works.

## Results by case group

All times are wall-clock seconds for a 256 px input, so they measure start-up more than throughput (see the perf-baseline issue for that).

### Output scale (`-s`, 4x model)

| `-s` | exit | output |
|---|---|---|
| 1 | 0 | 256 × 256 |
| 2 | 0 | 512 × 512 |
| 3 | 0 | 768 × 768 |
| 4 | 0 | 1024 × 1024 |
| 5 | 0 | 1280 × 1280 |
| 6 | 0 | 1536 × 1536 |
| 8 | 0 | 2048 × 2048 |
| 12 | 0 | 3072 × 3072 |
| 16 | 0 | 4096 × 4096 |
| (none) | 0 | 1024 × 1024 |

### Native scale (`realesr-animevideov3-x2`)

| args | output | quality |
|---|---|---|
| (none) | 1024 × 1024 | corrupted |
| `-z 2` | 512 × 512 | correct |
| `-z 2 -s 4` | 1024 × 1024 | correct |
| `-s 2` (no `-z`) | 512 × 512 | corrupted |
| `-z 4 -s 2` | 512 × 512 | corrupted |

### Width and resize

| args | output |
|---|---|
| `-w 1000` | 1000 × 1000 |
| `-w 1000:pointsample` | 1000 × 1000 |
| `-r 1920x1080` | 1920 × 1080 |
| `-r 800x600:catmullrom` | 800 × 600 |
| `-w 1000 -s 2` | 1024 × 1024 (neither honoured) |
| `-w 1000 -r 800x600` | 800 × 800 (neither honoured) |
| `-r help` | usage on stdout, exit −1 |

### Format and compression

| format | `-c 0` | `-c 50` | `-c 100` |
|---|---|---|---|
| png | 1,357,035 B | identical | identical |
| jpg | 634,479 B | identical | identical |
| webp | 968,172 B | 50,028 B | 13,160 B |

`-o byext.webp` with no `-f` → WebP. `-o mismatch.png -f jpg` → PNG.

### Failure modes

| case | exit | stderr | output |
|---|---|---|---|
| model name does not exist | 3221226505 (crash) after 6.7 s | `Error: Failed to open …/does-not-exist.param` | none |
| `.param` without `.bin` (dir named `models`) | 3221226505 after 4.3 s | `Error: Failed to open …/upscayl-lite-4x.bin` | none |
| model dir not named `models` | −1 | `Error: Unknown model dir type…` | none |
| input does not exist | **0** | `Error: Couldn't read the image …` | none |
| output folder does not exist | 0 | success | created |
| output file already exists | 0 | success | overwritten |
| `-g 99` | −1 | `Error: Invalid GPU Device` | none |

The read-only output-folder case is inconclusive: Windows ignores the read-only attribute on folders, so the write succeeded.
