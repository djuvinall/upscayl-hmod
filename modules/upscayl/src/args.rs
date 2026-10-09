//! Turning a validated request into `upscayl-bin` arguments.
//!
//! Pure: no filesystem, no process. Every rule here comes from a measurement in
//! `docs/results/engine-probe.md`, cited by finding number, because several of them
//! contradict upstream's own argument builder.

use std::path::PathBuf;

/// How big the output is. Exactly one of these, because the engine honours neither
/// `-w` nor `-s` when both are sent, and neither `-w` nor `-r` when both are sent
/// (findings 3 and 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Size {
    /// `-s`: an integer factor, 1 to 16 (finding 2).
    Scale(u32),
    /// `-w`: a width in pixels, aspect kept (finding 5).
    Width(u32),
    /// `-r WxH`: exact dimensions, aspect not kept (finding 5).
    Resize(u32, u32),
}

/// The engine's resize filters, from `-r help` (finding 5).
pub const FILTERS: [&str; 7] = [
    "default",
    "box",
    "triangle",
    "cubicbspline",
    "catmullrom",
    "mitchell",
    "pointsample",
];

pub const FORMATS: [&str; 3] = ["png", "jpg", "webp"];

/// One engine run, fully decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub input: PathBuf,
    pub output: PathBuf,
    /// The folder holding the model; its last component is `models` (finding 8).
    pub models_dir: PathBuf,
    pub model_name: String,
    /// Always sent as `-z` (finding 1).
    pub native_scale: u32,
    pub size: Size,
    /// Only meaningful with `Width` or `Resize`; `None` is the engine's default.
    pub filter: Option<String>,
    pub format: String,
    /// WebP only; the engine ignores it for PNG and JPEG (finding 6).
    pub compression: u32,
    pub gpu_id: Option<String>,
    pub tile_size: Option<String>,
    pub threads: Option<String>,
    pub tta: bool,
}

/// The argv after the executable. `-z` and the size flag are always present; the
/// optional tuning flags only when asked for.
pub fn engine_args(plan: &Plan) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-i".into(),
        plan.input.display().to_string(),
        "-o".into(),
        plan.output.display().to_string(),
        "-m".into(),
        plan.models_dir.display().to_string(),
        "-n".into(),
        plan.model_name.clone(),
        "-z".into(),
        plan.native_scale.to_string(),
    ];
    let filter = |v: String| match &plan.filter {
        Some(f) if f != "default" => format!("{v}:{f}"),
        _ => v,
    };
    match plan.size {
        Size::Scale(s) => a.extend(["-s".into(), s.to_string()]),
        Size::Width(w) => a.extend(["-w".into(), filter(w.to_string())]),
        Size::Resize(w, h) => a.extend(["-r".into(), filter(format!("{w}x{h}"))]),
    }
    a.extend(["-f".into(), plan.format.clone()]);
    a.extend(["-c".into(), plan.compression.to_string()]);
    if let Some(g) = &plan.gpu_id {
        a.extend(["-g".into(), g.clone()]);
    }
    if let Some(t) = &plan.tile_size {
        a.extend(["-t".into(), t.clone()]);
    }
    if let Some(j) = &plan.threads {
        a.extend(["-j".into(), j.clone()]);
    }
    if plan.tta {
        a.push("-x".into());
    }
    a
}

/// The model's native scale from its name, as upstream's `check-model-scale.ts` reads
/// it: `x2`/`2x` is 2, `x3`/`3x` is 3, anything else 4.
pub fn scale_from_name(name: &str) -> u32 {
    let n = name.to_ascii_lowercase();
    if n.contains("x2") || n.contains("2x") {
        2
    } else if n.contains("x3") || n.contains("3x") {
        3
    } else {
        4
    }
}

/// The part of upstream's output name that says how big it is.
pub fn size_label(size: &Size) -> String {
    match size {
        Size::Scale(s) => format!("{s}x"),
        Size::Width(w) => format!("{w}px"),
        Size::Resize(w, h) => format!("{w}x{h}"),
    }
}

/// `<stem>_upscayl_<size>_<model>.<format>`, upstream's naming.
pub fn output_name(input_stem: &str, size: &Size, model: &str, format: &str) -> String {
    format!("{input_stem}_upscayl_{}_{model}.{format}", size_label(size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> Plan {
        Plan {
            input: PathBuf::from("in.jpg"),
            output: PathBuf::from("out.png"),
            models_dir: PathBuf::from("engine/models"),
            model_name: "upscayl-lite-4x".into(),
            native_scale: 4,
            size: Size::Scale(4),
            filter: None,
            format: "png".into(),
            compression: 0,
            gpu_id: None,
            tile_size: None,
            threads: None,
            tta: false,
        }
    }

    fn joined(p: &Plan) -> String {
        engine_args(p).join(" ")
    }

    #[test]
    fn z_and_s_are_always_sent_even_when_they_match() {
        // Upstream omits -s when it equals the model scale and never sends -z; both are
        // wrong for a non-4x model (findings 1 and 2).
        let a = joined(&plan());
        assert!(a.contains("-z 4"), "{a}");
        assert!(a.contains("-s 4"), "{a}");
    }

    #[test]
    fn a_2x_model_gets_z_2() {
        let mut p = plan();
        p.model_name = "realesr-animevideov3-x2".into();
        p.native_scale = scale_from_name(&p.model_name);
        p.size = Size::Scale(2);
        let a = joined(&p);
        assert!(a.contains("-z 2 -s 2"), "{a}");
    }

    #[test]
    fn width_replaces_scale_and_carries_its_filter() {
        let mut p = plan();
        p.size = Size::Width(1000);
        p.filter = Some("pointsample".into());
        let a = joined(&p);
        assert!(a.contains("-w 1000:pointsample"), "{a}");
        assert!(!a.contains("-s "), "{a}");
        assert!(!a.contains("-r "), "{a}");
    }

    #[test]
    fn resize_replaces_scale_and_the_default_filter_is_not_spelled() {
        let mut p = plan();
        p.size = Size::Resize(1920, 1080);
        p.filter = Some("default".into());
        let a = joined(&p);
        assert!(a.contains("-r 1920x1080 "), "{a}");
        assert!(!a.contains("-s ") && !a.contains("-w "), "{a}");
    }

    #[test]
    fn tuning_flags_appear_only_when_asked_for() {
        let a = joined(&plan());
        for f in ["-g", "-t", "-j", "-x"] {
            assert!(!a.split(' ').any(|t| t == f), "{f} in {a}");
        }
        let mut p = plan();
        p.gpu_id = Some("0,1".into());
        p.tile_size = Some("0,0".into());
        p.threads = Some("2:4:4".into());
        p.tta = true;
        let a = joined(&p);
        assert!(a.contains("-g 0,1") && a.contains("-t 0,0") && a.contains("-j 2:4:4"));
        assert!(a.ends_with("-x"), "{a}");
    }

    #[test]
    fn format_and_compression_are_always_sent() {
        let mut p = plan();
        p.format = "webp".into();
        p.compression = 50;
        let a = joined(&p);
        assert!(a.contains("-f webp -c 50"), "{a}");
    }

    #[test]
    fn scale_from_name_matches_upstream() {
        assert_eq!(scale_from_name("realesr-animevideov3-x2"), 2);
        assert_eq!(scale_from_name("RealESRGAN_2x_thing"), 2);
        assert_eq!(scale_from_name("model-x3"), 3);
        assert_eq!(scale_from_name("ultrasharp-4x"), 4);
        assert_eq!(scale_from_name("plain"), 4);
    }

    #[test]
    fn output_names_follow_upstream() {
        assert_eq!(
            output_name("photo", &Size::Scale(4), "ultrasharp-4x", "png"),
            "photo_upscayl_4x_ultrasharp-4x.png"
        );
        assert_eq!(
            output_name("photo", &Size::Width(1920), "m", "jpg"),
            "photo_upscayl_1920px_m.jpg"
        );
        assert_eq!(
            output_name("photo", &Size::Resize(800, 600), "m", "webp"),
            "photo_upscayl_800x600_m.webp"
        );
    }
}
