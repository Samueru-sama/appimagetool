use std::io::{Read, Write};
use std::path::Path;

use crate::error::{Error, Result};

/// Download a file with retries.
pub fn download(url: &str, dest: &Path) -> Result<()> {
    let mut last_err = String::new();
    for attempt in 0..5 {
        if attempt > 0 {
            eprintln!("Download failed, retrying in 5s...");
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
/// Replaces characters that are problematic in filenames with underscores.
pub fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_whitespace()
                || c == '"'
                || c == ':'
                || c == '>'
                || c == '<'
                || c == '*'
                || c == '|'
                || c == '?'
                || c == '\r'
                || c == '\n'
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
