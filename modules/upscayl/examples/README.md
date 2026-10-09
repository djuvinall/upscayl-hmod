# Example graphs

One Orchestrator graph per v1 capability. They are the acceptance suite for #15
(`docs/results/acceptance-v1.md`) and a starting point for your own graphs.

| Graph | Shows |
|---|---|
| `01-single-every-socket.json` | `upscale_image` three ways: scale with every flag set (GPU, tile, threads, TTA, WebP compression), width with a filter, exact size with a filter |
| `02-custom-model-param.json` | `custom_model` as a `.param` path |
| `03-custom-model-asset.json` | `custom_model` from an Asset node (the bundled Remacri asset) |
| `04-double-upscayl.json` | double upscayl: 2x twice is 4x |
| `05-copy-metadata.json` | EXIF carried from input to output |
| `06-batch-corrupt.json` | `upscale_batch` over a folder holding a corrupt file |
| `07-long-batch-job.json` | a batch longer than one tool call: `start_job`, then `wait_job` twice |
| `08-unattended-box-off.json` | an unattended run refuses a tool node whose *Allow in unattended runs* box is off |
| `09-unattended-box-on.json` | the same node with the box ticked runs |

Relative paths in the graphs (`samples/...`, `out/...`, `../engine/...`) resolve against
the folder of the open project, so run them with `upscayl-examples.hollow`:

```powershell
# Once: stage the engine, and build the long batch for 07.
..\scripts\sync-engine.ps1
.\make-long-batch.ps1

# One graph (attended):
hollowdeck --project .\upscayl-examples.hollow run .\01-single-every-socket.json --attended

# All of them, recorded into docs/results/acceptance-v1/:
$env:HDECK_DATA_DIR = '<a scratch data dir>'
.\run-acceptance.ps1
```

Outputs land in `out/` (gitignored). The inputs in `samples/` are drawn by
`samples/make-samples.py` and are CC0.
