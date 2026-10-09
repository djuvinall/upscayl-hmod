//! The `upscale_batch` tool: every image in one folder, synchronous.
//!
//! Upstream's batch: the engine's directory mode, into a subfolder named
//! `upscayl_<format>_<model>_<size>` inside the output folder, files keeping their stems.
//! Directory mode loads the model once and processes files in parallel, so it is used
//! whenever it can be; per-file runs take over for double upscayl, and when some targets
//! already exist and `overwrite` is off (directory mode cannot skip a file).

use crate::args::{self, Plan, Size};
use crate::engine::{self, Failure};
use crate::resolve;
use crate::upscale::{self, Call};
use hollowdeck_module::{json::Value, ModuleContext, Request, Response};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const IMAGE_EXTS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// The images in a folder, sorted, not recursive.
pub fn images_in(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot list \"{}\": {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .collect();
    found.sort();
    Ok(found)
}

/// Upstream's output subfolder name.
pub fn folder_name(format: &str, model: &str, size: &Size) -> String {
    format!("upscayl_{format}_{model}_{}", args::size_label(size))
}

/// Two inputs that would write the same output (`a.jpg` and `a.png`).
pub fn stem_clashes(files: &[PathBuf]) -> Vec<String> {
    let mut by_stem: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in files {
        let stem = f
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let name = f
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        by_stem.entry(stem).or_default().push(name);
    }
    by_stem
        .into_values()
        .filter(|v| v.len() > 1)
        .map(|v| v.join(" and "))
        .collect()
}

/// `POST tools/upscale_batch`.
pub fn handle(req: &Request, ctx: &ModuleContext) -> Response {
    let call = match Call::parse_for(req, ctx, "upscale_batch") {
        Ok(c) => c,
        Err((s, e)) => return Response::error(s, &e),
    };
    match run(&call, ctx, upscale::budget(), &|_| {}, &|| false) {
        Ok(v) => Response::json(200, &Value::object().with("outputs", v)),
        Err((status, msg)) => Response::error(status, &msg),
    }
}

/// The whole batch. `on_progress` gets 0 to 100.
pub fn run(
    call: &Call,
    ctx: &ModuleContext,
    budget: Duration,
    on_progress: &dyn Fn(f32),
    should_stop: &dyn Fn() -> bool,
) -> Result<Value, (u16, String)> {
    let started = Instant::now();
    let bad = |m: String| (400u16, m);
    let raw_in = call.str("input_folder");
    let in_dir = resolve::resolve(&call.base, &raw_in, "input_folder").map_err(bad)?;
    if !in_dir.is_dir() {
        return Err(bad(format!(
            "{}, and no folder is there",
            resolve::treatment(&call.base, &raw_in, "input_folder")
        )));
    }
    let files = images_in(&in_dir).map_err(bad)?;
    if files.is_empty() {
        return Err(bad(format!(
            "input_folder \"{}\" has no PNG, JPEG or WebP images",
            in_dir.display()
        )));
    }
    let clashes = stem_clashes(&files);
    if !clashes.is_empty() {
        return Err(bad(format!(
            "these inputs would write the same output file: {}. Rename one of each",
            clashes.join("; ")
        )));
    }
    // Plan the first file fully: that validates every socket, the model, the engine and
    // exiftool once, with the same refusals upscale_image gives.
    let s = upscale::settings(call).map_err(bad)?;
    let raw_out = call.str("output_folder");
    let parent = if raw_out.is_empty() {
        in_dir.clone()
    } else {
        resolve::resolve(&call.base, &raw_out, "output_folder").map_err(bad)?
    };
    let probe = upscale::prepare_with(
        call,
        ctx,
        &files[0].display().to_string(),
        Some(parent.join("probe")),
    )
    .map_err(|(st, m)| (st, m.replace("output \"", "output_folder \"")))?;
    let label_size = match (&s.size, probe.double) {
        (Size::Scale(x), true) => Size::Scale(x * x),
        (other, _) => other.clone(),
    };
    let out_dir = parent.join(folder_name(&s.format, &probe.model.name, &label_size));
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| (500, format!("cannot create \"{}\": {e}", out_dir.display())))?;
    let target = |f: &Path| -> PathBuf {
        let stem = f.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        out_dir.join(format!("{stem}.{}", s.format))
    };

    let mut outputs: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let any_exists = files.iter().any(|f| target(f).exists());
    let per_file = probe.double || (any_exists && !s.overwrite);

    if per_file {
        let n = files.len() as f32;
        for (i, f) in files.iter().enumerate() {
            let name = f
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            let remaining = budget.saturating_sub(started.elapsed());
            if should_stop() {
                failed.push(format!("{name}: not processed, the job was cancelled"));
                continue;
            }
            if remaining.is_zero() {
                failed.push(format!(
                    "{name}: not processed, the call's budget ran out; run this batch with start_job"
                ));
                continue;
            }
            let job =
                match upscale::prepare_with(call, ctx, &f.display().to_string(), Some(target(f))) {
                    Ok(j) => j,
                    Err((_, m)) => {
                        failed.push(format!("{name}: {m}"));
                        continue;
                    }
                };
            let offset = i as f32;
            match upscale::execute(
                &job,
                ctx,
                remaining,
                &|p| on_progress((offset + p / 100.0) / n * 100.0),
                should_stop,
            ) {
                Ok(_) => outputs.push(target(f).display().to_string()),
                Err((_, m)) => failed.push(format!("{name}: {m}")),
            }
        }
    } else {
        let plan = Plan {
            input: in_dir.clone(),
            output: out_dir.clone(),
            ..probe.plan.clone()
        };
        let argv = args::engine_args(&plan);
        let before: Vec<(PathBuf, Option<std::time::SystemTime>)> = files
            .iter()
            .map(|f| {
                (
                    target(f),
                    std::fs::metadata(target(f)).and_then(|m| m.modified()).ok(),
                )
            })
            .collect();
        let result = engine::run_directory(
            &probe.exe,
            &argv,
            files.len(),
            budget,
            on_progress,
            should_stop,
        );
        let (report, killed) = match result {
            Ok(r) => (r, false),
            Err((Failure::Budget(_) | Failure::Cancelled, r)) => (r, true),
            Err((Failure::Spawn(e), _)) => return Err((500, e)),
            Err((_, r)) => (r, false),
        };
        for (f, (t, was)) in files.iter().zip(before) {
            let name = f
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            let now = std::fs::metadata(&t).and_then(|m| m.modified()).ok();
            let written = now.is_some() && now != was;
            if written && imagesize::size(&t).is_ok() {
                outputs.push(t.display().to_string());
                continue;
            }
            let reason = report
                .errors
                .iter()
                .find(|e| e.contains(name.as_str()))
                .cloned()
                .unwrap_or_else(|| {
                    if killed {
                        "not processed, the call's budget ran out; run this batch with start_job"
                            .into()
                    } else {
                        "the engine wrote no output for it".into()
                    }
                });
            failed.push(format!("{name}: {reason}"));
        }
        if probe.copy_metadata {
            let mut still_ok = Vec::new();
            for (f, out) in files.iter().zip(files.iter().map(|f| target(f))) {
                let o = out.display().to_string();
                if !outputs.contains(&o) {
                    continue;
                }
                match upscale::copy_metadata(ctx, f, &out) {
                    Ok(()) => still_ok.push(o),
                    Err((_, m)) => failed.push(format!(
                        "{}: {m}",
                        f.file_name().and_then(|s| s.to_str()).unwrap_or("?")
                    )),
                }
            }
            outputs = still_ok;
        }
        ctx.log(
            if failed.is_empty() { "info" } else { "warning" },
            "upscayl.batch",
            &format!(
                "batch of {} from {} with {}: {} written, {} failed",
                files.len(),
                in_dir.display(),
                probe.model.label,
                outputs.len(),
                failed.len()
            ),
            &[
                ("model".to_string(), probe.model.label.as_str().into()),
                ("license".to_string(), probe.model.license.as_str().into()),
                (
                    "input_folder".to_string(),
                    in_dir.display().to_string().into(),
                ),
                (
                    "output_folder".to_string(),
                    out_dir.display().to_string().into(),
                ),
                ("args".to_string(), argv.join(" ").into()),
                ("written".to_string(), outputs.len().into()),
                ("failed".to_string(), failed.len().into()),
                (
                    "elapsed_ms".to_string(),
                    (started.elapsed().as_millis() as i64).into(),
                ),
            ],
        );
    }

    Ok(Value::object()
        .with("output_folder", out_dir.display().to_string().into())
        .with(
            "outputs",
            Value::Array(outputs.iter().map(|o| o.as_str().into()).collect()),
        )
        .with(
            "failed",
            Value::Array(failed.iter().map(|o| o.as_str().into()).collect()),
        )
        .with("complete", failed.is_empty().into())
        .with("elapsed_ms", (started.elapsed().as_millis() as i64).into())
        .with("model", probe.model.label.as_str().into())
        .with("license", probe.model.license.as_str().into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_follow_upstream() {
        assert_eq!(
            folder_name("png", "ultrasharp-4x", &Size::Scale(4)),
            "upscayl_png_ultrasharp-4x_4x"
        );
        assert_eq!(
            folder_name("jpg", "m", &Size::Width(1920)),
            "upscayl_jpg_m_1920px"
        );
    }

    #[test]
    fn inputs_sharing_a_stem_are_found() {
        let files = vec![
            PathBuf::from("a.jpg"),
            PathBuf::from("A.png"),
            PathBuf::from("b.webp"),
        ];
        assert_eq!(stem_clashes(&files), vec!["a.jpg and A.png".to_string()]);
    }

    mod engine_runs {
        use super::*;
        use hollowdeck_module::Headers;

        fn ctx() -> Option<ModuleContext> {
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            if !dir.join(crate::paths::ENGINE_REL).is_file() {
                eprintln!("skipped: engine not staged; run scripts/sync-engine.ps1");
                return None;
            }
            Some(ModuleContext {
                module_id: "upscayl".into(),
                module_dir: dir.clone(),
                mount_path: "/m/upscayl".into(),
                shared_data_dir: dir.join("target/test-data"),
                data_dir: dir.join("target/test-data/module_data/upscayl-batch"),
                core_version: None,
                core_bind: "127.0.0.1".into(),
                core_url: None,
                secret: None,
                port: 1,
            })
        }

        /// Two good images and one corrupt one, in a fresh folder.
        fn folder(name: &str) -> PathBuf {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/test-out/batch")
                .join(name);
            let _ = std::fs::remove_dir_all(&root);
            let input = root.join("in");
            std::fs::create_dir_all(&input).unwrap();
            let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../to_upscale.jpeg");
            std::fs::copy(&sample, input.join("a.jpeg")).unwrap();
            std::fs::copy(&sample, input.join("b.jpg")).unwrap();
            std::fs::write(input.join("corrupt.jpg"), b"\xff\xd8\xff\xe0 not a jpeg").unwrap();
            std::fs::write(input.join("notes.txt"), b"not an image").unwrap();
            root
        }

        fn post(inputs: serde_json::Value) -> Request {
            let body = serde_json::json!({"inputs": inputs, "base_dir": env!("CARGO_MANIFEST_DIR"), "base_dir_source": "project"});
            Request {
                method: "POST".into(),
                path: "/tools/upscale_batch".into(),
                query: String::new(),
                headers: Headers::new(),
                body: body.to_string().into_bytes(),
            }
        }

        fn outputs(r: &Response) -> serde_json::Value {
            assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
            serde_json::from_slice::<serde_json::Value>(&r.body).unwrap()["outputs"].clone()
        }

        #[test]
        fn a_folder_with_one_corrupt_file_gives_two_outputs_and_one_failure() {
            let Some(ctx) = ctx() else { return };
            let root = folder("dirmode");
            let r = handle(
                &post(
                    serde_json::json!({"input_folder": root.join("in").display().to_string(), "model": "upscayl-lite-4x", "scale": 2}),
                ),
                &ctx,
            );
            let o = outputs(&r);
            assert!(
                o["output_folder"]
                    .as_str()
                    .unwrap()
                    .ends_with("upscayl_png_upscayl-lite-4x_2x"),
                "{o}"
            );
            assert_eq!(o["outputs"].as_array().unwrap().len(), 2, "{o}");
            let failed = o["failed"].as_array().unwrap();
            assert_eq!(failed.len(), 1, "{o}");
            assert!(
                failed[0]
                    .as_str()
                    .unwrap()
                    .starts_with("corrupt.jpg: Couldn't read the image"),
                "{o}"
            );
            assert_eq!(o["complete"], false);
        }

        #[test]
        fn existing_outputs_are_left_alone_without_overwrite() {
            let Some(ctx) = ctx() else { return };
            let root = folder("existing");
            let inputs = serde_json::json!({"input_folder": root.join("in").display().to_string(), "output_folder": root.join("out").display().to_string(), "model": "upscayl-lite-4x", "scale": 2});
            let first = outputs(&handle(&post(inputs.clone()), &ctx));
            assert_eq!(first["outputs"].as_array().unwrap().len(), 2);
            let again = outputs(&handle(&post(inputs), &ctx));
            assert_eq!(again["outputs"].as_array().unwrap().len(), 0, "{again}");
            let failed: Vec<String> = again["failed"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
            assert!(
                failed
                    .iter()
                    .filter(|f| f.contains("already exists"))
                    .count()
                    == 2,
                "{failed:?}"
            );
        }

        #[test]
        fn double_and_metadata_work_in_batch() {
            let Some(ctx) = ctx() else { return };
            let root = folder("double");
            let o = outputs(&handle(
                &post(
                    serde_json::json!({"input_folder": root.join("in").display().to_string(), "model": "upscayl-lite-4x", "scale": 2, "double_upscayl": true, "copy_metadata": true, "format": "jpg"}),
                ),
                &ctx,
            ));
            assert!(
                o["output_folder"]
                    .as_str()
                    .unwrap()
                    .ends_with("upscayl_jpg_upscayl-lite-4x_4x"),
                "{o}"
            );
            let outs = o["outputs"].as_array().unwrap();
            assert_eq!(outs.len(), 2, "{o}");
            let d = imagesize::size(outs[0].as_str().unwrap()).unwrap();
            assert_eq!(d.width, 1024);
            assert_eq!(o["failed"].as_array().unwrap().len(), 1, "{o}");
        }
    }
}
