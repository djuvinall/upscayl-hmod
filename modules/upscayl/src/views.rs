//! What the views need beyond the tools: the images a job read and wrote, files dropped
//! on a view, and one view telling the others what to show.
//!
//! INTEROP.md gives panels no channel to each other (`hdeck:open-panel` carries no
//! payload), so the views share state through this module's server, as the
//! Orchestrator's views do: `POST api/select` records the result to show, and
//! `GET api/live` is a chunked `text/event-stream` that announces each change. A drop
//! in a HollowDeck view arrives as a browser `File`, never a path (the shell turns
//! Tauri's drop handler off), so dropped files are uploaded in slices to `api/upload`
//! and kept under the data dir's `inbox/`.

use crate::batch;
use crate::jobs::Jobs;
use crate::resolve::{self, Base};
use hollowdeck_module::{http, ModuleContext, Request, Response};
use serde_json::{json, Value as Json};
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// How long a dropped file is kept after its upload.
const INBOX_KEEP: Duration = Duration::from_secs(7 * 24 * 3600);

/// One query parameter, percent-decoded.
pub fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (http::percent_decode(k) == key).then(|| http::percent_decode(&v.replace('+', " ")))
    })
}

fn content_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

// ---------------------------------------------------------------- images

/// The input and output of result `n` of a finished job, as files.
///
/// Only paths a job recorded are ever served: this is not a file server. A batch's
/// input is found by stem in its input folder; stems are unique there, because a
/// batch with two inputs of one stem is refused before it runs.
pub fn pair(ctx: &ModuleContext, job: &str, n: usize) -> Result<(PathBuf, PathBuf), (u16, String)> {
    let rec = Jobs::get(ctx)
        .record(job)
        .ok_or((404, format!("no job \"{job}\"")))?;
    let result = &rec["result"];
    let base = Base::of(
        rec["base_dir"].as_str().unwrap_or(""),
        rec["base_dir_source"].as_str().unwrap_or(""),
    );
    let inputs = &rec["inputs"];
    if rec["kind"] == "batch" {
        let outputs = result["outputs"].as_array().cloned().unwrap_or_default();
        let after = outputs
            .get(n)
            .and_then(Json::as_str)
            .map(PathBuf::from)
            .ok_or((404, format!("job \"{job}\" has no result {n}")))?;
        let folder = resolve::resolve(
            &base,
            inputs["input_folder"].as_str().unwrap_or(""),
            "input_folder",
        )
        .map_err(|e| (404, e))?;
        let stem = after.file_stem().map(|s| s.to_os_string());
        let before = batch::images_in(&folder)
            .map_err(|e| (404, e))?
            .into_iter()
            .find(|p| p.file_stem().map(|s| s.to_os_string()) == stem)
            .ok_or((404, format!("the input for {} is gone", after.display())))?;
        Ok((before, after))
    } else {
        if n != 0 {
            return Err((404, format!("job \"{job}\" has one result")));
        }
        let after = result["output"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .ok_or((404, format!("job \"{job}\" has no output yet")))?;
        let before = resolve::resolve(&base, inputs["input"].as_str().unwrap_or(""), "input")
            .map_err(|e| (404, e))?;
        Ok((before, after))
    }
}

/// `GET api/image?job=<id>&n=<index>&side=before|after`.
pub fn image(req: &Request, ctx: &ModuleContext) -> Response {
    let job = query_param(&req.query, "job").unwrap_or_default();
    let n = query_param(&req.query, "n")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let side = query_param(&req.query, "side").unwrap_or_else(|| "after".into());
    let (before, after) = match pair(ctx, &job, n) {
        Ok(p) => p,
        Err((s, m)) => return Response::error(s, &m),
    };
    let path = match side.as_str() {
        "before" => before,
        "after" => after,
        other => {
            return Response::error(
                400,
                &format!("side must be before or after, got \"{other}\""),
            )
        }
    };
    let Some(ct) = content_type(&path) else {
        return Response::error(
            415,
            &format!("{} is not a PNG, JPEG or WebP", path.display()),
        );
    };
    match std::fs::read(&path) {
        Ok(bytes) => Response::new(200, ct, bytes).header("cache-control", "no-cache"),
        Err(e) => Response::error(404, &format!("cannot read {}: {e}", path.display())),
    }
}

// ---------------------------------------------------------------- uploads

/// A drop's folder name: what the view generated, checked.
fn valid_drop(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

/// A dropped file's name, kept only if it is one plain image file name.
fn valid_name(name: &str) -> bool {
    let p = Path::new(name);
    !name.is_empty()
        && name.len() <= 200
        && !name.contains(['/', '\\', ':'])
        && name != "."
        && name != ".."
        && p.file_name().and_then(|n| n.to_str()) == Some(name)
        && content_type(p).is_some()
}

/// `POST api/upload?drop=<id>&name=<file>&offset=<bytes>`: one slice of one dropped
/// file. The SDK reads at most 16 MB of body, so a view sends files in slices; offset 0
/// starts the file, any other offset must equal what is already there.
pub fn upload(req: &Request, ctx: &ModuleContext) -> Response {
    let drop = query_param(&req.query, "drop").unwrap_or_default();
    let name = query_param(&req.query, "name").unwrap_or_default();
    let offset: u64 = match query_param(&req.query, "offset").map(|v| v.parse()) {
        Some(Ok(o)) => o,
        None => 0,
        Some(Err(_)) => return Response::error(400, "offset must be a whole number"),
    };
    if !valid_drop(&drop) {
        return Response::error(400, &format!("drop \"{drop}\" is not a drop id"));
    }
    if !valid_name(&name) {
        return Response::error(
            400,
            &format!("\"{name}\" is not a PNG, JPEG or WebP file name"),
        );
    }
    let dir = ctx.data_path("inbox").join(&drop);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Response::error(500, &format!("cannot create {}: {e}", dir.display()));
    }
    let path = dir.join(&name);
    let have = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if offset != 0 && offset != have {
        return Response::error(
            409,
            &format!("{name} has {have} bytes here, not {offset}; send it again from 0"),
        );
    }
    let file = if offset == 0 {
        std::fs::File::create(&path)
    } else {
        std::fs::OpenOptions::new().append(true).open(&path)
    };
    let written = file.and_then(|mut f| f.write_all(&req.body));
    if let Err(e) = written {
        return Response::error(500, &format!("cannot write {}: {e}", path.display()));
    }
    Response::new(
        200,
        "application/json",
        serde_json::to_vec(&json!({
            "path": path.display().to_string(),
            "folder": dir.display().to_string(),
            "size": offset + req.body.len() as u64,
        }))
        .unwrap_or_default(),
    )
}

/// Remove drops older than [`INBOX_KEEP`]. Called at start; errors are ignored, the
/// next start tries again.
pub fn prune_inbox(ctx: &ModuleContext) {
    let Ok(entries) = std::fs::read_dir(ctx.data_path("inbox")) else {
        return;
    };
    let cutoff = SystemTime::now().checked_sub(INBOX_KEEP);
    for e in entries.filter_map(Result::ok) {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .zip(cutoff)
            .is_some_and(|(m, c)| m < c);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

// ---------------------------------------------------------------- selection

struct Selection {
    seq: u64,
    value: Json,
}

fn selection() -> &'static (Mutex<Selection>, Condvar) {
    static SEL: OnceLock<(Mutex<Selection>, Condvar)> = OnceLock::new();
    SEL.get_or_init(|| {
        (
            Mutex::new(Selection {
                seq: 0,
                value: Json::Null,
            }),
            Condvar::new(),
        )
    })
}

fn current() -> (u64, Json) {
    let s = selection().0.lock().expect("selection lock");
    (s.seq, s.value.clone())
}

/// `GET api/select`: the result the views are showing, or null.
pub fn get_selection(_req: &Request, _ctx: &ModuleContext) -> Response {
    let (_, value) = current();
    Response::new(
        200,
        "application/json",
        serde_json::to_vec(&json!({"selection": value})).unwrap_or_default(),
    )
}

/// `POST api/select {"job_id", "n"}`: show this result in every Preview view.
pub fn post_selection(req: &Request, ctx: &ModuleContext) -> Response {
    let body: Json = match serde_json::from_slice(&req.body) {
        Ok(b) => b,
        Err(e) => return Response::error(400, &format!("the request body is not JSON: {e}")),
    };
    let job = body["job_id"].as_str().unwrap_or("").to_string();
    let n = body["n"].as_u64().unwrap_or(0) as usize;
    if let Err((s, m)) = pair(ctx, &job, n) {
        return Response::error(s, &m);
    }
    let value = json!({"job_id": job, "n": n});
    {
        let (lock, cv) = selection();
        let mut s = lock.lock().expect("selection lock");
        s.seq += 1;
        s.value = value.clone();
        cv.notify_all();
    }
    Response::new(
        200,
        "application/json",
        serde_json::to_vec(&json!({"selection": value})).unwrap_or_default(),
    )
}

/// One chunk of a chunked body.
fn chunk(stream: &mut TcpStream, data: &str) -> std::io::Result<()> {
    write!(stream, "{:x}\r\n{data}\r\n", data.len())?;
    stream.flush()
}

/// `GET api/live`: a `text/event-stream` with one `select` event now and one per change,
/// and a comment every 15 s so a closed view is noticed. Chunked, because an endless
/// body must be (the SDK's `Module::stream` docs): a module that dies mid-stream then
/// reads as a truncated body, not a finished one. While a view holds it open the module
/// is not idle-stopped, which is what a person looking at a Preview wants.
pub fn live(stream: &mut TcpStream) {
    let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-store\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n";
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    let (lock, cv) = selection();
    let mut seen = u64::MAX;
    loop {
        let (seq, value) = current();
        let frame = if seq != seen {
            seen = seq;
            format!("event: select\ndata: {value}\n\n")
        } else {
            ": ping\n\n".to_string()
        };
        if chunk(stream, &frame).is_err() {
            return;
        }
        let guard = lock.lock().expect("selection lock");
        let _ = cv
            .wait_timeout_while(guard, Duration::from_secs(15), |s| s.seq == seen)
            .expect("selection lock");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs;
    use hollowdeck_module::Headers;

    fn ctx(name: &str) -> ModuleContext {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        ModuleContext {
            module_id: "upscayl".into(),
            module_dir: dir.clone(),
            mount_path: "/m/upscayl".into(),
            shared_data_dir: dir.join("target/test-data"),
            data_dir: dir.join("target/test-data/views").join(name),
            core_version: None,
            core_bind: "127.0.0.1".into(),
            core_url: None,
            secret: None,
            port: 1,
        }
    }

    fn req(method: &str, path: &str, query: &str, body: Vec<u8>) -> Request {
        Request {
            method: method.into(),
            path: path.into(),
            query: query.into(),
            headers: Headers::new(),
            body,
        }
    }

    fn body(r: &Response) -> Json {
        serde_json::from_slice(&r.body).unwrap_or(Json::Null)
    }

    #[test]
    fn a_file_arrives_in_slices_and_a_gap_is_refused() {
        let c = ctx("upload");
        let _ = std::fs::remove_dir_all(&c.data_dir);
        let q = |offset: u64| format!("drop=d1&name=my%20pic.png&offset={offset}");
        let r = upload(&req("POST", "/api/upload", &q(0), b"abc".to_vec()), &c);
        assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
        let r = upload(&req("POST", "/api/upload", &q(3), b"def".to_vec()), &c);
        assert_eq!(body(&r)["size"], 6);
        let path = PathBuf::from(body(&r)["path"].as_str().unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"abcdef");
        assert!(path.ends_with("inbox/d1/my pic.png"));
        // A slice that does not continue the file is refused, and nothing is written.
        let r = upload(&req("POST", "/api/upload", &q(10), b"x".to_vec()), &c);
        assert_eq!(r.status, 409);
        assert_eq!(std::fs::read(&path).unwrap(), b"abcdef");
        // Offset 0 starts the file again.
        upload(&req("POST", "/api/upload", &q(0), b"z".to_vec()), &c);
        assert_eq!(std::fs::read(&path).unwrap(), b"z");
        for bad in [
            "drop=..&name=a.png",
            "drop=d1&name=..%2Fa.png",
            "drop=d1&name=a.exe",
        ] {
            let r = upload(&req("POST", "/api/upload", bad, b"x".to_vec()), &c);
            assert_eq!(r.status, 400, "{bad}");
        }
    }

    #[test]
    fn a_finished_job_serves_its_before_and_after_and_can_be_selected() {
        let c = ctx("image");
        if !c.module_path(crate::paths::ENGINE_REL).is_file() {
            eprintln!("skipped: engine not staged");
            return;
        }
        let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/samples/sample.png");
        let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-out/views");
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir).unwrap();
        let start = json!({"inputs": {"kind": "image", "input": input.display().to_string(), "output": format!("{}/", out_dir.display()), "model": "upscayl-lite-4x", "scale": 2}});
        let r = jobs::start_job(
            &req(
                "POST",
                "/tools/start_job",
                "",
                start.to_string().into_bytes(),
            ),
            &c,
        );
        assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
        let id = body(&r)["outputs"]["job_id"].as_str().unwrap().to_string();
        let done = Jobs::get(&c).wait(&id, Duration::from_secs(120)).unwrap();
        assert_eq!(done["state"], "done", "{done}");

        let q = |side: &str| format!("job={id}&n=0&side={side}");
        let before = image(&req("GET", "/api/image", &q("before"), vec![]), &c);
        assert_eq!(
            before.status,
            200,
            "{}",
            String::from_utf8_lossy(&before.body)
        );
        assert_eq!(before.body, std::fs::read(&input).unwrap());
        let after = image(&req("GET", "/api/image", &q("after"), vec![]), &c);
        assert_eq!(after.status, 200);
        assert_eq!(imagesize::blob_size(&after.body).unwrap().width, 512);
        assert_eq!(
            image(
                &req("GET", "/api/image", &format!("job={id}&n=1"), vec![]),
                &c
            )
            .status,
            404
        );
        assert_eq!(
            image(&req("GET", "/api/image", "job=nope", vec![]), &c).status,
            404
        );

        let sel = |b: Json| {
            post_selection(
                &req("POST", "/api/select", "", b.to_string().into_bytes()),
                &c,
            )
        };
        assert_eq!(sel(json!({"job_id": id, "n": 0})).status, 200);
        assert_eq!(
            body(&get_selection(&req("GET", "/api/select", "", vec![]), &c))["selection"]["job_id"],
            id.as_str()
        );
        assert_eq!(sel(json!({"job_id": "nope"})).status, 404);
    }

    #[test]
    fn the_live_stream_is_chunked_and_announces_each_selection() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            live(&mut s);
        });
        let mut client = TcpStream::connect(addr).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut got = String::new();
        let mut buf = [0u8; 4096];
        let mut read_until = |needle: &str, got: &mut String| {
            while !got.contains(needle) {
                let n = client.read(&mut buf).expect("the stream answered in time");
                assert!(n > 0, "the stream closed: {got}");
                got.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        };
        read_until("event: select", &mut got);
        assert!(got.starts_with("HTTP/1.1 200 OK\r\n"), "{got}");
        assert!(got.contains("transfer-encoding: chunked"), "{got}");
        assert!(got.contains("content-type: text/event-stream"), "{got}");
        // A change is pushed without being asked for.
        {
            let (lock, cv) = selection();
            let mut st = lock.lock().unwrap();
            st.seq += 1;
            st.value = json!({"job_id": "jlive", "n": 3});
            cv.notify_all();
        }
        read_until("\"jlive\"", &mut got);
        assert!(got.contains(r#"data: {"job_id":"jlive","n":3}"#), "{got}");
    }

    #[test]
    fn query_params_decode() {
        assert_eq!(query_param("job=j1&n=2", "n").as_deref(), Some("2"));
        assert_eq!(
            query_param("name=my%20photo.png&drop=d1", "name").as_deref(),
            Some("my photo.png")
        );
        assert_eq!(query_param("a=1", "b"), None);
    }

    #[test]
    fn only_plain_image_names_are_accepted() {
        for ok in ["a.png", "photo 1.JPG", "x.webp", "y.jpeg"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "",
            "..",
            "../a.png",
            "a/b.png",
            "a\\b.png",
            "C:a.png",
            "notes.txt",
            "a",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
        assert!(valid_drop("d18f2a-3"));
        assert!(!valid_drop("../x"));
        assert!(!valid_drop(""));
    }
}
