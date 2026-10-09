//! This module's accept loop: the SDK's, plus routes with an id in the path.
//!
//! `hollowdeck-module` matches exact paths only, and the asset surface every asset owner
//! serves is `GET/DELETE api/assets/{id}` (HollowDeck `docs/arch/assets.md`). INTEROP.md
//! allows a module to run its own server as long as it keeps the SDK's guard: every
//! request here goes through `Module::refuse` first, exactly as `Module::serve` does,
//! and everything that is not a prefix route is answered by `Module::answer`.

use hollowdeck_module::{http, Module, ModuleContext, Request, Response};
use std::net::TcpListener;
use std::sync::Arc;

type Handler = Arc<dyn Fn(&Request, &ModuleContext, &str) -> Response + Send + Sync>;

/// A route whose last path segment is a parameter: `GET /api/assets/<id>`.
pub struct PrefixRoute {
    pub method: &'static str,
    /// Without the trailing slash: `/api/assets`.
    pub prefix: &'static str,
    pub handler: Handler,
}

/// The one segment after `prefix/`, if this path is under it and has exactly one more.
pub fn param<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = path.strip_prefix(prefix)?.strip_prefix('/')?;
    let rest = rest.trim_end_matches('/');
    (!rest.is_empty() && !rest.contains('/')).then_some(rest)
}

/// The path with the host's mount prefix taken off, if it is there (the SDK does the
/// same, privately).
pub fn strip_mount(path: &str, mount: &str) -> String {
    path.strip_prefix(mount)
        .filter(|rest| rest.is_empty() || rest.starts_with('/'))
        .unwrap_or(path)
        .to_string()
}

/// Answer one request: the guard, then a prefix route, then the SDK's own routing.
pub fn answer(module: &Module, routes: &[PrefixRoute], request: &Request, port: u16) -> Response {
    if let Some(refusal) = module.refuse(request, port) {
        return refusal;
    }
    let ctx = module.context();
    let path = strip_mount(&request.path, &ctx.mount_path);
    for r in routes {
        if request.method.eq_ignore_ascii_case(r.method) {
            if let Some(id) = param(&path, r.prefix) {
                let mut clean = request.clone();
                clean.headers = request
                    .headers
                    .without(hollowdeck_module::guard::SECRET_HEADER);
                return (r.handler)(&clean, ctx, id);
            }
        }
    }
    module.answer(request, port)
}

/// Bind the port the host gave, on loopback, and answer until killed. `127.0.0.1` is
/// written out, as the SDK does: nothing here may listen on a network.
pub fn serve(module: Module, routes: Vec<PrefixRoute>) -> Result<(), String> {
    let port = module.context().port;
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("cannot bind 127.0.0.1:{port}: {e}"))?;
    println!(
        "{} listening on http://127.0.0.1:{port}/ (module dir {})",
        module.context().module_id,
        module.context().module_dir.display()
    );
    let module = Arc::new(module);
    let routes = Arc::new(routes);
    for incoming in listener.incoming() {
        let Ok(mut stream) = incoming else { continue };
        let module = Arc::clone(&module);
        let routes = Arc::clone(&routes);
        std::thread::spawn(move || {
            let response = match http::read_request(&stream) {
                Ok(Some(request)) => answer(&module, &routes, &request, port),
                Ok(None) => return,
                Err(refusal) => refusal,
            };
            let _ = http::write_response(&mut stream, &response);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_takes_exactly_one_segment() {
        assert_eq!(param("/api/assets/abc", "/api/assets"), Some("abc"));
        assert_eq!(param("/api/assets/abc/", "/api/assets"), Some("abc"));
        assert_eq!(param("/api/assets", "/api/assets"), None);
        assert_eq!(param("/api/assets/", "/api/assets"), None);
        assert_eq!(param("/api/assets/a/b", "/api/assets"), None);
        assert_eq!(param("/api/assetsx/a", "/api/assets"), None);
    }

    #[test]
    fn the_mount_comes_off_only_as_a_whole_segment() {
        assert_eq!(strip_mount("/m/upscayl/api/x", "/m/upscayl"), "/api/x");
        assert_eq!(
            strip_mount("/m/upscaylx/api", "/m/upscayl"),
            "/m/upscaylx/api"
        );
        assert_eq!(strip_mount("/api/x", "/m/upscayl"), "/api/x");
    }
}
