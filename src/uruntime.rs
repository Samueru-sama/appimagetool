use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::elf;
use crate::error::{Error, Result};
use crate::util;

/// Sections that may carry the URUNTIME_MOUNT marker, in priority order.
const MOUNT_PATCH_SECTIONS: &[&str] = &[".upd_info", ".envs"];

/// Existing marker values we know how to rewrite.
const MOUNT_PATCH_PATTERNS: &[&str] = &[
    "URUNTIME_MOUNT=3",
    "URUNTIME_MOUNT=2",
    "URUNTIME_MOUNT=1",
    "URUNTIME_MOUNT=0",
];

/// Default uruntime download URL pattern.
/// `{arch}` gets replaced with the target architecture.
const DEFAULT_URL_TEMPLATE: &str = "https://github.com/VHSgunzo/uruntime/releases/latest/download/uruntime-appimage-dwarfs-lite-{arch}";

/// Ensure a runtime binary is available. Returns the path to the runtime.
///
/// If `config.runtime` is set and the file exists, use that.
/// Otherwise, download from `config.runtime_url` (or the default URL) to `config.tmpdir`.
pub fn resolve_runtime(config: &Config) -> Result<PathBuf> {
    // If user provided a runtime path, use it directly
    if let Some(ref path) = config.runtime {
        if path.exists() {
            return Ok(path.clone());
        }
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("runtime not found at {}", path.display()),
        )));
    }

    // Check cache in tmpdir (include arch in name for cross-builds)
    let cached = config.tmpdir.join(format!("uruntime-{}", config.arch));
    if cached.exists() && util::is_elf(&cached) {
        // Return a copy so the cached original stays pristine
        let work = config.tmpdir.join(format!("uruntime-{}.work", config.arch));
        std::fs::copy(&cached, &work)?;
        set_executable(&work)?;
        return Ok(work);
    }

    // Download
    let url = config
        .runtime_url
        .as_deref()
        .unwrap_or(DEFAULT_URL_TEMPLATE)
        .replace("{arch}", &config.arch);

    crate::log_info!("Downloading uruntime from {url}...");
    util::download(&url, &cached)?;
    set_executable(&cached)?;

    if !util::is_elf(&cached) {
        let _ = std::fs::remove_file(&cached);
        return Err(Error::DownloadFailed {
            url,
            reason: "downloaded file is not a valid ELF binary".to_string(),
        });
    }

    // Return a working copy so the cached original stays pristine
    let work = config.tmpdir.join(format!("uruntime-{}.work", config.arch));
    std::fs::copy(&cached, &work)?;
    set_executable(&work)?;
    Ok(work)
}

/// Configure the runtime: write ELF sections for update info and env vars,
/// and optionally patch runtime behavior.
pub fn configure_runtime(
    runtime_path: &Path,
    update_info: Option<&str>,
    env_vars: &[String],
    keep_mount: bool,
) -> Result<()> {
    let mut data = std::fs::read(runtime_path)?;

    // Write update info into .upd_info section
    if let Some(upinfo) = update_info {
        crate::log_info!("Adding update information to runtime...");
        elf::write_section(&mut data, ".upd_info", upinfo.as_bytes())?;
    }

    // Write env vars into .envs section
    if !env_vars.is_empty() {
        crate::log_info!("Adding environment variables to runtime...");
        let env_data: String = env_vars.join("\n");
        elf::write_section(&mut data, ".envs", env_data.as_bytes())?;
    }

    if keep_mount {
        crate::log_info!("Setting runtime to keep mount point...");
        patch_keep_mount(&mut data)?;
    }

    std::fs::write(runtime_path, data)?;
    Ok(())
}

/// Rewrite any `URUNTIME_MOUNT=<n>` marker found in the runtime's config sections
/// to `URUNTIME_MOUNT=0` (keep-mount). Returns an error if no marker is found
/// in any known section — silently leaving the runtime unmodified would mask a
/// real runtime/version mismatch.
fn patch_keep_mount(data: &mut [u8]) -> Result<()> {
    for section in MOUNT_PATCH_SECTIONS {
        // Skip sections the runtime doesn't carry.
        if elf::find_section(data, section).is_none() {
            continue;
        }
        for pattern in MOUNT_PATCH_PATTERNS {
            if elf::patch_section_string(data, section, pattern, "URUNTIME_MOUNT=0")? {
                return Ok(());
            }
        }
    }
    Err(Error::Config(
        "could not patch URUNTIME_MOUNT: marker not found in .upd_info or .envs\n  \
         hint: this runtime build may not support URUNTIME_PRELOAD; \
         try a newer uruntime release"
            .to_string(),
    ))
}

fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}
