use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Download a file with retries. The download is written to a per-process
/// temp file alongside `dest` and atomically renamed on success, so two
/// concurrent builds racing on the same cache path can't observe a torn
/// half-downloaded file.
pub fn download(url: &str, dest: &Path) -> Result<()> {
    let mut last_err = String::new();
    for attempt in 0..5 {
        if attempt > 0 {
            crate::log_warn!("Download failed, retrying in 5s...");
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
        match try_download(url, dest) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(Error::DownloadFailed {
        url: url.to_string(),
        reason: last_err,
    })
}

fn try_download(url: &str, dest: &Path) -> Result<()> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let staging = staging_path(dest);
    {
        let mut file = std::fs::File::create(&staging)?;
        let reader = response.into_body().into_reader();
        let mut reader = std::io::BufReader::new(reader);
        if let Err(e) = std::io::copy(&mut reader, &mut file) {
            let _ = std::fs::remove_file(&staging);
            return Err(e.into());
        }
        file.flush()?;
    }
    if let Err(e) = std::fs::rename(&staging, dest) {
        let _ = std::fs::remove_file(&staging);
        return Err(e.into());
    }
    Ok(())
}

/// Build a per-process staging path next to `dest` for atomic-rename writes.
fn staging_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".partial.{}", std::process::id()));
    dest.with_file_name(name)
}

/// Build a per-process unique path inside `dir` with the given basename. Used
/// for transient files (profile tarballs, working copies of binaries) so two
/// concurrent builds in the same TMPDIR don't clobber each other.
pub fn process_unique_path(dir: &Path, basename: &str) -> PathBuf {
    dir.join(format!("{basename}.{}", std::process::id()))
}

/// Ensure a cached binary exists at `cached`, downloading from `url` if missing
/// or invalid. The downloaded file is marked executable and verified to be ELF.
/// On verification failure the file is removed so the next run re-downloads.
pub fn ensure_cached_binary(cached: &Path, url: &str, label: &str) -> Result<()> {
    if cached.exists() && is_elf(cached) {
        return Ok(());
    }
    crate::log_info!("Downloading {label} from {url}...");
    download(url, cached)?;
    set_executable(cached)?;
    if !is_elf(cached) {
        let _ = std::fs::remove_file(cached);
        return Err(Error::DownloadFailed {
            url: url.to_string(),
            reason: "downloaded file is not a valid ELF binary".to_string(),
        });
    }
    Ok(())
}

/// Mark a file as executable (0o755).
pub fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// Sanitize a string for use as a filename.
/// Replaces characters that are problematic in filenames — including path
/// separators and NUL — with underscores. The desktop entry's `Name=` field
/// flows into output paths, so this also defends against path traversal.
pub fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_whitespace()
                || c.is_control()
                || matches!(
                    c,
                    '"' | ':' | '>' | '<' | '*' | '|' | '?' | '/' | '\\' | '\0'
                )
            {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim_end_matches('_')
        .to_string()
}

/// Check if a file starts with the ELF magic bytes.
pub fn is_elf(path: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 4];
    f.read_exact(&mut buf).is_ok() && buf == *b"\x7fELF"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename_whitespace() {
        assert_eq!(sanitize_filename("hello world"), "hello_world");
    }

    #[test]
    fn test_sanitize_filename_colon() {
        assert_eq!(sanitize_filename("app:name"), "app_name");
    }

    #[test]
    fn test_sanitize_filename_dots_and_dashes() {
        assert_eq!(sanitize_filename("my-app-1.0"), "my-app-1.0");
    }

    #[test]
    fn test_sanitize_filename_trailing_underscores() {
        assert_eq!(sanitize_filename("app***"), "app");
    }

    #[test]
    fn test_sanitize_filename_all_special() {
        assert_eq!(sanitize_filename("<>:\"|?*"), "");
    }

    #[test]
    fn test_sanitize_filename_mixed() {
        assert_eq!(
            sanitize_filename("My App v2.0 (beta)"),
            "My_App_v2.0_(beta)"
        );
    }

    #[test]
    fn test_sanitize_filename_newlines() {
        assert_eq!(sanitize_filename("line1\nline2"), "line1_line2");
    }

    #[test]
    fn test_sanitize_filename_empty() {
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn test_sanitize_filename_path_separators() {
        // Path traversal must be neutralised on both unix and windows separators.
        assert_eq!(sanitize_filename("../etc/passwd"), ".._etc_passwd");
        assert_eq!(sanitize_filename(r"foo\bar"), "foo_bar");
        assert_eq!(sanitize_filename("a/b/c"), "a_b_c");
    }

    #[test]
    fn test_sanitize_filename_nul_and_control() {
        assert_eq!(sanitize_filename("foo\0bar"), "foo_bar");
        assert_eq!(sanitize_filename("foo\x07bar"), "foo_bar");
    }
}
