//! Where this module's own files live, relative to `HDECK_MODULE_DIR`.
//!
//! Never an absolute path: a module directory is packed and moved, so every file is
//! named relative to it at call time (INTEROP.md, rule 8).

use hollowdeck_module::ModuleContext;
use std::path::PathBuf;

/// The license record for every bundled model (`docs/arch/model-assets.md`).
pub const LICENSES_FILE: &str = "licenses/models.json";

#[cfg(windows)]
pub const PLATFORM: &str = "win";
#[cfg(target_os = "macos")]
pub const PLATFORM: &str = "mac";
#[cfg(all(unix, not(target_os = "macos")))]
pub const PLATFORM: &str = "linux";

#[cfg(windows)]
pub const ENGINE_REL: &str = "engine/bin/win/upscayl-bin.exe";
#[cfg(target_os = "macos")]
pub const ENGINE_REL: &str = "engine/bin/mac/upscayl-bin";
#[cfg(all(unix, not(target_os = "macos")))]
pub const ENGINE_REL: &str = "engine/bin/linux/upscayl-bin";

/// Where the sync script stages the bundled models. The last component must be
/// `models`: the engine refuses any other folder name.
pub const MODELS_REL: &str = "engine/models";

/// The staged exiftool, for copy_metadata.
pub const EXIFTOOL_REL: &str = "engine/exiftool/exiftool.exe";

/// The staged engine binary for this platform. It exists only after the sync script ran.
pub fn engine_binary(ctx: &ModuleContext) -> PathBuf {
    ctx.module_path(ENGINE_REL)
}
