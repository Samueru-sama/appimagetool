use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(
        "AppDir not found: {0}\n  hint: provide a valid AppDir path via --appdir or the APPDIR env var"
    )]
    AppDirNotFound(PathBuf),

    #[error(
        "No .desktop file found in AppDir\n  \
         hint: place exactly one .desktop file in the AppDir root"
    )]
    NoDesktopEntry,

    #[error(
        "No .DirIcon found in AppDir\n  \
         hint: add a PNG/SVG icon as .DirIcon in the AppDir root"
    )]
    NoDirIcon,

    #[error(
        "No AppRun found in AppDir\n  \
         hint: add an executable AppRun script in the AppDir root"
    )]
    NoAppRun,

    #[error(
        "Failed to download {url}: {reason}\n  hint: check your network connection and the URL"
    )]
    DownloadFailed { url: String, reason: String },

    #[error(
        "ELF section '{0}' not found in runtime\n  \
         hint: the runtime binary may be incompatible or corrupted; \
         try a different --runtime or remove the cached download"
    )]
    SectionNotFound(String),

    #[error(
        "Runtime is not a valid ELF binary or has corrupt section headers\n  \
         hint: remove the cached runtime and let it re-download, \
         or pass a known-good binary via --runtime"
    )]
    MalformedElf,

    #[error(
        "Data too large for ELF section '{name}': {size} > {capacity} bytes\n  \
         hint: shorten the value or use a runtime with larger ELF sections"
    )]
    SectionOverflow {
        name: String,
        size: usize,
        capacity: usize,
    },

    #[error("mkdwarfs failed: {0}\n  hint: check that the AppDir contents are valid and readable")]
    DwarfsFailed(String),

    #[error("mksquashfs failed: {0}")]
    SquashfsFailed(String),

    #[error(
        "zsync generation failed: {0}\n  \
         hint: ensure the output AppImage was created successfully"
    )]
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
