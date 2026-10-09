//! The `upscayl` HollowDeck module: Upscayl's `upscayl-bin` engine as tool nodes.
//!
//! The contract is HollowDeck's `INTEROP.md`; this repo's map is `ARCHITECTURE.md` and
//! `docs/arch/upscayl-module.md`. The SDK supplies the handshake, the two-mode guard,
//! `/health`, `/lifecycle` and static files; everything here is this module's own.

use hollowdeck_module::{json::Value, Module, ModuleContext, Request, Response};

mod paths;

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
    build(ctx).serve()
}

/// Every route, in one place, so tests can drive the same module the binary serves.
fn build(ctx: ModuleContext) -> Module {
    Module::new(ctx)
        .get("/api/status", status)
        .get("/module.json", |_req, ctx| {
            match std::fs::read(ctx.module_path("module.json")) {
                Ok(bytes) => Response::new(200, "application/json", bytes),
                Err(e) => Response::error(500, &format!("cannot read module.json: {e}")),
            }
        })
        .statics("static")
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
    for rel in ["module.json", "static/index.html", paths::LICENSES_FILE] {
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
    }

    #[test]
    fn selfcheck_passes_on_the_checkout() {
        assert_eq!(selfcheck(&ctx(None)), Ok(()));
    }
}
