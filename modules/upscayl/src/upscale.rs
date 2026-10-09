//! The `upscale_image` tool: one image, synchronous.
//!
//! Inputs are the sockets declared in `module.json`. Every refusal names the socket and
//! says what to do, because the broker carries it to a person reading a failed node.

use crate::args::{self, Plan, Size};
use crate::engine::{self, Failure};
use crate::models;
use crate::paths;
use crate::resolve::{self, Base};
use hollowdeck_module::{json::Value, ModuleContext, Request, Response};
use serde_json::Value as Json;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a synchronous call may run before it is stopped. Below the Orchestrator's
/// default 300 s tool timeout, so this module answers with a sentence instead of the
/// caller timing out with none. `UPSCAYL_TOOL_BUDGET_SECS` overrides it.
pub fn budget() -> Duration {
    let secs = std::env::var("UPSCAYL_TOOL_BUDGET_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(280);
    Duration::from_secs(secs)
}

/// The body a tool is sent: `{"inputs": {...}, "base_dir": ..., "base_dir_source": ...}`.
pub struct Call {
    pub inputs: Json,
    pub base: Base,
}

impl Call {
    pub fn parse(req: &Request) -> Result<Call, String> {
        let body: Json = serde_json::from_slice(&req.body)
            .map_err(|e| format!("the request body is not JSON: {e}"))?;
        let s = |k: &str| body[k].as_str().unwrap_or("").to_string();
        Ok(Call {
            inputs: body["inputs"].clone(),
            base: Base::of(&s("base_dir"), &s("base_dir_source")),
        })
    }

    pub fn str(&self, k: &str) -> String {
        match &self.inputs[k] {
            Json::String(s) => s.trim().to_string(),
            Json::Number(n) => n.to_string(),
            _ => String::new(),
        }
    }

    /// An integer socket. Null or absent is `default`; a numeric string is accepted,
    /// because a value wired from a text node arrives as one.
    pub fn int(&self, k: &str, default: i64) -> Result<i64, String> {
        match &self.inputs[k] {
            Json::Null => Ok(default),
            Json::Number(n) => n
                .as_i64()
                .or_else(|| n.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64))
                .ok_or_else(|| format!("{k} must be a whole number, got {n}")),
            Json::String(s) if s.trim().is_empty() => Ok(default),
            Json::String(s) => s
                .trim()
                .parse()
                .map_err(|_| format!("{k} must be a whole number, got \"{s}\"")),
            other => Err(format!("{k} must be a whole number, got {other}")),
        }
    }

    pub fn bool(&self, k: &str) -> Result<bool, String> {
        match &self.inputs[k] {
            Json::Null => Ok(false),
            Json::Bool(b) => Ok(*b),
            Json::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                "" | "false" | "0" | "no" => Ok(false),
                "true" | "1" | "yes" => Ok(true),
                _ => Err(format!("{k} must be true or false, got \"{s}\"")),
            },
            other => Err(format!("{k} must be true or false, got {other}")),
        }
    }
}

/// Everything decided about one request except the model, which needs the module.
#[derive(Debug, PartialEq, Eq)]
pub struct Settings {
    pub size: Size,
    pub filter: Option<String>,
    pub format: String,
    pub compression: u32,
    pub gpu_id: Option<String>,
    pub tile_size: Option<String>,
    pub threads: Option<String>,
    pub tta: bool,
    pub overwrite: bool,
}

fn digits_list(s: &str) -> bool {
    !s.is_empty()
        && s.split(',')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

pub fn settings(call: &Call) -> Result<Settings, String> {
    let scale = call.int("scale", 4)?;
    if !(1..=16).contains(&scale) {
        return Err(format!("scale must be 1 to 16, got {scale}"));
    }
    let width = call.int("width", 0)?;
    if width < 0 {
        return Err(format!(
            "width must be 0 (unset) or a width in pixels, got {width}"
        ));
    }
    let resize = call.str("resize").to_ascii_lowercase();
    if width > 0 && !resize.is_empty() {
        return Err(
            "width and resize are both set; set one. The engine honours neither when both \
             are sent"
                .into(),
        );
    }
    let size = if width > 0 {
        Size::Width(width as u32)
    } else if !resize.is_empty() {
        let parsed = resize
            .split_once('x')
            .and_then(|(w, h)| Some((w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?)))
            .filter(|(w, h)| *w > 0 && *h > 0);
        match parsed {
            Some((w, h)) => Size::Resize(w, h),
            None => {
                return Err(format!(
                    "resize must be WIDTHxHEIGHT in pixels, like 1920x1080, got \"{resize}\""
                ))
            }
        }
    } else {
        Size::Scale(scale as u32)
    };
    let filter = match call.str("resize_filter").to_ascii_lowercase().as_str() {
        "" => None,
        f if args::FILTERS.contains(&f) => Some(f.to_string()),
        f => {
            return Err(format!(
                "resize_filter must be one of {}, got \"{f}\"",
                args::FILTERS.join(", ")
            ))
        }
    };
    let format = match call.str("format").to_ascii_lowercase().as_str() {
        "" => "png".to_string(),
        "jpeg" => "jpg".to_string(),
        f if args::FORMATS.contains(&f) => f.to_string(),
        f => return Err(format!("format must be png, jpg or webp, got \"{f}\"")),
    };
    let compression = call.int("compression", 0)?;
    if !(0..=100).contains(&compression) {
        return Err(format!("compression must be 0 to 100, got {compression}"));
    }
    let gpu = call.str("gpu_id");
    let gpu_id = match gpu.to_ascii_lowercase().as_str() {
        "" | "auto" => None,
        g if digits_list(g) => Some(g.to_string()),
        g => {
            return Err(format!(
                "gpu_id must be a GPU number like 0, or 0,1 for several, got \"{g}\""
            ))
        }
    };
    let tile = call.str("tile_size");
    let tile_size = if tile.is_empty() || tile == "0" {
        None
    } else if digits_list(&tile)
        && tile
            .split(',')
            .all(|t| t == "0" || t.parse::<u32>().is_ok_and(|v| v >= 32))
    {
        Some(tile)
    } else {
        return Err(format!(
            "tile_size must be 0 (automatic) or 32 and up, one per GPU like 0,0, got \"{tile}\""
        ));
    };
    let threads_raw = call.str("threads");
    let threads = if threads_raw.is_empty() {
        None
    } else {
        let parts: Vec<&str> = threads_raw.split(':').collect();
        if parts.len() == 3 && parts.iter().all(|p| digits_list(p)) {
            Some(threads_raw)
        } else {
            return Err(format!(
                "threads must be load:proc:save, like 1:2:2, got \"{threads_raw}\""
            ));
        }
    };
    Ok(Settings {
        size,
        filter,
        format,
        compression: compression as u32,
        gpu_id,
        tile_size,
        threads,
        tta: call.bool("tta")?,
        overwrite: call.bool("overwrite")?,
    })
}

/// Where the output goes: next to the input when `output` is empty, inside it when it is
/// a folder, or exactly there when it is a file whose extension matches `format` (the
/// extension decides the engine's format, finding 7).
pub fn output_path(
    base: &Base,
    input: &Path,
    raw_output: &str,
    auto_name: &str,
    format: &str,
) -> Result<PathBuf, String> {
    let raw = raw_output.trim();
    if raw.is_empty() {
        let dir = input.parent().unwrap_or(Path::new("."));
        return Ok(dir.join(auto_name));
    }
    let resolved = resolve::resolve(base, raw, "output")?;
    let looks_like_dir = raw.ends_with('/') || raw.ends_with('\\') || resolved.is_dir();
    if looks_like_dir {
        return Ok(resolved.join(auto_name));
    }
    let ext = resolved
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = if ext == "jpeg" {
        "jpg".to_string()
    } else {
        ext
    };
    if ext != format {
        return Err(format!(
            "output ends in .{ext} but format is {format}. The engine writes the format the \
             extension names, so the two must agree"
        ));
    }
    Ok(resolved)
}

/// `POST tools/upscale_image`.
pub fn handle(req: &Request, ctx: &ModuleContext) -> Response {
    let call = match Call::parse(req) {
        Ok(c) => c,
        Err(e) => return Response::error(400, &e),
    };
    match run(&call, ctx) {
        Ok(v) => Response::json(200, &Value::object().with("outputs", v)),
        Err((status, msg)) => Response::error(status, &msg),
    }
}

fn run(call: &Call, ctx: &ModuleContext) -> Result<Value, (u16, String)> {
    let bad = |m: String| (400u16, m);
    let s = settings(call).map_err(bad)?;
    let raw_input = call.str("input");
    let input = resolve::resolve(&call.base, &raw_input, "input").map_err(bad)?;
    if !input.is_file() {
        return Err(bad(format!(
            "{}, and no file is there",
            resolve::treatment(&call.base, &raw_input, "input")
        )));
    }
    let model_scale = call.int("model_scale", 0).map_err(bad)?;
    if !(0..=4).contains(&model_scale) || model_scale == 1 {
        return Err(bad(format!(
            "model_scale must be 0 (read it from the model name), 2, 3 or 4, got {model_scale}"
        )));
    }
    let model = models::resolve_model(
        ctx,
        &call.base,
        &call.str("model"),
        &call.str("custom_model"),
        model_scale as u32,
    )
    .map_err(bad)?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    let auto = args::output_name(stem, &s.size, &model.name, &s.format);
    let output =
        output_path(&call.base, &input, &call.str("output"), &auto, &s.format).map_err(bad)?;
    if output.exists() && !s.overwrite {
        return Err((
            409,
            format!(
                "output \"{}\" already exists and overwrite is false",
                output.display()
            ),
        ));
    }
    let exe = paths::engine_binary(ctx);
    if !exe.is_file() {
        return Err((
            503,
            format!(
                "the engine is not staged ({} is missing). Run scripts/sync-engine.ps1",
                paths::ENGINE_REL
            ),
        ));
    }
    let plan = Plan {
        input: input.clone(),
        output: output.clone(),
        models_dir: model.dir.clone(),
        model_name: model.name.clone(),
        native_scale: model.native_scale,
        size: s.size.clone(),
        filter: s.filter.clone(),
        format: s.format.clone(),
        compression: s.compression,
        gpu_id: s.gpu_id.clone(),
        tile_size: s.tile_size.clone(),
        threads: s.threads.clone(),
        tta: s.tta,
    };
    let argv = args::engine_args(&plan);
    let result = engine::run(&exe, &argv, &output, budget(), &|_| {});
    let (ok, report, failure) = match result {
        Ok(r) => (true, r, None),
        Err((f, r)) => (false, r, Some(f)),
    };
    ctx.log(
        if ok { "info" } else { "warning" },
        if ok {
            "upscayl.upscaled"
        } else {
            "upscayl.failed"
        },
        &format!(
            "{} {} with {}",
            if ok { "upscaled" } else { "failed to upscale" },
            input.display(),
            model.label
        ),
        &[
            ("model".to_string(), model.label.as_str().into()),
            ("license".to_string(), model.license.as_str().into()),
            ("input".to_string(), input.display().to_string().into()),
            ("output".to_string(), output.display().to_string().into()),
            ("args".to_string(), argv.join(" ").into()),
            (
                "elapsed_ms".to_string(),
                (report.elapsed.as_millis() as i64).into(),
            ),
            ("progress".to_string(), f64::from(report.progress).into()),
            (
                "exit_code".to_string(),
                report.exit_code.map_or(Value::Null, |c| c.into()),
            ),
        ],
    );
    if let Some(f) = failure {
        return Err(match f {
            Failure::Engine(e) => (500, format!("the engine refused: {e}")),
            Failure::Budget(b) => (
                504,
                format!(
                    "stopped after {} s, this tool's limit for one call. Run long work with \
                     start_job instead",
                    b.as_secs()
                ),
            ),
            Failure::Spawn(e) => (500, e),
            Failure::NoOutput => (
                500,
                format!(
                    "the engine finished without writing {}. Last lines: {}",
                    output.display(),
                    report.stderr_tail.join(" / ")
                ),
            ),
        });
    }
    let (w, h) = imagesize::size(&output)
        .map(|d| (d.width as i64, d.height as i64))
        .map_err(|e| {
            (
                500,
                format!("wrote {} but cannot read its size: {e}", output.display()),
            )
        })?;
    Ok(Value::object()
        .with("output", output.display().to_string().into())
        .with("width", w.into())
        .with("height", h.into())
        .with("elapsed_ms", (report.elapsed.as_millis() as i64).into())
        .with("model", model.label.as_str().into())
        .with("license", model.license.as_str().into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(inputs: serde_json::Value) -> Call {
        Call {
            inputs,
            base: Base::of(if cfg!(windows) { "C:\\base" } else { "/base" }, "project"),
        }
    }

    #[test]
    fn defaults_are_scale_4_png() {
        let s = settings(&call(serde_json::json!({}))).unwrap();
        assert_eq!(s.size, Size::Scale(4));
        assert_eq!(s.format, "png");
        assert_eq!(s.compression, 0);
        assert!(!s.tta && !s.overwrite);
    }

    #[test]
    fn width_and_resize_together_are_refused() {
        let e = settings(&call(
            serde_json::json!({"width": 1000, "resize": "800x600"}),
        ))
        .unwrap_err();
        assert!(e.contains("both set"), "{e}");
    }

    #[test]
    fn width_and_resize_replace_scale() {
        let s = settings(&call(serde_json::json!({"scale": 2, "width": 1000}))).unwrap();
        assert_eq!(s.size, Size::Width(1000));
        let s = settings(&call(serde_json::json!({"resize": "1920X1080"}))).unwrap();
        assert_eq!(s.size, Size::Resize(1920, 1080));
    }

    #[test]
    fn out_of_range_values_are_refused_with_the_socket_named() {
        for (inputs, socket) in [
            (serde_json::json!({"scale": 17}), "scale"),
            (serde_json::json!({"scale": 0}), "scale"),
            (serde_json::json!({"compression": 101}), "compression"),
            (serde_json::json!({"format": "gif"}), "format"),
            (serde_json::json!({"resize": "big"}), "resize"),
            (
                serde_json::json!({"resize_filter": "lanczos"}),
                "resize_filter",
            ),
            (serde_json::json!({"gpu_id": "first"}), "gpu_id"),
            (serde_json::json!({"tile_size": "16"}), "tile_size"),
            (serde_json::json!({"threads": "2:4"}), "threads"),
            (serde_json::json!({"tta": "maybe"}), "tta"),
        ] {
            let e = settings(&call(inputs.clone())).unwrap_err();
            assert!(e.starts_with(socket), "{inputs} -> {e}");
        }
    }

    #[test]
    fn wired_strings_are_accepted_for_numbers_and_booleans() {
        let s = settings(&call(serde_json::json!({"scale": "2", "tta": "true", "gpu_id": "auto", "tile_size": "0,256", "format": "JPEG"}))).unwrap();
        assert_eq!(s.size, Size::Scale(2));
        assert!(s.tta);
        assert_eq!(s.gpu_id, None);
        assert_eq!(s.tile_size.as_deref(), Some("0,256"));
        assert_eq!(s.format, "jpg");
    }

    #[test]
    fn output_defaults_beside_the_input_and_a_mismatched_extension_is_refused() {
        let b = call(serde_json::json!({})).base;
        let input = if cfg!(windows) {
            PathBuf::from("C:\\pics\\a.jpg")
        } else {
            PathBuf::from("/pics/a.jpg")
        };
        let o = output_path(&b, &input, "", "a_upscayl_4x_m.png", "png").unwrap();
        assert_eq!(o, input.parent().unwrap().join("a_upscayl_4x_m.png"));
        let e = output_path(&b, &input, "out.jpg", "x.png", "png").unwrap_err();
        assert!(e.contains(".jpg") && e.contains("png"), "{e}");
        let o = output_path(&b, &input, "out.jpeg", "x.jpg", "jpg").unwrap();
        assert!(o.ends_with("out.jpeg"));
        let o = output_path(&b, &input, "outdir/", "x.png", "png").unwrap();
        assert!(o.ends_with("x.png"));
    }

    /// End to end against the staged engine: skipped when the sync script has not run.
    mod engine_runs {
        use super::*;
        use hollowdeck_module::Headers;

        fn ctx() -> Option<ModuleContext> {
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            if !dir.join(paths::ENGINE_REL).is_file() {
                eprintln!("skipped: engine not staged; run scripts/sync-engine.ps1");
                return None;
            }
            Some(ModuleContext {
                module_id: "upscayl".into(),
                module_dir: dir.clone(),
                mount_path: "/m/upscayl".into(),
                shared_data_dir: dir.join("target/test-data"),
                data_dir: dir.join("target/test-data/module_data/upscayl"),
                core_version: None,
                core_bind: "127.0.0.1".into(),
                core_url: None,
                secret: None,
                port: 1,
            })
        }

        fn sample() -> String {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../to_upscale.jpeg")
                .display()
                .to_string()
        }

        fn out_dir(name: &str) -> PathBuf {
            let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/test-out")
                .join(name);
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            d
        }

        fn post(inputs: serde_json::Value) -> Request {
            let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .display()
                .to_string();
            let body = serde_json::json!({"inputs": inputs, "base_dir": base, "base_dir_source": "project"});
            Request {
                method: "POST".into(),
                path: "/tools/upscale_image".into(),
                query: String::new(),
                headers: Headers::new(),
                body: body.to_string().into_bytes(),
            }
        }

        fn outputs(r: &Response) -> serde_json::Value {
            let v: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            v["outputs"].clone()
        }

        #[test]
        fn a_4x_model_at_scale_2_writes_a_512_png_beside_nothing_else() {
            let Some(ctx) = ctx() else { return };
            let dir = out_dir("scale2");
            let r = handle(
                &post(
                    serde_json::json!({"input": sample(), "output": format!("{}/", dir.display()), "model": "upscayl-lite-4x", "scale": 2}),
                ),
                &ctx,
            );
            assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
            let o = outputs(&r);
            assert_eq!(
                (o["width"].as_i64(), o["height"].as_i64()),
                (Some(512), Some(512))
            );
            assert!(o["output"]
                .as_str()
                .unwrap()
                .ends_with("to_upscale_upscayl_2x_upscayl-lite-4x.png"));
            assert_eq!(o["license"], "BSD-3-Clause");
        }

        #[test]
        fn a_2x_model_is_run_with_its_own_scale() {
            let Some(ctx) = ctx() else { return };
            let dir = out_dir("x2");
            let out = dir.join("x2.webp");
            let r = handle(
                &post(
                    serde_json::json!({"input": sample(), "output": out.display().to_string(), "model": "realesr-animevideov3-x2", "scale": 2, "format": "webp", "compression": 50}),
                ),
                &ctx,
            );
            assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
            assert_eq!(outputs(&r)["width"].as_i64(), Some(512));
        }

        #[test]
        fn width_and_resize_produce_their_sizes() {
            let Some(ctx) = ctx() else { return };
            let dir = out_dir("size");
            let r = handle(
                &post(
                    serde_json::json!({"input": sample(), "output": format!("{}/", dir.display()), "model": "upscayl-lite-4x", "width": 1000}),
                ),
                &ctx,
            );
            assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
            assert_eq!(outputs(&r)["width"].as_i64(), Some(1000));
            let r = handle(
                &post(
                    serde_json::json!({"input": sample(), "output": format!("{}/", dir.display()), "model": "upscayl-lite-4x", "resize": "800x600", "resize_filter": "catmullrom"}),
                ),
                &ctx,
            );
            assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
            let o = outputs(&r);
            assert_eq!(
                (o["width"].as_i64(), o["height"].as_i64()),
                (Some(800), Some(600))
            );
        }

        #[test]
        fn an_existing_output_is_refused_unless_overwrite() {
            let Some(ctx) = ctx() else { return };
            let dir = out_dir("exists");
            let out = dir.join("taken.png");
            std::fs::write(&out, b"not an image").unwrap();
            let inputs = |ow: bool| serde_json::json!({"input": sample(), "output": out.display().to_string(), "model": "upscayl-lite-4x", "overwrite": ow});
            let r = handle(&post(inputs(false)), &ctx);
            assert_eq!(r.status, 409, "{}", String::from_utf8_lossy(&r.body));
            let r = handle(&post(inputs(true)), &ctx);
            assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
        }

        #[test]
        fn bad_inputs_fail_with_a_sentence_naming_the_cause() {
            let Some(ctx) = ctx() else { return };
            let dir = out_dir("bad");
            for (inputs, status, needle) in [
                (
                    serde_json::json!({"input": sample(), "output": format!("{}/", dir.display()), "model": "no-such-model"}),
                    400,
                    "not a bundled model",
                ),
                (
                    serde_json::json!({"input": "missing.jpg", "model": "upscayl-lite-4x"}),
                    400,
                    "no file is there",
                ),
                (
                    serde_json::json!({"input": sample(), "custom_model": "nowhere/x.param"}),
                    400,
                    "no file is there",
                ),
                (
                    serde_json::json!({"input": sample(), "custom_model": "licenses/models.json"}),
                    400,
                    ".param",
                ),
            ] {
                let r = handle(&post(inputs.clone()), &ctx);
                let body = String::from_utf8_lossy(&r.body).to_string();
                assert_eq!(r.status, status, "{inputs} -> {body}");
                assert!(body.contains(needle), "{inputs} -> {body}");
            }
        }

        #[test]
        fn a_corrupt_input_is_a_failure_even_though_the_engine_exits_0() {
            let Some(ctx) = ctx() else { return };
            let dir = out_dir("corrupt");
            let bad = dir.join("corrupt.jpg");
            std::fs::write(&bad, b"\xff\xd8\xff\xe0 not a jpeg").unwrap();
            let r = handle(
                &post(
                    serde_json::json!({"input": bad.display().to_string(), "model": "upscayl-lite-4x"}),
                ),
                &ctx,
            );
            let body = String::from_utf8_lossy(&r.body).to_string();
            assert_eq!(r.status, 500, "{body}");
            assert!(body.contains("Couldn't read the image"), "{body}");
        }
    }
}
