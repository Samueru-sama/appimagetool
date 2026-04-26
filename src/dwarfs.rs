use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::util;

const DEFAULT_DWARFS_URL_TEMPLATE: &str =
    "https://github.com/mhx/dwarfs/releases/download/v0.15.1/dwarfs-universal-0.15.1-Linux-{arch}";

/// Resolve the mkdwarfs binary path. Checks user-provided path, then $PATH,
/// then downloads to tmpdir.
pub fn resolve_mkdwarfs(config: &Config) -> Result<PathBuf> {
    // User-provided path
    if let Some(ref path) = config.mkdwarfs {
        if path.exists() {
            return Ok(path.clone());
        }
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("mkdwarfs not found at {}", path.display()),
        )));
    }

    // Check $PATH
    if let Ok(path) = which("mkdwarfs") {
        return Ok(path);
    }

    // Check cache
    let cached = config.tmpdir.join("mkdwarfs");
    if cached.exists() {
        return Ok(cached);
    }

    // Download
    let url = config
        .dwarfs_url
        .as_deref()
        .unwrap_or(DEFAULT_DWARFS_URL_TEMPLATE)
        .replace("{arch}", &config.arch);
    eprintln!("Downloading mkdwarfs from {url}...");
    util::download(&url, &cached)?;
    set_executable(&cached)?;

    if !util::is_elf(&cached) {
        let _ = std::fs::remove_file(&cached);
        return Err(Error::DownloadFailed {
            url,
            reason: "downloaded file is not a valid ELF binary".to_string(),
        });
    }

    Ok(cached)
}

/// Build a DWARFS AppImage. The runtime is embedded via `--header`.
pub fn build_appimage(
    mkdwarfs: &Path,
    appdir: &Path,
    runtime: &Path,
    output: &Path,
    compression: &str,
    profile: Option<&Path>,
) -> Result<()> {
    eprintln!("Building DWARFS AppImage...");

    let mut cmd = Command::new(mkdwarfs);
    cmd.arg("--force")
        .arg("--order=path")
        .arg("--set-owner")
        .arg("0")
        .arg("--set-group")
        .arg("0")
        .arg("--no-history")
        .arg("--no-create-timestamp")
        .arg("--header")
        .arg(runtime)
        .arg("--input")
        .arg(appdir);

    // Add profile optimization if available
    if let Some(profile) = profile
        && profile.exists()
    {
        eprintln!("Using DWARFS profile {}...", profile.display());
        cmd.arg("--categorize=hotness")
            .arg(format!("--hotness-list={}", profile.display()));
    }

    // Add compression options. The string can contain multiple space-separated
    // args like "zstd:level=22 -S26 -B6". The first part goes to -C, the rest
    // are passed as separate args.
    let mut parts = compression.split_whitespace().peekable();
    if let Some(comp_algo) = parts.next() {
        cmd.arg("-C").arg(comp_algo);
    }
    for part in parts {
        cmd.arg(part);
    }

    cmd.arg("--output").arg(output);

    let status = cmd.status()?;
    if !status.success() {
        return Err(Error::DwarfsFailed(format!(
            "mkdwarfs exited with status {}",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(())
}

/// Build a temporary AppImage for DWARFS profiling with basic compression.
pub fn build_profile_image(
    mkdwarfs: &Path,
    appdir: &Path,
    runtime: &Path,
    output: &Path,
) -> Result<()> {
    eprintln!("Building temporary image for DWARFS profiling...");

    let status = Command::new(mkdwarfs)
        .arg("--force")
        .arg("--order=path")
        .arg("--set-owner")
        .arg("0")
        .arg("--set-group")
        .arg("0")
        .arg("--no-history")
        .arg("--no-create-timestamp")
        .arg("--header")
        .arg(runtime)
        .arg("--input")
        .arg(appdir)
        .arg("-C")
        .arg("zstd:level=5")
        .arg("-S19")
        .arg("--output")
        .arg(output)
        .status()?;

    if !status.success() {
        return Err(Error::DwarfsFailed(format!(
            "mkdwarfs (profile build) exited with status {}",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(())
}

/// Run DWARFS profiling: launch a temp AppImage under xvfb with DWARFS_ANALYSIS_FILE set,
/// wait for a timeout, then kill the process group and unmount any FUSE mounts.
pub fn run_profiling(
    appimage: &Path,
    profile_output: &Path,
    tmpdir: &Path,
    timeout_secs: u64,
) -> Result<()> {
    use std::os::unix::process::CommandExt;

    let tmp_profile = tmpdir.join("dwarfsprof.tmp");

    eprintln!("Running DWARFS profiling for {timeout_secs}s...");

    let mut child = Command::new("xvfb-run")
        .arg("-a")
        .arg("--")
        .arg(appimage)
        .env("DWARFS_ANALYSIS_FILE", &tmp_profile)
        // Create a new process group so we can kill the tree
        .process_group(0)
        .spawn()
        .map_err(|e| Error::Config(format!("failed to spawn xvfb-run: {e}")))?;

    let pgid = child.id() as i32;

    // Wait for the timeout
    std::thread::sleep(std::time::Duration::from_secs(timeout_secs));

    // Kill the entire process group
    unsafe {
        libc::kill(-pgid, libc::SIGTERM);
    }
    let _ = child.wait();

    // Unmount any FUSE mounts under tmpdir
    unmount_fuse(tmpdir);

    // Give the profile writer a moment to finalize
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Copy to final location (avoiding race with any remaining writer)
    if tmp_profile.exists() {
        std::fs::copy(&tmp_profile, profile_output)?;
        let _ = std::fs::remove_file(&tmp_profile);
        eprintln!("DWARFS profile written to {}", profile_output.display());
    } else {
        eprintln!("WARNING: DWARFS profile was not generated");
    }

    // Clean up temp appimage
    let _ = std::fs::remove_file(appimage);

    Ok(())
}

/// Unmount FUSE mounts under the given directory.
fn unmount_fuse(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".mount_") {
            let mountpoint = dir.join(&*name);
            let _ = Command::new("umount").arg(&mountpoint).status();
        }
    }
}

fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

/// Simple `which` implementation.
fn which(name: &str) -> std::result::Result<PathBuf, ()> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(())
}
