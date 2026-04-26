use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::util;

/// Parsed metadata from a .desktop file.
pub struct DesktopEntry {
    pub path: PathBuf,
    pub name: String,
    pub exec: String,
    pub icon_name: Option<String>,
}

impl DesktopEntry {
    /// Find and parse the single top-level .desktop file in an AppDir.
    pub fn from_appdir(appdir: &Path) -> Result<Self> {
        let mut found: Option<PathBuf> = None;
        for entry in
            std::fs::read_dir(appdir).map_err(|_| Error::AppDirNotFound(appdir.to_path_buf()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "desktop") && path.parent() == Some(appdir)
            {
                if found.is_some() {
                    return Err(Error::Config(
                        "multiple .desktop files found in AppDir, expected exactly one".to_string(),
                    ));
                }
                found = Some(path);
            }
        }

        let path = found.ok_or(Error::NoDesktopEntry)?;
        let content = std::fs::read_to_string(&path)?;

        let name = parse_key(&content, "Name").unwrap_or_default();
        let exec = parse_key(&content, "Exec").unwrap_or_default();
        let icon_name = parse_key(&content, "Icon");

        Ok(DesktopEntry {
            path,
            name,
            exec,
            icon_name,
        })
    }

    /// Add X-AppImage-* entries to the desktop file.
    pub fn add_appimage_metadata(&self, app_name: &str, version: &str, arch: &str) -> Result<()> {
        let content = std::fs::read_to_string(&self.path)?;

        // Remove existing X-AppImage-* lines
        let filtered: String = content
            .lines()
            .filter(|line| {
                !line.starts_with("X-AppImage-Name=")
                    && !line.starts_with("X-AppImage-Version=")
                    && !line.starts_with("X-AppImage-Arch=")
            })
            .collect::<Vec<&str>>()
            .join("\n");

        let mut out = filtered;
        // Ensure there's a trailing newline before appending
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("X-AppImage-Name={app_name}\n"));
        out.push_str(&format!("X-AppImage-Version={version}\n"));
        out.push_str(&format!("X-AppImage-Arch={arch}\n"));

        std::fs::write(&self.path, out)?;
        Ok(())
    }

    /// Get the main binary name from the Exec key.
    pub fn main_binary(&self) -> Option<&str> {
        let exec = self.exec.split_whitespace().next()?;
        let name = exec.trim_matches('"');
        let name = name.rsplit('/').next().unwrap_or(name);
        Some(name)
    }

    /// Verify .DirIcon exists in the AppDir.
    pub fn check_dir_icon(appdir: &Path) -> Result<()> {
        let dir_icon = appdir.join(".DirIcon");
        if dir_icon.exists() {
            Ok(())
        } else {
            Err(Error::NoDirIcon)
        }
    }

    /// Verify AppRun exists in the AppDir.
    pub fn check_apprun(appdir: &Path) -> Result<()> {
        let apprun = appdir.join("AppRun");
        if apprun.exists() {
            Ok(())
        } else {
            Err(Error::NoAppRun)
        }
    }
}

/// Compute the output filename.
pub fn compute_output_name(app_name: &str, version: Option<&str>, arch: &str) -> String {
    let app_name = util::sanitize_filename(app_name);
    match version {
        Some(v) if !v.is_empty() => {
            let v = util::sanitize_filename(v);
            format!("{app_name}-{v}-anylinux-{arch}.AppImage")
        }
        _ => {
            eprintln!("WARNING: VERSION is not set, omitting from filename");
            format!("{app_name}-anylinux-{arch}.AppImage")
        }
    }
}

/// Parse a key from a .desktop file (simple line-based parser).
fn parse_key(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(&prefix) {
            let value = rest.split_whitespace().next().unwrap_or(rest);
            let value = value.trim_matches('"');
            return Some(value.to_string());
        }
    }
    None
}
