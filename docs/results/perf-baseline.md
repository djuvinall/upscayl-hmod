# Engine performance baseline — RTX 5070 Ti

Issue #7. Measured 2026-10-08 (2026-10-08T21:31:36 to 2026-10-08T21:58:30, local time), so v2's performance work has numbers to beat.

**Machine:** NVIDIA GeForce RTX 5070 Ti, 16 GB (driver 617.14, `-g 0`), AMD Ryzen 7 9800X3D, 62 GB RAM, Windows 11. Engine: `upscayl-bin` from the Upscayl 2.15.x tree, staged by `sync-engine.ps1`.

**Method:** `python docs/results/perf-baseline/perf.py` (committed beside this file). Input: `to_upscale.jpeg` resized to **1024 × 1024** with Lanczos (deterministic). Every staged model × tile size (`-t` 0 = automatic, 128, 256, 512) × TTA off/on, each at the model's native scale (`-z n -s n`, so the 4x models write 4096 × 4096), PNG, **3 runs, median reported**. Peak VRAM is `nvidia-smi` `memory.used` sampled every 100 ms during the run, minus the reading just before it. One warm-up run first. 240 runs, all successful; no configuration's three runs spread by more than 20%.

**Raw data:** `perf-baseline/perf-baseline.json` (every run: seconds, VRAM, output size).

## Results

Each cell: median seconds without / with TTA, then peak VRAM (without TTA; TTA adds under 0.1 GB).

| Model | tile 0 (auto) | tile 128 | tile 256 | tile 512 |
|---|---|---|---|---|
| upscayl-standard-4x | 4.51 / 16.29 s · 1.3 GB | 4.80 / 18.23 s · 0.8 GB | 4.49 / 16.22 s · 1.9 GB | 4.73 / 17.26 s · 6.0 GB |
| high-fidelity-4x | 4.64 / 16.49 s · 1.3 GB | 4.87 / 18.28 s · 0.8 GB | 4.64 / 16.50 s · 1.9 GB | 4.83 / 17.30 s · 6.0 GB |
| remacri-4x | 4.41 / 17.07 s · 1.3 GB | 4.77 / 19.07 s · 0.8 GB | 4.38 / 16.75 s · 1.9 GB | 4.62 / 17.75 s · 6.0 GB |
| ultramix-balanced-4x | 4.60 / 17.18 s · 1.3 GB | 4.87 / 19.18 s · 0.8 GB | 4.54 / 16.90 s · 1.9 GB | 4.71 / 17.82 s · 6.0 GB |
| ultrasharp-4x | 4.53 / 16.36 s · 1.3 GB | 4.71 / 18.20 s · 0.8 GB | 4.48 / 16.11 s · 1.9 GB | 4.62 / 17.10 s · 6.0 GB |
| digital-art-4x | 2.50 / 6.46 s · 1.2 GB | 2.55 / 7.05 s · 0.6 GB | 2.47 / 6.20 s · 1.7 GB | 2.60 / 6.41 s · 5.8 GB |
| upscayl-lite-4x | 1.82 / 2.41 s · 0.2 GB | 1.81 / 2.62 s · 0.2 GB | 1.81 / 2.22 s · 0.2 GB | 1.87 / 2.46 s · 0.4 GB |
| realesr-animevideov3-x2 | 1.07 / 1.45 s · 0.2 GB | 1.13 / 1.54 s · 0.2 GB | 1.07 / 1.36 s · 0.2 GB | 1.11 / 1.47 s · 0.4 GB |
| realesr-animevideov3-x3 | 1.41 / 1.71 s · 0.2 GB | 1.43 / 1.80 s · 0.2 GB | 1.37 / 1.62 s · 0.2 GB | 1.37 / 1.70 s · 0.4 GB |
| realesr-animevideov3-x4 | 1.81 / 2.06 s · 0.2 GB | 1.79 / 2.19 s · 0.2 GB | 1.80 / 1.95 s · 0.2 GB | 1.82 / 2.09 s · 0.4 GB |

## Findings

1. **Keep `tile_size` 0 (automatic) as the default.** Auto is within 1–3% of the fastest fixed size (256) on every model, at 1.3 GB instead of 1.9 GB. 512 is no faster and takes about 6 GB. 128 saves VRAM (0.8 GB) at a 5–10% cost, so it is the setting to suggest when VRAM is short, not a default.
2. **Three speed classes.** The five 32 MB ESRGAN models (standard, high-fidelity, remacri, ultramix, ultrasharp) all take about 4.5 s per input megapixel at 4x. digital-art takes about 2.5 s. upscayl-lite and the animevideov3 models take under 2 s.
3. **TTA costs about 3.6x on the large models** (4.5 s → 16.5 s), and only 1.2–1.4x on the light ones.
4. **When to use a job instead of `upscale_image`.** For a large model, the time per *input* megapixel at 4x is about 4.5 s without TTA and about 16.5 s with it. Against the module's 280 s call budget, that puts the sync limit near **60 MP** of input without TTA and near **16 MP** with it (about 4000 × 4000). Double upscayl's second pass runs on an image 16 times larger, so **a double run with a large model and TTA on anything over about 0.9 MP (about 1000 × 1000) should be a job.** *These limits extrapolate from a single 1 MP measurement, assuming time is linear in pixel count; that assumption is not measured here.*
5. **Start-up is not negligible for small images.** The light models' 1–2 s is mostly model load and process start (compare the 256 px engine-probe runs at about 1.2 s). Directory mode, which loads the model once, is the right default for batches of small images. `upscale_batch` already uses it.

## What this does not measure

Images larger than 1024 px (so the linear-scaling assumption in finding 4), multi-GPU (`-g 0,1`), the iGPU, `-j` thread counts, and any output format other than PNG.
