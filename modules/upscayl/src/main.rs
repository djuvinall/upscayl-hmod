//! The `upscayl` HollowDeck module: Upscayl's `upscayl-bin` engine as tool nodes.
//!
//! The contract is HollowDeck's `INTEROP.md`; this repo's map is `ARCHITECTURE.md` and
//! `docs/arch/upscayl-module.md`. The SDK supplies the handshake, the two-mode guard,
//! `/health`, `/lifecycle` and static files; everything here is this module's own.

use hollowdeck_module::{json::Value, Module, ModuleContext, Request, Response};

mod args;
mod assets;
mod batch;
mod engine;
mod jobs;
mod models;
mod paths;
mod resolve;
mod server;
mod upscale;
mod views;

fn main() -> Result<(), String> {
    let ctx = ModuleContext::from_env()?;
    if std::env::args().any(|a| a == "--selfcheck") {
        return selfcheck(&ctx);
    }
    ctx.log(
        "info",
        "upscayl.started",
        "the Upscayl module is coming up",
        &[("port".to_string(), Value::from(ctx.port))],
    );
    // Recover the job table now: a job the last process left running is marked
    // interrupted at start, not when someone next asks.
    jobs::Jobs::get(&ctx);
    views::prune_inbox(&ctx);
    server::serve(build(ctx), routes())
}

/// Every route, in one place, so tests can drive the same module the binary serves.
fn build(ctx: ModuleContext) -> Module {
    Module::new(ctx)
        .get("/api/status", status)
        .tool("upscale_image", upscale::handle)
        .tool("upscale_batch", batch::handle)
        .tool("import_model", assets::import_model)
        .tool("list_models", assets::list_models)
        .tool("start_job", jobs::start_job)
        .tool("job_status", jobs::job_status)
        .tool("wait_job", jobs::wait_job)
        .tool("cancel_job", jobs::cancel_job)
        .get("/api/jobs", jobs::list)
        .get("/api/image", views::image)
        .post("/api/upload", views::upload)
        .get("/api/select", views::get_selection)
        .post("/api/select", views::post_selection)
        // A running or queued job keeps the module alive while nobody is watching.
        .hold_while(|_| jobs::busy_now())
        .get("/api/assets", assets::list)
        .post("/api/assets", assets::create)
        .get("/module.json", |_req, ctx| {
            match std::fs::read(ctx.module_path("module.json")) {
                Ok(bytes) => Response::new(200, "application/json", bytes),
                Err(e) => Response::error(500, &format!("cannot read module.json: {e}")),
            }
        })
        // The other views sit beside index.html, at the module root, so their relative
        // URLs (api/..., static/...) resolve exactly as the main view's do.
        .get("/preview.html", |_req, ctx| page(ctx, "preview.html"))
        .get("/models.html", |_req, ctx| page(ctx, "models.html"))
        .statics("static")
}

/// One of the views' pages, from `static/`.
fn page(ctx: &ModuleContext, name: &str) -> Response {
    match std::fs::read(ctx.module_path(&format!("static/{name}"))) {
        Ok(bytes) => Response::new(200, "text/html; charset=utf-8", bytes),
        Err(e) => Response::error(500, &format!("cannot read static/{name}: {e}")),
    }
}

/// The routes with an id in the path, which the SDK cannot match (see `server.rs`).
fn routes() -> Vec<server::PrefixRoute> {
    use std::sync::Arc;
    vec![
        server::PrefixRoute {
            method: "GET",
            prefix: "/api/assets",
            handler: Arc::new(assets::get),
        },
        server::PrefixRoute {
            method: "DELETE",
            prefix: "/api/assets",
            handler: Arc::new(assets::delete),
        },
    ]
}

/// `GET api/status` -- what the panel shows: the module version and whether the engine
/// and the license record are where the module expects them.
fn status(_req: &Request, ctx: &ModuleContext) -> Response {
    let engine = paths::engine_binary(ctx);
    let licenses = ctx.module_path(paths::LICENSES_FILE);
    Response::json(
        200,
        &Value::object()
            .with("version", env!("CARGO_PKG_VERSION").into())
            .with("platform", paths::PLATFORM.into())
            .with("engine_staged", engine.is_file().into())
            .with("engine_path", paths::ENGINE_REL.into())
            .with("licenses_present", licenses.is_file().into())
            .with("data_dir", ctx.data_dir.display().to_string().into()),
    )
}

/// What `module.json`'s `verify` runs: the files without which the module cannot work.
/// A missing engine is not a failure here -- it is staged by the sync script, and the
/// panel says so -- but a missing manifest, panel or license record is.
fn selfcheck(ctx: &ModuleContext) -> Result<(), String> {
    for rel in [
        "module.json",
        "static/index.html",
        "static/preview.html",
        "static/models.html",
        paths::LICENSES_FILE,
    ] {
        let p = ctx.module_path(rel);
        if !p.is_file() {
            return Err(format!("missing {}", p.display()));
        }
    }
    println!("upscayl: ok");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollowdeck_module::Headers;
    use std::path::PathBuf;

    const PORT: u16 = 47123;

    fn ctx(secret: Option<&str>) -> ModuleContext {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        ModuleContext {
            module_id: "upscayl".into(),
            module_dir: dir.clone(),
            mount_path: "/m/upscayl".into(),
            shared_data_dir: dir.join("target").join("test-data"),
            data_dir: dir
                .join("target")
                .join("test-data")
                .join("module_data")
                .join("upscayl"),
            core_version: None,
            core_bind: "127.0.0.1".into(),
            core_url: None,
            secret: secret.map(str::to_string),
            port: PORT,
        }
    }

    fn get(path: &str, headers: &[(&str, &str)]) -> Request {
        let mut h = Headers::new();
        for (k, v) in headers {
            h.push(k, v);
        }
        Request {
            method: "GET".into(),
            path: path.into(),
            query: String::new(),
            headers: h,
            body: vec![],
        }
    }

    #[test]
    fn hosted_refuses_a_request_without_the_secret() {
        let m = build(ctx(Some("s3cret")));
        let r = m.answer(&get("/health", &[("host", "127.0.0.1:9999")]), PORT);
        assert_eq!(r.status, 421);
    }

    #[test]
    fn hosted_answers_with_the_secret_whatever_the_host() {
        let m = build(ctx(Some("s3cret")));
        let req = get(
            "/health",
            &[
                ("host", "core.example:1234"),
                ("x-hdeck-module-secret", "s3cret"),
            ],
        );
        let r = m.answer(&req, PORT);
        assert_eq!(r.status, 200);
        assert!(String::from_utf8_lossy(&r.body).contains("\"ok\":true"));
    }

    #[test]
    fn standalone_refuses_a_foreign_host() {
        let m = build(ctx(None));
        let r = m.answer(
            &get("/health", &[("host", "127.0.0.1.evil.example:47123")]),
            PORT,
        );
        assert_eq!(r.status, 421);
    }

    #[test]
    fn standalone_answers_its_own_loopback_host() {
        let m = build(ctx(None));
        let r = m.answer(&get("/health", &[("host", "127.0.0.1:47123")]), PORT);
        assert_eq!(r.status, 200);
    }

    #[test]
    fn status_and_manifest_are_served_under_the_mount() {
        let m = build(ctx(None));
        let host = [("host", "127.0.0.1:47123")];
        let status = m.answer(&get("/m/upscayl/api/status", &host), PORT);
        assert_eq!(status.status, 200);
        assert!(String::from_utf8_lossy(&status.body).contains("\"licenses_present\":true"));
        let manifest = m.answer(&get("/module.json", &host), PORT);
        assert_eq!(manifest.status, 200);
        assert!(String::from_utf8_lossy(&manifest.body).contains("\"id\": \"upscayl\""));
        let panel = m.answer(&get("/", &host), PORT);
        assert_eq!(panel.status, 200);
        // Every view the manifest declares is served at its path, under the mount too.
        let manifest: serde_json::Value = serde_json::from_slice(&manifest.body).unwrap();
        for p in manifest["panels"].as_array().unwrap() {
            let path = format!("/m/upscayl/{}", p["path"].as_str().unwrap());
            let page = m.answer(&get(&path, &host), PORT);
            assert_eq!(page.status, 200, "{path}");
            assert!(
                String::from_utf8_lossy(&page.body).contains("<title>Upscayl"),
                "{path}"
            );
        }
    }

    /// `resolve.rs` is HollowDeck's reference path resolver carried verbatim
    /// (INTEROP.md section 12: what a relative path means may not have two
    /// implementations). Compared when the sibling checkout is present; line endings are
    /// normalised because each checkout's git decides those.
    #[test]
    fn the_path_resolver_is_still_the_reference_copy() {
        let ours = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/resolve.rs");
        let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../HollowDeck/core_modules/system/src/tools/paths.rs");
        let Ok(theirs) = std::fs::read(&reference) else {
            eprintln!("skipped: no HollowDeck checkout at {}", reference.display());
            return;
        };
        let norm = |b: Vec<u8>| b.into_iter().filter(|c| *c != b'\r').collect::<Vec<u8>>();
        let mine = norm(std::fs::read(ours).expect("resolve.rs"));
        assert!(
            mine == norm(theirs),
            "src/resolve.rs has drifted from {}; copy it again",
            reference.display()
        );
    }

    fn answer(m: &Module, req: &Request) -> Response {
        server::answer(m, &routes(), req, PORT)
    }

    fn request(method: &str, path: &str, body: &str) -> Request {
        let mut h = Headers::new();
        h.push("host", "127.0.0.1:47123");
        Request {
            method: method.into(),
            path: path.into(),
            query: String::new(),
            headers: h,
            body: body.as_bytes().to_vec(),
        }
    }

    fn json_of(r: &Response) -> serde_json::Value {
        serde_json::from_slice(&r.body).unwrap_or(serde_json::Value::Null)
    }

    fn asset_ctx(name: &str) -> ModuleContext {
        let mut c = ctx(None);
        c.data_dir = c.module_dir.join("target/test-data/assets").join(name);
        let _ = std::fs::remove_dir_all(&c.data_dir);
        c
    }

    #[test]
    fn bundled_models_are_listed_as_licensed_assets_and_cannot_be_deleted() {
        let m = build(asset_ctx("list"));
        let r = answer(&m, &request("GET", "/m/upscayl/api/assets", ""));
        assert_eq!(r.status, 200);
        let v = json_of(&r);
        assert_eq!(v["owner"], "upscayl");
        let list = v["assets"].as_array().unwrap();
        assert_eq!(list.len(), 10);
        let us = list
            .iter()
            .find(|a| a["id"] == "bundled-ultrasharp-4x")
            .unwrap();
        assert_eq!(us["kind"], "ncnn_model");
        assert_eq!(us["properties"]["license"], "CC-BY-NC-SA-4.0");
        assert_eq!(us["properties"]["commercial_use"], "forbidden");
        assert!(us.get("payload").is_none(), "a listing carries no payload");
        let one = json_of(&answer(
            &m,
            &request("GET", "/api/assets/bundled-ultrasharp-4x", ""),
        ));
        assert_eq!(
            one["asset"]["payload"]["outputs"]["model"],
            "upscayl:bundled/ultrasharp-4x"
        );
        let del = answer(
            &m,
            &request("DELETE", "/api/assets/bundled-ultrasharp-4x", ""),
        );
        assert_eq!(del.status, 409);
        assert_eq!(
            answer(&m, &request("GET", "/api/assets/nope", "")).status,
            404
        );
        assert_eq!(
            answer(&m, &request("GET", "/api/assets/Bad", "")).status,
            400
        );
    }

    #[test]
    fn a_generic_asset_can_be_created_read_and_deleted() {
        let m = build(asset_ctx("generic"));
        let r = answer(
            &m,
            &request(
                "POST",
                "/api/assets",
                r#"{"name": "Note", "kind": "note", "payload": {"x": 1}}"#,
            ),
        );
        assert_eq!(r.status, 201, "{}", String::from_utf8_lossy(&r.body));
        let id = json_of(&r)["asset"]["id"].as_str().unwrap().to_string();
        assert_eq!(id, "note");
        assert_eq!(
            json_of(&answer(
                &m,
                &request("GET", &format!("/api/assets/{id}"), "")
            ))["asset"]["payload"]["x"],
            1
        );
        assert_eq!(
            answer(&m, &request("DELETE", &format!("/api/assets/{id}"), "")).status,
            200
        );
        assert_eq!(
            answer(&m, &request("GET", &format!("/api/assets/{id}"), "")).status,
            404
        );
    }

    /// The asset envelope is HollowDeck's (`shared/python/assets.py`), carried verbatim.
    #[test]
    fn the_vendored_assets_py_is_still_the_reference_copy() {
        let ours = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/assets.py");
        let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../HollowDeck/shared/python/assets.py");
        let Ok(theirs) = std::fs::read(&reference) else {
            eprintln!("skipped: no HollowDeck checkout at {}", reference.display());
            return;
        };
        let norm = |b: Vec<u8>| b.into_iter().filter(|c| *c != b'\r').collect::<Vec<u8>>();
        assert!(
            norm(std::fs::read(ours).expect("vendor/assets.py")) == norm(theirs),
            "vendor/assets.py has drifted from {}",
            reference.display()
        );
    }

    /// Every record this module serves loads through `assets.py`'s own `Asset.from_dict`
    /// and round-trips unchanged: the envelope has one implementation, and this proves
    /// the Rust one matches it. Skipped when no Python is on PATH.
    #[test]
    fn every_record_round_trips_through_assets_py() {
        let c = asset_ctx("roundtrip");
        let m = build(c.clone());
        let r = answer(
            &m,
            &request(
                "POST",
                "/api/assets",
                r#"{"name": "Rec", "kind": "ncnn_model", "tags": ["a"], "properties": {"license": "MIT"}, "payload": {"outputs": {"model": "x"}}}"#,
            ),
        );
        assert_eq!(r.status, 201);
        let listing = json_of(&answer(&m, &request("GET", "/api/assets", "")));
        let full: Vec<serde_json::Value> = listing["assets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                json_of(&answer(
                    &m,
                    &request(
                        "GET",
                        &format!("/api/assets/{}", a["id"].as_str().unwrap()),
                        "",
                    ),
                ))
            })
            .map(|v| v["asset"].clone())
            .collect();
        let file = c.data_dir.join("records.json");
        std::fs::create_dir_all(&c.data_dir).unwrap();
        std::fs::write(
            &file,
            serde_json::to_vec(&serde_json::json!({"summaries": listing["assets"], "full": full}))
                .unwrap(),
        )
        .unwrap();
        let script = r#"
import json, sys
sys.path.insert(0, sys.argv[1])
from assets import Asset
d = json.load(open(sys.argv[2], encoding="utf-8"))
for s in d["summaries"]:
    got = Asset.from_dict(s).summary()
    assert got == s, (s["id"], got, s)
for f in d["full"]:
    got = Asset.from_dict(f).to_dict()
    assert got == f, (f["id"], got, f)
print("ok", len(d["summaries"]), len(d["full"]))
"#;
        let vendor = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor");
        let out = match std::process::Command::new("python")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .arg("-c")
            .arg(script)
            .arg(&vendor)
            .arg(&file)
            .output()
        {
            Ok(o) => o,
            Err(_) => {
                eprintln!("skipped: no python on PATH");
                return;
            }
        };
        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out.stdout).starts_with("ok 11 11"),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }

    /// Import a model, run it through its token, then delete it and its files.
    #[test]
    fn an_imported_model_is_an_asset_its_token_runs_and_delete_removes_its_files() {
        let c = asset_ctx("import");
        if !c.module_dir.join(paths::ENGINE_REL).is_file() {
            eprintln!("skipped: engine not staged");
            return;
        }
        let src = c.data_dir.join("incoming/custom");
        std::fs::create_dir_all(&src).unwrap();
        for ext in ["param", "bin"] {
            std::fs::copy(
                c.module_dir
                    .join(format!("engine/models/upscayl-lite-4x.{ext}")),
                src.join(format!("my-lite-4x.{ext}")),
            )
            .unwrap();
        }
        let m = build(c.clone());
        let body = serde_json::json!({"inputs": {"param": src.join("my-lite-4x.param").display().to_string(), "name": "My Lite", "license": "BSD-3-Clause", "commercial_use": "allowed"}, "base_dir": env!("CARGO_MANIFEST_DIR"), "base_dir_source": "project"});
        let r = answer(
            &m,
            &request("POST", "/tools/import_model", &body.to_string()),
        );
        assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
        let o = json_of(&r)["outputs"].clone();
        assert_eq!(o["model"], "upscayl:imported/my-lite");
        assert!(c
            .data_dir
            .join("imported/my-lite/models/my-lite-4x.bin")
            .is_file());
        let listed = json_of(&answer(
            &m,
            &request("POST", "/tools/list_models", r#"{"inputs": {}}"#),
        ));
        assert_eq!(listed["outputs"]["count"], 11);
        let out = c.data_dir.join("run/out.png");
        let up = serde_json::json!({"inputs": {"input": c.module_dir.join("../../to_upscale.jpeg").display().to_string(), "output": out.display().to_string(), "custom_model": "upscayl:imported/my-lite", "scale": 2}, "base_dir": env!("CARGO_MANIFEST_DIR"), "base_dir_source": "project"});
        let r = answer(
            &m,
            &request("POST", "/tools/upscale_image", &up.to_string()),
        );
        assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
        assert_eq!(json_of(&r)["outputs"]["license"], "BSD-3-Clause");
        assert_eq!(
            answer(&m, &request("DELETE", "/api/assets/my-lite", "")).status,
            200
        );
        assert!(!c.data_dir.join("imported/my-lite").exists());
        let r = answer(
            &m,
            &request("POST", "/tools/upscale_image", &up.to_string()),
        );
        assert_eq!(r.status, 400);
        assert!(String::from_utf8_lossy(&r.body).contains("no imported model"));
    }

    #[test]
    fn selfcheck_passes_on_the_checkout() {
        assert_eq!(selfcheck(&ctx(None)), Ok(()));
    }
}
