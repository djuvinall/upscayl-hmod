//! Upscale work that outlives one tool call (`docs/arch/jobs.md`).
//!
//! `start_job` checks the request, queues it and returns at once. One worker thread runs
//! jobs one at a time (one GPU), so a second job queues behind the first. Every state
//! change is written to `<data dir>/jobs/<id>.json`, so a job's record survives a module
//! restart; its engine process does not, and a job found `queued` or `running` when the
//! module starts is marked `interrupted` -- never silently re-run.

use crate::batch;
use crate::resolve::Base;
use crate::upscale::{self, Call};
use hollowdeck_module::{ModuleContext, Request, Response};
use serde_json::{json, Value as Json};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A job runs until it finishes or is cancelled; this is only a backstop against an
/// engine that never exits.
const JOB_BUDGET: Duration = Duration::from_secs(24 * 60 * 60);
/// `wait_job` must answer before the caller's own tool timeout (300 s by default).
const WAIT_DEFAULT: u64 = 240;
const WAIT_MAX: u64 = 280;

pub const TERMINAL: [&str; 4] = ["done", "failed", "cancelled", "interrupted"];

struct State {
    records: HashMap<String, Json>,
    queue: VecDeque<String>,
    cancel: HashMap<String, Arc<AtomicBool>>,
}

pub struct Jobs {
    ctx: ModuleContext,
    state: Mutex<State>,
    changed: Condvar,
}

static JOBS: OnceLock<Arc<Jobs>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(0);

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn new_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("j{ms:x}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

impl Jobs {
    /// The process-wide job table, created on first use. Recovers what is on disk.
    pub fn get(ctx: &ModuleContext) -> Arc<Jobs> {
        JOBS.get_or_init(|| {
            let jobs = Arc::new(Jobs {
                ctx: ctx.clone(),
                state: Mutex::new(State {
                    records: HashMap::new(),
                    queue: VecDeque::new(),
                    cancel: HashMap::new(),
                }),
                changed: Condvar::new(),
            });
            jobs.recover();
            let worker = Arc::clone(&jobs);
            std::thread::spawn(move || worker.work());
            jobs
        })
        .clone()
    }

    fn dir(&self) -> std::path::PathBuf {
        self.ctx.data_path("jobs")
    }

    fn persist(&self, rec: &Json) {
        let dir = self.dir();
        let _ = std::fs::create_dir_all(&dir);
        let id = rec["id"].as_str().unwrap_or("x");
        let tmp = dir.join(format!("{id}.json.tmp"));
        if std::fs::write(&tmp, serde_json::to_vec_pretty(rec).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join(format!("{id}.json")));
        }
    }

    /// Load every record; a job the last process left queued or running is interrupted.
    fn recover(&self) {
        let Ok(entries) = std::fs::read_dir(self.dir()) else {
            return;
        };
        let mut st = self.state.lock().expect("jobs lock");
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let Ok(mut rec) = serde_json::from_str::<Json>(&text) else {
                continue;
            };
            let Some(id) = rec["id"].as_str().map(str::to_string) else {
                continue;
            };
            let state = rec["state"].as_str().unwrap_or("").to_string();
            if state == "queued" || state == "running" {
                rec["state"] = json!("interrupted");
                rec["error"] = json!(format!(
                    "the module stopped while this job was {state}; start it again with start_job"
                ));
                rec["finished_at"] = json!(now());
                self.persist(&rec);
            }
            st.records.insert(id, rec);
        }
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut Json)) {
        let rec = {
            let mut st = self.state.lock().expect("jobs lock");
            let Some(rec) = st.records.get_mut(id) else {
                return;
            };
            f(rec);
            rec.clone()
        };
        self.persist(&rec);
        self.changed.notify_all();
    }

    pub fn busy(&self) -> bool {
        let st = self.state.lock().expect("jobs lock");
        st.records
            .values()
            .any(|r| matches!(r["state"].as_str(), Some("queued" | "running")))
    }

    fn submit(&self, kind: &str, call: &Call) -> Json {
        let id = new_id();
        let rec = json!({
            "id": id,
            "kind": kind,
            "state": "queued",
            "progress": 0.0,
            "created_at": now(),
            "started_at": null,
            "finished_at": null,
            "inputs": call.inputs,
            "base_dir": call.base.dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default(),
            "base_dir_source": call.base.source.clone().unwrap_or_default(),
            "result": null,
            "error": null,
        });
        {
            let mut st = self.state.lock().expect("jobs lock");
            st.records.insert(id.clone(), rec.clone());
            st.cancel
                .insert(id.clone(), Arc::new(AtomicBool::new(false)));
            st.queue.push_back(id.clone());
        }
        self.persist(&rec);
        self.changed.notify_all();
        rec
    }

    fn work(self: Arc<Self>) {
        loop {
            let (id, rec, stop) = {
                let mut st = self.state.lock().expect("jobs lock");
                while st.queue.is_empty() {
                    st = self.changed.wait(st).expect("jobs lock");
                }
                let id = st.queue.pop_front().expect("non-empty");
                let rec = st.records.get(&id).cloned().unwrap_or(Json::Null);
                let stop = st.cancel.get(&id).cloned().unwrap_or_default();
                (id, rec, stop)
            };
            if rec["state"] != "queued" {
                continue; // cancelled while queued
            }
            self.update(&id, |r| {
                r["state"] = json!("running");
                r["started_at"] = json!(now());
            });
            let call = Call {
                inputs: rec["inputs"].clone(),
                base: Base::of(
                    rec["base_dir"].as_str().unwrap_or(""),
                    rec["base_dir_source"].as_str().unwrap_or(""),
                ),
            };
            let last = Mutex::new(0.0_f32);
            let progress = |p: f32| {
                let mut l = last.lock().expect("progress lock");
                if p - *l >= 2.0 || p >= 100.0 {
                    *l = p;
                    self.update(&id, |r| r["progress"] = json!(f64::from(p)));
                }
            };
            let should_stop = || stop.load(Ordering::Relaxed);
            let outcome = if rec["kind"] == "batch" {
                batch::run(&call, &self.ctx, JOB_BUDGET, &progress, &should_stop)
            } else {
                upscale::prepare(&call, &self.ctx).and_then(|job| {
                    upscale::execute(&job, &self.ctx, JOB_BUDGET, &progress, &should_stop)
                })
            };
            let result =
                outcome.map(|v| serde_json::from_str::<Json>(&v.to_json()).unwrap_or(Json::Null));
            let cancelled = stop.load(Ordering::Relaxed);
            self.update(&id, |r| {
                r["finished_at"] = json!(now());
                match result {
                    Ok(v) => {
                        r["state"] = json!(if cancelled { "cancelled" } else { "done" });
                        r["progress"] = json!(100.0);
                        r["result"] = v;
                    }
                    Err((_, m)) => {
                        r["state"] = json!(if cancelled { "cancelled" } else { "failed" });
                        r["error"] = json!(m);
                    }
                }
            });
        }
    }

    pub fn record(&self, id: &str) -> Option<Json> {
        self.state
            .lock()
            .expect("jobs lock")
            .records
            .get(id)
            .cloned()
    }

    /// Block until the job is terminal or `wait` passes.
    pub fn wait(&self, id: &str, wait: Duration) -> Option<Json> {
        let deadline = Instant::now() + wait;
        let mut st = self.state.lock().expect("jobs lock");
        loop {
            let rec = st.records.get(id)?.clone();
            if TERMINAL.contains(&rec["state"].as_str().unwrap_or("")) {
                return Some(rec);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Some(rec);
            }
            st = self
                .changed
                .wait_timeout(st, left.min(Duration::from_millis(500)))
                .expect("jobs lock")
                .0;
        }
    }

    pub fn cancel(&self, id: &str) -> Option<Json> {
        let queued = {
            let mut st = self.state.lock().expect("jobs lock");
            let state = st.records.get(id)?["state"]
                .as_str()
                .unwrap_or("")
                .to_string();
            if let Some(flag) = st.cancel.get(id) {
                flag.store(true, Ordering::Relaxed);
            }
            if state == "queued" {
                st.queue.retain(|q| q != id);
            }
            state == "queued"
        };
        if queued {
            self.update(id, |r| {
                r["state"] = json!("cancelled");
                r["finished_at"] = json!(now());
            });
        }
        self.record(id)
    }

    pub fn list(&self) -> Vec<Json> {
        let mut all: Vec<Json> = self
            .state
            .lock()
            .expect("jobs lock")
            .records
            .values()
            .cloned()
            .collect();
        all.sort_by(|a, b| {
            b["created_at"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&a["created_at"].as_f64().unwrap_or(0.0))
        });
        all
    }
}

/// Whether any job is queued or running, for `GET /lifecycle`. Never creates the table.
pub fn busy_now() -> bool {
    JOBS.get().is_some_and(|j| j.busy())
}

// ---------------------------------------------------------------- the four tools

fn ok(outputs: Json) -> Response {
    Response::new(
        200,
        "application/json",
        serde_json::to_vec(&json!({"outputs": outputs})).unwrap_or_default(),
    )
}

/// The outputs every job tool returns: the job as a graph sees it.
pub fn view(rec: &Json) -> Json {
    let r = &rec["result"];
    let elapsed = match (rec["started_at"].as_f64(), rec["finished_at"].as_f64()) {
        (Some(s), Some(f)) => ((f - s) * 1000.0) as i64,
        (Some(s), None) => ((now() - s) * 1000.0) as i64,
        _ => 0,
    };
    let state = rec["state"].as_str().unwrap_or("").to_string();
    json!({
        "job_id": rec["id"],
        "state": state,
        "finished": TERMINAL.contains(&state.as_str()),
        "progress": rec["progress"],
        "output": r["output"].as_str().unwrap_or(""),
        "outputs": r["outputs"].as_array().cloned().unwrap_or_else(|| {
            r["output"].as_str().map(|o| vec![json!(o)]).unwrap_or_default()
        }),
        "failed": r["failed"].as_array().cloned().unwrap_or_default(),
        "error": rec["error"].as_str().unwrap_or(""),
        "elapsed_ms": elapsed,
    })
}

/// `POST tools/start_job`.
pub fn start_job(req: &Request, ctx: &ModuleContext) -> Response {
    let call = match Call::parse_for(req, ctx, "start_job") {
        Ok(c) => c,
        Err((s, e)) => return Response::error(s, &e),
    };
    let kind = match call.str("kind").to_ascii_lowercase().as_str() {
        "" | "image" => "image",
        "batch" => "batch",
        k => return Response::error(400, &format!("kind must be image or batch, got \"{k}\"")),
    };
    // Refuse a bad request now, while someone is waiting for the answer, rather than
    // queue it and fail it later.
    if kind == "image" {
        if let Err((s, m)) = upscale::prepare(&call, ctx) {
            return Response::error(s, &m);
        }
    } else {
        if let Err(m) = upscale::settings(&call) {
            return Response::error(400, &m);
        }
        if call.str("input_folder").is_empty() {
            return Response::error(400, "input_folder is empty: a batch job needs a folder");
        }
        let model_scale = call.int("model_scale", 0).unwrap_or(0).clamp(0, 4) as u32;
        if let Err(m) = crate::models::resolve_model(
            ctx,
            &call.base,
            &call.str("model"),
            &call.str("custom_model"),
            model_scale,
        ) {
            return Response::error(400, &m);
        }
    }
    let rec = Jobs::get(ctx).submit(kind, &call);
    ctx.log(
        "info",
        "upscayl.job.queued",
        &format!("queued {kind} job {}", rec["id"].as_str().unwrap_or("")),
        &[("job".to_string(), rec["id"].as_str().unwrap_or("").into())],
    );
    ok(view(&rec))
}

fn job_id(call: &Call) -> Result<String, Response> {
    let id = call.str("job_id");
    if !valid_id(&id) {
        return Err(Response::error(
            400,
            &format!("job_id \"{id}\" is not a job id"),
        ));
    }
    Ok(id)
}

fn not_found(id: &str) -> Response {
    Response::error(404, &format!("no job \"{id}\""))
}

/// `POST tools/job_status`.
pub fn job_status(req: &Request, ctx: &ModuleContext) -> Response {
    let call = match Call::parse_for(req, ctx, "job_status") {
        Ok(c) => c,
        Err((s, e)) => return Response::error(s, &e),
    };
    let id = match job_id(&call) {
        Ok(i) => i,
        Err(r) => return r,
    };
    match Jobs::get(ctx).record(&id) {
        Some(rec) => ok(view(&rec)),
        None => not_found(&id),
    }
}

/// `POST tools/wait_job`: block until the job finishes or `timeout_s` passes, then
/// answer with its state either way.
pub fn wait_job(req: &Request, ctx: &ModuleContext) -> Response {
    let call = match Call::parse_for(req, ctx, "wait_job") {
        Ok(c) => c,
        Err((s, e)) => return Response::error(s, &e),
    };
    let id = match job_id(&call) {
        Ok(i) => i,
        Err(r) => return r,
    };
    let secs = match call.int("timeout_s", WAIT_DEFAULT as i64) {
        Ok(v) if v >= 0 => (v as u64).min(WAIT_MAX),
        Ok(v) => return Response::error(400, &format!("timeout_s must be 0 or more, got {v}")),
        Err(e) => return Response::error(400, &e),
    };
    match Jobs::get(ctx).wait(&id, Duration::from_secs(secs)) {
        Some(rec) => ok(view(&rec)),
        None => not_found(&id),
    }
}

/// `POST tools/cancel_job`.
pub fn cancel_job(req: &Request, ctx: &ModuleContext) -> Response {
    let call = match Call::parse_for(req, ctx, "cancel_job") {
        Ok(c) => c,
        Err((s, e)) => return Response::error(s, &e),
    };
    let id = match job_id(&call) {
        Ok(i) => i,
        Err(r) => return r,
    };
    let jobs = Jobs::get(ctx);
    if jobs.cancel(&id).is_none() {
        return not_found(&id);
    }
    // A running job stops within a poll of the engine; give it that long so the answer
    // says cancelled rather than running.
    match jobs.wait(&id, Duration::from_secs(5)) {
        Some(rec) => ok(view(&rec)),
        None => not_found(&id),
    }
}

/// `GET api/jobs`: every job, newest first, for the panel.
pub fn list(_req: &Request, ctx: &ModuleContext) -> Response {
    let all: Vec<Json> = Jobs::get(ctx).list().iter().map(view).collect();
    Response::new(
        200,
        "application/json",
        serde_json::to_vec(&json!({"jobs": all})).unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollowdeck_module::Headers;
    use std::path::PathBuf;

    fn ctx() -> Option<ModuleContext> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if !dir.join(crate::paths::ENGINE_REL).is_file() {
            eprintln!("skipped: engine not staged");
            return None;
        }
        Some(ModuleContext {
            module_id: "upscayl".into(),
            module_dir: dir.clone(),
            mount_path: "/m/upscayl".into(),
            shared_data_dir: dir.join("target/test-data"),
            data_dir: dir.join("target/test-data/jobs-module"),
            core_version: None,
            core_bind: "127.0.0.1".into(),
            core_url: None,
            secret: None,
            port: 1,
        })
    }

    fn post(tool: &str, inputs: Json) -> Request {
        let body = json!({"inputs": inputs, "base_dir": env!("CARGO_MANIFEST_DIR"), "base_dir_source": "project"});
        Request {
            method: "POST".into(),
            path: format!("/tools/{tool}"),
            query: String::new(),
            headers: Headers::new(),
            body: body.to_string().into_bytes(),
        }
    }

    fn out(r: &Response) -> Json {
        assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
        serde_json::from_slice::<Json>(&r.body).unwrap()["outputs"].clone()
    }

    fn sample() -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../to_upscale.jpeg")
            .display()
            .to_string()
    }

    fn fresh(name: &str) -> PathBuf {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-out/jobs")
            .join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn an_image_job_runs_to_done_and_wait_collects_its_output() {
        let Some(c) = ctx() else { return };
        let dir = fresh("image");
        let started = out(&start_job(
            &post(
                "start_job",
                json!({"kind": "image", "input": sample(), "output": format!("{}/", dir.display()), "model": "upscayl-lite-4x", "scale": 2}),
            ),
            &c,
        ));
        assert!(
            matches!(started["state"].as_str(), Some("queued" | "running")),
            "{started}"
        );
        let id = started["job_id"].as_str().unwrap().to_string();
        let done = out(&wait_job(
            &post("wait_job", json!({"job_id": id, "timeout_s": 60})),
            &c,
        ));
        assert_eq!(done["state"], "done", "{done}");
        assert_eq!(done["finished"], true);
        assert!(
            done["output"]
                .as_str()
                .unwrap()
                .ends_with("to_upscale_upscayl_2x_upscayl-lite-4x.png"),
            "{done}"
        );
        assert!(dir
            .join("to_upscale_upscayl_2x_upscayl-lite-4x.png")
            .is_file());
        let st = out(&job_status(&post("job_status", json!({"job_id": id})), &c));
        assert_eq!(st["progress"], 100.0);
    }

    #[test]
    fn a_bad_request_is_refused_at_start_not_queued() {
        let Some(c) = ctx() else { return };
        let r = start_job(
            &post(
                "start_job",
                json!({"kind": "image", "input": "missing.jpg"}),
            ),
            &c,
        );
        assert_eq!(r.status, 400);
        let r = start_job(&post("start_job", json!({"kind": "batch"})), &c);
        assert_eq!(r.status, 400);
        let r = start_job(&post("start_job", json!({"kind": "video"})), &c);
        assert_eq!(r.status, 400);
        assert_eq!(
            job_status(&post("job_status", json!({"job_id": "nope-1"})), &c).status,
            404
        );
    }

    #[test]
    fn a_running_batch_can_be_cancelled_and_holds_the_module_while_it_runs() {
        let Some(c) = ctx() else { return };
        let dir = fresh("cancel");
        let input = dir.join("in");
        std::fs::create_dir_all(&input).unwrap();
        for i in 0..12 {
            std::fs::copy(sample(), input.join(format!("img{i:02}.jpeg"))).unwrap();
        }
        // Per-file (double upscayl) with TTA on a large model: slow enough to cancel.
        let started = out(&start_job(
            &post(
                "start_job",
                json!({"kind": "batch", "input_folder": input.display().to_string(), "model": "ultrasharp-4x", "tta": true, "double_upscayl": true, "scale": 2}),
            ),
            &c,
        ));
        let id = started["job_id"].as_str().unwrap().to_string();
        let t0 = Instant::now();
        loop {
            let st = out(&job_status(&post("job_status", json!({"job_id": id})), &c));
            if st["state"] == "running" {
                break;
            }
            assert!(
                t0.elapsed() < Duration::from_secs(60),
                "never started: {st}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(busy_now(), "a running job must hold the module");
        let cancelled = out(&cancel_job(&post("cancel_job", json!({"job_id": id})), &c));
        assert_eq!(cancelled["state"], "cancelled", "{cancelled}");
        let written = cancelled["outputs"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        assert!(written < 12, "cancel came too late to test: {cancelled}");
    }

    #[test]
    fn a_job_left_running_by_a_dead_process_is_interrupted_on_recovery() {
        let dir = fresh("recover");
        let c = ModuleContext {
            module_id: "upscayl".into(),
            module_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            mount_path: "/m/upscayl".into(),
            shared_data_dir: dir.clone(),
            data_dir: dir.clone(),
            core_version: None,
            core_bind: "127.0.0.1".into(),
            core_url: None,
            secret: None,
            port: 1,
        };
        std::fs::create_dir_all(dir.join("jobs")).unwrap();
        for (id, state) in [("ja-1", "running"), ("jb-2", "queued"), ("jc-3", "done")] {
            std::fs::write(
                dir.join(format!("jobs/{id}.json")),
                json!({"id": id, "state": state, "created_at": 1.0}).to_string(),
            )
            .unwrap();
        }
        let jobs = Jobs {
            ctx: c,
            state: Mutex::new(State {
                records: HashMap::new(),
                queue: VecDeque::new(),
                cancel: HashMap::new(),
            }),
            changed: Condvar::new(),
        };
        jobs.recover();
        assert_eq!(jobs.record("ja-1").unwrap()["state"], "interrupted");
        assert_eq!(jobs.record("jb-2").unwrap()["state"], "interrupted");
        assert_eq!(jobs.record("jc-3").unwrap()["state"], "done");
        let on_disk: Json =
            serde_json::from_str(&std::fs::read_to_string(dir.join("jobs/ja-1.json")).unwrap())
                .unwrap();
        assert_eq!(on_disk["state"], "interrupted");
        assert!(!jobs.busy());
    }
}
