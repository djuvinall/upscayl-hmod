//! Models as HollowDeck assets of kind `ncnn_model` (`docs/arch/model-assets.md`).
//!
//! The envelope is HollowDeck's (`shared/python/assets.py`, carried verbatim at
//! `vendor/assets.py` and checked against in a test). Two sources:
//!
//! * **bundled** models are listed from `licenses/models.json`, not stored: their record
//!   is derived on each read, ids `bundled-<name>`, and they cannot be deleted;
//! * **imported** models are stored as `<data dir>/assets/<id>.json`, with their files at
//!   `<data dir>/imported/<id>/models/` -- a folder named `models`, because the engine
//!   will not load from any other (engine probe, finding 8).
//!
//! Every asset carries its model's license in `properties`, and its single output is a
//! model token for `custom_model`.

use crate::args::scale_from_name;
use crate::models::{self, TOKEN_PREFIX};
use crate::paths;
use crate::resolve;
use crate::upscale::Call;
use hollowdeck_module::{ModuleContext, Request, Response};
use serde_json::{json, Map, Value as Json};
use std::path::PathBuf;

pub const KIND: &str = "ncnn_model";
const BUNDLED_PREFIX: &str = "bundled-";

fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn json_response(status: u16, v: &Json) -> Response {
    Response::new(
        status,
        "application/json",
        serde_json::to_vec(v).unwrap_or_default(),
    )
}

/// The id rule from `assets.py`: `^[a-z0-9][a-z0-9_-]{0,63}$`.
pub fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-')
}

/// `assets.py`'s `slug`: ASCII, runs of anything else to `-`, lowercase, 48 chars.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(48).collect();
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "asset".into()
    } else {
        out
    }
}

fn interface() -> Json {
    json!({"inputs": [], "outputs": [{"name": "model", "type": "str", "description": "A model token for upscayl's custom_model socket"}]})
}

fn assets_dir(ctx: &ModuleContext) -> PathBuf {
    ctx.data_path("assets")
}

pub fn imported_models_dir(ctx: &ModuleContext, id: &str) -> PathBuf {
    ctx.data_path("imported").join(id).join("models")
}

/// The full record of a bundled model.
fn bundled_asset(ctx: &ModuleContext, m: &models::Bundled, raw: &Json) -> Json {
    let staged = ["param", "bin"].iter().all(|e| {
        ctx.module_path(paths::MODELS_REL)
            .join(format!("{}.{e}", m.name))
            .is_file()
    });
    let s = |k: &str| raw[k].as_str().unwrap_or("").to_string();
    json!({
        "id": format!("{BUNDLED_PREFIX}{}", m.name),
        "name": m.name,
        "owner": ctx.module_id,
        "origin": "ingested",
        "kind": KIND,
        "captured_against": version(),
        "tags": ["upscale", "ncnn", "bundled"],
        "interface": interface(),
        "properties": {
            "model_name": m.name,
            "native_scale": m.native_scale,
            "license": m.license,
            "license_url": s("license_url"),
            "license_file": s("license_file"),
            "source_url": s("source_url"),
            "author": s("author"),
            "commercial_use": m.commercial_use,
            "attribution": m.attribution,
            "original_name": s("original_name"),
            "staged": staged,
        },
        "payload": {"outputs": {"model": format!("{TOKEN_PREFIX}bundled/{}", m.name)}},
        "created_at": 0.0,
        "api_version": 1,
    })
}

fn bundled_all(ctx: &ModuleContext) -> Result<Vec<Json>, String> {
    let list = models::bundled(ctx)?;
    let text = std::fs::read_to_string(ctx.module_path(paths::LICENSES_FILE)).unwrap_or_default();
    let doc: Json = serde_json::from_str(&text).unwrap_or(Json::Null);
    let raws = doc["models"].as_array().cloned().unwrap_or_default();
    Ok(list
        .iter()
        .map(|m| {
            let raw = raws
                .iter()
                .find(|r| r["name"] == m.name.as_str())
                .cloned()
                .unwrap_or(Json::Null);
            bundled_asset(ctx, m, &raw)
        })
        .collect())
}

fn read_stored(ctx: &ModuleContext, id: &str) -> Result<Option<Json>, String> {
    let path = assets_dir(ctx).join(format!("{id}.json"));
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{id} will not load: {e}"))?;
    let mut v: Json =
        serde_json::from_str(&text).map_err(|e| format!("{id} will not load: {e}"))?;
    v["id"] = json!(id);
    if v["owner"].as_str().unwrap_or("").is_empty() {
        v["owner"] = json!(ctx.module_id);
    }
    Ok(Some(v))
}

fn write_stored(ctx: &ModuleContext, asset: &Json) -> Result<(), String> {
    let id = asset["id"].as_str().unwrap_or_default();
    let dir = assets_dir(ctx);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let path = dir.join(format!("{id}.json"));
    let tmp = dir.join(format!("{id}.json.tmp"));
    let text = serde_json::to_string_pretty(asset).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn stored_ids(ctx: &ModuleContext) -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(assets_dir(ctx))
        .map(|d| {
            d.filter_map(Result::ok)
                .filter_map(|e| {
                    let p = e.path();
                    (p.extension().and_then(|x| x.to_str()) == Some("json"))
                        .then(|| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
                        .flatten()
                })
                .filter(|id| valid_id(id))
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

fn new_id(ctx: &ModuleContext, base: &str) -> String {
    let mut candidate = slug(base);
    if candidate.starts_with(BUNDLED_PREFIX.trim_end_matches('-')) {
        candidate = format!("m-{candidate}");
    }
    let existing = stored_ids(ctx);
    if !existing.contains(&candidate) {
        return candidate;
    }
    (2..1000)
        .map(|n| format!("{candidate}-{n}"))
        .find(|c| !existing.contains(c))
        .unwrap_or_else(|| format!("{candidate}-x"))
}

/// What a listing shows: the record without its payload.
fn summary(full: &Json) -> Json {
    let mut s = full.clone();
    if let Some(o) = s.as_object_mut() {
        o.remove("payload");
    }
    s
}

/// One asset by id, bundled or stored.
pub fn find(ctx: &ModuleContext, id: &str) -> Result<Option<Json>, String> {
    if let Some(name) = id.strip_prefix(BUNDLED_PREFIX) {
        return Ok(bundled_all(ctx)?.into_iter().find(|a| a["name"] == name));
    }
    read_stored(ctx, id)
}

// ---------------------------------------------------------------- the HTTP surface

/// `GET api/assets` -> `{"owner", "assets": [summary], "broken": [...]}`.
pub fn list(_req: &Request, ctx: &ModuleContext) -> Response {
    let mut assets: Vec<Json> = Vec::new();
    let mut broken: Vec<Json> = Vec::new();
    match bundled_all(ctx) {
        Ok(b) => assets.extend(b.iter().map(summary)),
        Err(e) => {
            broken.push(json!({"id": "bundled", "name": "bundled", "broken": true, "reason": e}))
        }
    }
    for id in stored_ids(ctx) {
        match read_stored(ctx, &id) {
            Ok(Some(a)) => assets.push(summary(&a)),
            Ok(None) => {}
            Err(e) => broken.push(json!({"id": id, "name": id, "broken": true, "reason": e})),
        }
    }
    json_response(
        200,
        &json!({"owner": ctx.module_id, "assets": assets, "broken": broken}),
    )
}

/// `GET api/assets/{id}` -> `{"asset": full}`.
pub fn get(_req: &Request, ctx: &ModuleContext, id: &str) -> Response {
    if !valid_id(id) {
        return Response::error(400, &format!("'{id}' is not a valid asset id"));
    }
    match find(ctx, id) {
        Ok(Some(a)) => json_response(200, &json!({"asset": a})),
        Ok(None) => Response::error(404, &format!("no asset '{id}'")),
        Err(e) => Response::error(500, &e),
    }
}

/// `DELETE api/assets/{id}`. Bundled models cannot be deleted; an imported model's
/// files go with its record.
pub fn delete(_req: &Request, ctx: &ModuleContext, id: &str) -> Response {
    if !valid_id(id) {
        return Response::error(400, &format!("'{id}' is not a valid asset id"));
    }
    if id.starts_with(BUNDLED_PREFIX) {
        return Response::error(
            409,
            &format!("'{id}' is a bundled model; it ships with the module and cannot be deleted"),
        );
    }
    let path = assets_dir(ctx).join(format!("{id}.json"));
    if !path.is_file() {
        return Response::error(404, &format!("no asset '{id}'"));
    }
    if let Err(e) = std::fs::remove_file(&path) {
        return Response::error(500, &format!("cannot delete {}: {e}", path.display()));
    }
    let files = ctx.data_path("imported").join(id);
    let _ = std::fs::remove_dir_all(&files);
    ctx.log(
        "warning",
        "assets.deleted",
        &format!("removed asset {id}"),
        &[("asset".to_string(), id.into())],
    );
    json_response(200, &json!({"ok": true, "id": id}))
}

/// The envelope `assets.py`'s `Asset.from_dict` would produce from an untrusted body:
/// every field present, the same shallow checks, the same refusals. Anything else in the
/// body is dropped, as it is there.
pub fn normalize(body: &Json) -> Result<Json, String> {
    let obj = body.as_object().ok_or("an asset must be an object")?;
    let text = |k: &str, limit: usize| -> Result<String, String> {
        match obj.get(k) {
            None | Some(Json::Null) => Ok(String::new()),
            Some(Json::String(s)) => Ok(s.trim().chars().take(limit).collect()),
            Some(_) => Err(format!("{k} must be a string")),
        }
    };
    let origin = {
        let o = text("origin", 200)?;
        if o.is_empty() {
            "authored".to_string()
        } else {
            o
        }
    };
    if origin != "authored" && origin != "ingested" {
        return Err(format!(
            "origin must be one of ['ingested', 'authored'], got '{origin}'"
        ));
    }
    let kind = text("kind", 200)?;
    let kind_ok = kind.is_empty()
        || (kind.len() <= 64
            && kind.as_bytes()[0].is_ascii_lowercase()
            && kind
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c)));
    if !kind_ok {
        return Err(format!(
            "kind '{kind}' must match ^[a-z][a-z0-9_.-]{{0,63}}$"
        ));
    }
    let mut tags: Vec<String> = Vec::new();
    match obj.get("tags") {
        None | Some(Json::Null) => {}
        Some(Json::Array(a)) => {
            for t in a.iter().take(64) {
                let t = t.as_str().ok_or("tags must be a list of strings")?.trim();
                let t: String = t.chars().take(64).collect();
                if !t.is_empty() && !tags.contains(&t) {
                    tags.push(t);
                }
            }
        }
        Some(_) => return Err("tags must be a list of strings".into()),
    }
    let sockets = |v: Option<&Json>, what: &str| -> Result<Json, String> {
        let list = match v {
            None | Some(Json::Null) => return Ok(json!([])),
            Some(Json::Array(a)) => a,
            Some(_) => return Err(format!("{what} must be a list of socket objects")),
        };
        if list.len() > 64 {
            return Err(format!("{what} may not declare more than 64 sockets"));
        }
        let mut out = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for e in list {
            let o = e
                .as_object()
                .ok_or(format!("{what} entries must be objects"))?;
            let name = o.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let ident = !name.is_empty()
                && !name.as_bytes()[0].is_ascii_digit()
                && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_');
            if !ident {
                return Err(format!("{what} socket name '{name}' must be an identifier"));
            }
            if seen.iter().any(|s| s == name) {
                return Err(format!("{what} socket '{name}' is declared twice"));
            }
            seen.push(name.to_string());
            let ty = o
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if ty.is_empty() {
                return Err(format!(
                    "{what} socket '{name}' needs a non-empty 'type' string"
                ));
            }
            let mut sock = o.clone();
            sock.insert("name".into(), json!(name.trim()));
            sock.insert("type".into(), json!(ty));
            out.push(Json::Object(sock));
        }
        Ok(Json::Array(out))
    };
    let interface = match obj.get("interface") {
        None | Some(Json::Null) => json!({"inputs": [], "outputs": []}),
        Some(Json::Object(i)) => json!({
            "inputs": sockets(i.get("inputs"), "interface.inputs")?,
            "outputs": sockets(i.get("outputs"), "interface.outputs")?,
        }),
        Some(_) => return Err("interface must be an object with 'inputs' and 'outputs'".into()),
    };
    let properties = match obj.get("properties") {
        None | Some(Json::Null) => json!({}),
        Some(Json::Object(p)) => {
            if p.len() > 100 {
                return Err("properties may not exceed 100 keys".into());
            }
            for (k, v) in p {
                match v {
                    Json::String(s) if s.chars().count() > 4096 => {
                        return Err(format!("property '{k}' is longer than 4096 characters"))
                    }
                    Json::Array(_) | Json::Object(_) => {
                        return Err(format!(
                            "property '{k}' must be a string, number, boolean or null"
                        ))
                    }
                    _ => {}
                }
            }
            Json::Object(p.clone())
        }
        Some(_) => return Err("properties must be an object".into()),
    };
    let created_at = match obj.get("created_at") {
        None | Some(Json::Null) => 0.0,
        Some(Json::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(_) => return Err("created_at must be a number".into()),
    };
    let api_version = match obj.get("api_version") {
        None | Some(Json::Null) => 1,
        Some(Json::Number(n)) if n.is_i64() => n.as_i64().unwrap_or(1),
        Some(_) => return Err("api_version must be an integer".into()),
    };
    let id = text("id", 64)?;
    let name = {
        let n = text("name", 200)?;
        if n.is_empty() {
            id.clone()
        } else {
            n
        }
    };
    Ok(json!({
        "id": id,
        "name": name,
        "owner": text("owner", 64)?,
        "origin": origin,
        "kind": kind,
        "captured_against": text("captured_against", 64)?,
        "tags": tags,
        "interface": interface,
        "properties": properties,
        "payload": obj.get("payload").cloned().unwrap_or(Json::Null),
        "created_at": created_at,
        "api_version": api_version,
    }))
}

/// `POST api/assets`: the generic create every asset owner answers. The body is an
/// asset object; the owner is forced, the id is made unique. A record created this way
/// carries no model files: models arrive through `import_model`.
pub fn create(req: &Request, ctx: &ModuleContext) -> Response {
    let body: Json = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => return Response::error(400, &format!("the body is not JSON: {e}")),
    };
    let mut asset = match normalize(&body) {
        Ok(a) => a,
        Err(e) => return Response::error(400, &e),
    };
    let base = {
        let id = asset["id"].as_str().unwrap_or("");
        let name = asset["name"].as_str().unwrap_or("");
        if !id.is_empty() {
            id.to_string()
        } else if !name.is_empty() {
            name.to_string()
        } else {
            "asset".into()
        }
    };
    let id = new_id(ctx, &base);
    asset["id"] = json!(id);
    asset["owner"] = json!(ctx.module_id);
    if asset["name"].as_str().unwrap_or("").is_empty() {
        asset["name"] = json!(id);
    }
    if asset["created_at"].as_f64().unwrap_or(0.0) == 0.0 {
        asset["created_at"] = json!(now());
    }
    if let Err(e) = write_stored(ctx, &asset) {
        return Response::error(500, &e);
    }
    ctx.log(
        "info",
        "assets.created",
        &format!("created asset {id}"),
        &[("asset".to_string(), id.as_str().into())],
    );
    json_response(201, &json!({"asset": asset}))
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

// ---------------------------------------------------------------- the two tools

fn tool_ok(outputs: Json) -> Response {
    json_response(200, &json!({"outputs": outputs}))
}

/// `POST tools/import_model`: copy a `.param`/`.bin` pair into the module's data dir and
/// publish it as an `ncnn_model` asset with the license the caller states.
pub fn import_model(req: &Request, ctx: &ModuleContext) -> Response {
    let call = match Call::parse(req) {
        Ok(c) => c,
        Err(e) => return Response::error(400, &e),
    };
    match import(&call, ctx) {
        Ok(v) => tool_ok(v),
        Err((s, m)) => Response::error(s, &m),
    }
}

fn import(call: &Call, ctx: &ModuleContext) -> Result<Json, (u16, String)> {
    let bad = |m: String| (400u16, m);
    let raw = call.str("param");
    let param = resolve::resolve(&call.base, &raw, "param").map_err(bad)?;
    if !param
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("param"))
    {
        return Err(bad(format!(
            "{}. param must name a .param file",
            resolve::treatment(&call.base, &raw, "param")
        )));
    }
    if !param.is_file() {
        return Err(bad(format!(
            "{}, and no file is there",
            resolve::treatment(&call.base, &raw, "param")
        )));
    }
    let bin = param.with_extension("bin");
    if !bin.is_file() {
        return Err(bad(format!(
            "param has no .bin beside it: expected \"{}\"",
            bin.display()
        )));
    }
    let model_name = param
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model")
        .to_string();
    let scale = call.int("model_scale", 0).map_err(bad)?;
    if !(0..=4).contains(&scale) || scale == 1 {
        return Err(bad(format!(
            "model_scale must be 0 (read it from the name), 2, 3 or 4, got {scale}"
        )));
    }
    let native = if scale > 0 {
        scale as u32
    } else {
        scale_from_name(&model_name)
    };
    let license = {
        let l = call.str("license");
        if l.is_empty() {
            "unknown".to_string()
        } else {
            l
        }
    };
    let commercial = match call.str("commercial_use").to_ascii_lowercase().as_str() {
        "" | "unknown" => "unknown".to_string(),
        c @ ("allowed" | "forbidden") => c.to_string(),
        c => {
            return Err(bad(format!(
                "commercial_use must be allowed, forbidden or unknown, got \"{c}\""
            )))
        }
    };
    let display = {
        let n = call.str("name");
        if n.is_empty() {
            model_name.clone()
        } else {
            n
        }
    };
    let id = new_id(ctx, &display);
    let dir = imported_models_dir(ctx, &id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| (500, format!("cannot create {}: {e}", dir.display())))?;
    for (src, ext) in [(&param, "param"), (&bin, "bin")] {
        let dst = dir.join(format!("{model_name}.{ext}"));
        std::fs::copy(src, &dst).map_err(|e| {
            (
                500,
                format!("cannot copy {} to {}: {e}", src.display(), dst.display()),
            )
        })?;
    }
    let token = format!("{TOKEN_PREFIX}imported/{id}");
    let mut props = Map::new();
    for (k, v) in [
        ("model_name", json!(model_name)),
        ("native_scale", json!(native)),
        ("license", json!(license)),
        ("license_url", json!(call.str("license_url"))),
        ("source_url", json!(call.str("source_url"))),
        ("author", json!(call.str("author"))),
        ("commercial_use", json!(commercial)),
        ("attribution", json!(call.str("attribution"))),
        ("imported_from", json!(param.display().to_string())),
    ] {
        props.insert(k.to_string(), v);
    }
    let asset = json!({
        "id": id,
        "name": display,
        "owner": ctx.module_id,
        "origin": "authored",
        "kind": KIND,
        "captured_against": version(),
        "tags": ["upscale", "ncnn", "imported"],
        "interface": interface(),
        "properties": Json::Object(props),
        "payload": {"outputs": {"model": token}, "model_name": model_name, "native_scale": native},
        "created_at": now(),
        "api_version": 1,
    });
    write_stored(ctx, &asset).map_err(|e| (500, e))?;
    ctx.log(
        "info",
        "assets.created",
        &format!("imported model {display} as {token}"),
        &[
            ("asset".to_string(), id.as_str().into()),
            ("license".to_string(), license.as_str().into()),
            ("model".to_string(), token.as_str().into()),
        ],
    );
    Ok(json!({"model": token, "asset_id": id, "name": display, "license": license}))
}

/// `POST tools/list_models`: every model this module can run, bundled and imported.
pub fn list_models(_req: &Request, ctx: &ModuleContext) -> Response {
    let mut all = match bundled_all(ctx) {
        Ok(b) => b,
        Err(e) => return Response::error(500, &e),
    };
    for id in stored_ids(ctx) {
        if let Ok(Some(a)) = read_stored(ctx, &id) {
            if a["kind"] == KIND {
                all.push(a);
            }
        }
    }
    let col = |f: &dyn Fn(&Json) -> Json| Json::Array(all.iter().map(f).collect());
    tool_ok(json!({
        "models": col(&|a| a["payload"]["outputs"]["model"].clone()),
        "names": col(&|a| a["name"].clone()),
        "licenses": col(&|a| a["properties"]["license"].clone()),
        "commercial_use": col(&|a| a["properties"]["commercial_use"].clone()),
        "count": all.len(),
    }))
}

/// For `models::resolve_model`: an imported token's files and record.
pub fn imported(ctx: &ModuleContext, id: &str) -> Result<(PathBuf, String, u32, String), String> {
    if !valid_id(id) {
        return Err(format!("'{id}' is not a valid asset id"));
    }
    let a = read_stored(ctx, id)?
        .ok_or_else(|| format!("no imported model '{id}' (it may have been deleted)"))?;
    let name = a["payload"]["model_name"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let scale = a["payload"]["native_scale"].as_u64().unwrap_or(4) as u32;
    let license = a["properties"]["license"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    Ok((imported_models_dir(ctx, id), name, scale, license))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_slugs_follow_assets_py() {
        assert!(valid_id("bundled-ultrasharp-4x"));
        assert!(valid_id("a"));
        assert!(!valid_id("-a"));
        assert!(!valid_id("A"));
        assert!(!valid_id(""));
        assert_eq!(slug("My Model (2x)!"), "my-model-2x");
        assert_eq!(slug("  "), "asset");
        // assets.py transliterates (NFKD); this drops non-ASCII instead. Only this
        // module's own ids come from here, so the difference never crosses a boundary.
        assert_eq!(slug("ÜberSharp"), "bersharp");
    }
}
