# jobs (module half built, #13)

**Responsibility:** run upscale work that can outlive one tool call — large batches, TTA, double upscayl — and report on it.

Three layers, of which **only the first is this repo's**. The first is built (#13); the other two are djuvinall/Orchestrator#93 and #94:

| Layer | Owner | What |
|---|---|---|
| Job tools | this module | `start_job` returns a `job_id` immediately; `job_status`, `wait_job` (blocks up to a bound below the tool timeout), `cancel_job`. Jobs persist under `HDECK_MODULE_DATA_DIR/jobs/` and hold the module alive via `GET /lifecycle` while any is running |
| Timeout setting | **Orchestrator** | a global default tool timeout (today `HDECK_TOOL_TIMEOUT`, 300 s) as a setting, copied into each new graph, overridable per graph |
| Per-job triggers | **Orchestrator** | several independent trigger points in one graph, so one graph can hold separate job pipelines like a project space |

The Orchestrator layers are djuvinall/Orchestrator#93 (timeout setting, per-graph override) and #94 (several independently triggered pipelines per graph); nothing here can build them.

## Public interface

Tools (`src/jobs.rs`): `start_job` takes `kind` (`image` or `batch`) plus every `upscale_image` socket and `input_folder`/`output_folder`, checks the request as the synchronous tool would (a bad request is a 400 now, never a failed job later), queues it and answers at once. `job_status`, `wait_job` (`timeout_s`, default 240, capped at 280) and `cancel_job` take `job_id`. All four answer the same outputs: `job_id`, `state`, `finished`, `progress`, `output`, `outputs`, `failed`, `error`, `elapsed_ms`. `GET api/jobs` lists every job, newest first, for the panel, which polls it (#14).

One worker thread runs jobs one at a time; records live in `<data dir>/jobs/<id>.json` and are rewritten on every state change and every 2% of progress. `GET /lifecycle` holds while a job is queued or running. Cancelling stops the engine within one poll (50 ms); a batch keeps what it already wrote, lists the rest as `not processed, the job was cancelled`, and keeps the progress it reached (#32). A folder's progress counts finished files, not the engine's per-file percentages, which interleave in directory mode (#28).

## Invariants

- A job survives a module restart as a record; an engine process does not. On restart, a job whose process is gone is marked `interrupted`, never silently re-run.
- `wait_job` always answers before the caller's timeout, with the job's current state.

## Known unknowns

- `wait_job` cannot learn the caller's timeout, so it takes `timeout_s` and is capped at 280 s, below the 300 s default. A graph whose timeout is raised per graph (Orchestrator#93) can pass a larger value once the cap is lifted.
