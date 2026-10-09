# Acceptance v1: every capability through real graphs

Issue #15. Run 2026-10-08 on Windows 11, RTX 5070 Ti (GPU 0), HollowDeck `0aabff1e`,
this repo at `64ecee6` plus the examples, engine staged from upstream `e1aeaf12`.

**Result: 9 of 9 pass.** Every graph ran through `hollowdeck run` against the
module as the Orchestrator would call it, not through the module's API directly.

- Graphs: `modules/upscayl/examples/` (one per row; see its README).
- How: `modules/upscayl/examples/run-acceptance.ps1` with a scratch `HDECK_DATA_DIR`, after
  `make-long-batch.ps1` (80 copies of `samples/sample-512.jpg`).
- Raw data: `acceptance-v1/results.json` (exit code, run status, summary, refusals and the
  module's event-log lines per graph) and `acceptance-v1/raw/<graph>.txt` (each run's
  console output). The repository root is written `<repo>`.

Commands are `hollowdeck --project modules/upscayl/examples/upscayl-examples.hollow run modules/upscayl/examples/<graph>`, with `--attended` for 01–07 and without it for 08–09.

| Graph | Expected | Actual | Pass | Event-log line(s) |
|---|---|---|---|---|
| `01-single-every-socket` | Three outputs: scale 3 WebP with every flag; width 900 with catmullrom (keeps aspect); exact 640x360 with mitchell | 768x768 WebP, 900x900 JPEG, 640x360 PNG; licenses CC-BY-4.0, BSD-3-Clause, BSD-3-Clause; 7.7 s | ✅ | 3× `upscayl.upscaled`, e.g. `-n high-fidelity-4x -z 4 -s 3 -f webp -c 30 -g 0 -j 1:2:2 -x`; `-w 900:catmullrom`; `-r 640x360:mitchell` |
| `02-custom-model-param` | `custom_model` path wins; license `unrecorded` | 512x512, `model` = the `.param` path, license `unrecorded`; 3.8 s | ✅ | `upscayl.upscaled`, license `unrecorded` |
| `03-custom-model-asset` | Asset node's token wins; license travels | `upscayl:bundled/remacri-4x`, license `CC-BY-NC-SA-4.0`; 5.7 s | ✅ | `upscayl.upscaled`, `-n remacri-4x`, license `CC-BY-NC-SA-4.0` |
| `04-double-upscayl` | 2x twice from 256 px = 1024 px | 1024x1024, named `_4x_`; 4.8 s | ✅ | `upscayl.upscaled` (pass 1 into the data dir's `tmp/`, then pass 2) |
| `05-copy-metadata` | Output carries the input's Artist, Copyright, ImageDescription | All three present on the output (read with the bundled exiftool); 6.0 s | ✅ | `upscayl.upscaled` with `copy_metadata` |
| `06-batch-corrupt` | 2 written, 1 failed naming `corrupt.jpg`, node does not fail | `outputs` a.png, b.png; `failed` = `corrupt.jpg: Couldn't read the image …`; `complete` false; exit 0; 3.6 s | ✅ | `upscayl.batch` |
| `07-long-batch-job` | A batch longer than one tool call finishes through `start_job` + `wait_job` | Job ran 393.6 s (past the 300 s tool timeout): first `wait_job` returned at 280 s with `running`, progress 70; second at 113.6 s with `done`, 80 written, 0 failed; run 396.7 s | ✅ | `upscayl.job.queued`; `upscayl.batch` "80 written, 0 failed", ultrasharp-4x, `CC-BY-NC-SA-4.0` |
| `08-unattended-box-off` | Unattended run refused before the call | Exit 1, `failed`, refused: "upscayl/upscale_image node 'upscale_image_1' writes to this machine, and its 'Allow in unattended runs' box is not ticked …"; 1.4 s | ✅ | none (the module was never called) |
| `09-unattended-box-on` | Same node, box ticked, runs unattended | Exit 0, 512x512; 5.4 s | ✅ | `upscayl.upscaled` |

Every run that reached the module also logged `upscayl.started` (the module coming up for
the run).

## Observations

- `07` shows folder progress counting files (#28): 70 % at 280 s, which is 56 of 80 files.
- `06`'s failure sentence comes from the engine and mixes `\` and `/` in the path
  (`…\samples\batch/corrupt.jpg`): upstream's own bug (upscayl#903), left as is.
- The run summary truncates long values (`…`); the console output in `raw/` has the full
  sink values.
- Not covered here, already covered elsewhere: `import_model` and `list_models` (unit and
  hosted tests in #12, and the panel in #14), cancel (#32).
