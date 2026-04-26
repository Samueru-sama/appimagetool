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

    #[error("zsync generation failed: {0}")]
    ZsyncGenerate(String),

    #[error("zsync write failed: {0}")]
    ZsyncWrite(String),

    #[error("{0}")]
    Config(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<zsync_rs::GenerateError> for Error {
    fn from(e: zsync_rs::GenerateError) -> Self {
        Error::ZsyncGenerate(e.to_string())
    }
}

impl From<zsync_rs::WriteError> for Error {
    fn from(e: zsync_rs::WriteError) -> Self {
        Error::ZsyncWrite(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
