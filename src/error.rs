use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("AppDir not found: {0}")]
    AppDirNotFound(PathBuf),

    #[error("No .desktop file found in AppDir")]
    NoDesktopEntry,

    #[error("No .DirIcon found in AppDir")]
    NoDirIcon,

    #[error("No AppRun found in AppDir")]
    NoAppRun,

    #[error("Failed to download {url}: {reason}")]
    DownloadFailed { url: String, reason: String },

    #[error("ELF section '{0}' not found in runtime")]
    SectionNotFound(String),

    #[error("Data too large for section '{name}': {size} > {capacity}")]
    SectionOverflow {
        name: String,
        size: usize,
        capacity: usize,
    },

    #[error("mkdwarfs failed: {0}")]
    DwarfsFailed(String),

    #[error("mksquashfs failed: {0}")]
    SquashfsFailed(String),

    #[error("{0}")]
    Config(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
