# jobs (planned)

**Responsibility:** run upscale work that can outlive one tool call — large batches, TTA, double upscayl — and report on it.

Intent as of 2026-10-08 (`decisions.md`). Three layers, of which **only the first is this repo's**:

| Layer | Owner | What |
|---|---|---|
| Job tools | this module | `start_job` returns a `job_id` immediately; `job_status`, `wait_job` (blocks up to a bound below the tool timeout), `cancel_job`. Jobs persist under `HDECK_MODULE_DATA_DIR/jobs/` and hold the module alive via `GET /lifecycle` while any is running |
| Timeout setting | **Orchestrator** | a global default tool timeout (today `HDECK_TOOL_TIMEOUT`, 300 s) as a setting, copied into each new graph, overridable per graph |
| Per-job triggers | **Orchestrator** | several independent trigger points in one graph, so one graph can hold separate job pipelines like a project space |

The Orchestrator layers need issues filed on `djuvinall/Orchestrator`; nothing here can build them.

## Public interface

See the job tools in `upscayl-module.md`. The panel reads the same jobs over a chunked progress stream.

## Invariants

- A job survives a module restart as a record; an engine process does not. On restart, a job whose process is gone is marked `interrupted`, never silently re-run.
- `wait_job` always answers before the caller's timeout, with the job's current state.

## Known unknowns

- Whether `wait_job` can learn the caller's timeout or needs it as an input.
