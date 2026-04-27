use std::io::{Read, Write};
use std::path::Path;

use crate::error::{Error, Result};

/// Download a file with retries.
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

    let mut file = std::fs::File::create(dest)?;
    let reader = response.into_body().into_reader();
    let mut reader = std::io::BufReader::new(reader);
    std::io::copy(&mut reader, &mut file)?;
    file.flush()?;
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
