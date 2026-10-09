//! Which model a request means, where its files are, and what license it carries.
//!
//! Bundled models are listed in `licenses/models.json` and staged into `engine/models/`
//! by the sync script. A custom model is a `.param` path whose `.bin` sits beside it, in
//! a folder named `models` -- the engine refuses any other folder name (finding 8 in
//! `docs/results/engine-probe.md`) and crashes on a missing file (finding 9), so both
//! are checked here, before anything is spawned.

use crate::args::scale_from_name;
use crate::paths;
use crate::resolve::{self, Base};
use hollowdeck_module::ModuleContext;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundled {
    pub name: String,
    pub native_scale: u32,
    pub license: String,
    pub commercial_use: String,
    pub attribution: String,
}

/// The license record, read off disk on each call: it is small, and reading it each time
/// means an edit is picked up without a restart.
pub fn bundled(ctx: &ModuleContext) -> Result<Vec<Bundled>, String> {
    let path = ctx.module_path(paths::LICENSES_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let doc: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not valid JSON: {e}", path.display()))?;
    let list = doc["models"]
        .as_array()
        .ok_or_else(|| format!("{} has no models array", path.display()))?;
    Ok(list
        .iter()
        .filter_map(|m| {
            let s = |k: &str| m[k].as_str().unwrap_or("").to_string();
            let name = s("name");
            (!name.is_empty()).then(|| Bundled {
                native_scale: m["native_scale"]
                    .as_u64()
                    .map_or_else(|| scale_from_name(&name), |v| v as u32),
                license: s("license"),
                commercial_use: s("commercial_use"),
                attribution: s("attribution"),
                name,
            })
        })
        .collect())
}

/// A model ready to hand to the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// Passed as `-m`; always a folder named `models`.
    pub dir: PathBuf,
    pub name: String,
    pub native_scale: u32,
    pub license: String,
    /// What the event log calls it: a bundled token or the `.param` path.
    pub label: String,
}

pub const TOKEN_PREFIX: &str = "upscayl:";

/// `custom_model` wins when it is not empty; otherwise `model` names a bundled model.
/// `model_scale` > 0 overrides the scale read from the name.
pub fn resolve_model(
    ctx: &ModuleContext,
    base: &Base,
    model: &str,
    custom: &str,
    model_scale: u32,
) -> Result<Resolved, String> {
    let custom = custom.trim();
    if custom.is_empty() {
        return resolve_bundled(ctx, model.trim(), model_scale);
    }
    if let Some(rest) = custom.strip_prefix(TOKEN_PREFIX) {
        if let Some(name) = rest.strip_prefix("bundled/") {
            return resolve_bundled(ctx, name, model_scale);
        }
        if let Some(id) = rest.strip_prefix("imported/") {
            let (dir, name, scale, license) = crate::assets::imported(ctx, id)?;
            for ext in ["param", "bin"] {
                if !dir.join(format!("{name}.{ext}")).is_file() {
                    return Err(format!(
                        "imported model '{id}' is missing {name}.{ext}; import it again"
                    ));
                }
            }
            return Ok(Resolved {
                dir,
                native_scale: if model_scale > 0 { model_scale } else { scale },
                license,
                label: custom.to_string(),
                name,
            });
        }
        return Err(format!(
            "custom_model \"{custom}\" is a model token this module does not know. Bundled \
             models are upscayl:bundled/<name>, imported ones upscayl:imported/<id>"
        ));
    }
    let param = resolve::resolve(base, custom, "custom_model")?;
    let is_param = param
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("param"));
    if !is_param {
        return Err(format!(
            "{}. custom_model must name a .param file, with its .bin beside it",
            resolve::treatment(base, custom, "custom_model")
        ));
    }
    if !param.is_file() {
        return Err(format!(
            "{}, and no file is there",
            resolve::treatment(base, custom, "custom_model")
        ));
    }
    let bin = param.with_extension("bin");
    if !bin.is_file() {
        return Err(format!(
            "custom_model has no .bin beside it: expected \"{}\"",
            bin.display()
        ));
    }
    let dir = param.parent().map(PathBuf::from).unwrap_or_default();
    let named_models = dir
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "models");
    if !named_models {
        return Err(format!(
            "custom_model is in \"{}\", but the engine only loads models from a folder \
             named models. Move the .param and .bin into a folder called models",
            dir.display()
        ));
    }
    let name = param
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    Ok(Resolved {
        native_scale: if model_scale > 0 {
            model_scale
        } else {
            scale_from_name(&name)
        },
        license: "unrecorded".into(),
        label: param.display().to_string(),
        name,
        dir,
    })
}

fn resolve_bundled(ctx: &ModuleContext, name: &str, model_scale: u32) -> Result<Resolved, String> {
    let all = bundled(ctx)?;
    let Some(m) = all.iter().find(|m| m.name == name) else {
        let names: Vec<&str> = all.iter().map(|m| m.name.as_str()).collect();
        return Err(format!(
            "model \"{name}\" is not a bundled model. Bundled: {}",
            names.join(", ")
        ));
    };
    let dir = ctx.module_path(paths::MODELS_REL);
    for ext in ["param", "bin"] {
        if !dir.join(format!("{name}.{ext}")).is_file() {
            return Err(format!(
                "model \"{name}\" is not staged ({name}.{ext} is missing from engine/models). \
                 Run scripts/sync-engine.ps1; a model whose license is unknown, or a \
                 non-commercial one under -NoNonCommercial, is never staged"
            ));
        }
    }
    Ok(Resolved {
        dir,
        name: m.name.clone(),
        native_scale: if model_scale > 0 {
            model_scale
        } else {
            m.native_scale
        },
        license: m.license.clone(),
        label: format!("{TOKEN_PREFIX}bundled/{}", m.name),
    })
}
