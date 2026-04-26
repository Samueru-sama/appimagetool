use std::path::PathBuf;

use clap::Parser;

/// Create AppImages from an AppDir.
#[derive(Parser)]
#[command(name = "appimagetool", version, about)]
struct Cli {
    /// Path to the AppDir directory
    #[arg(env = "APPDIR", default_value = "./AppDir")]
    appdir: Option<PathBuf>,

    /// Output directory
    #[arg(short, long, env = "OUTPATH", default_value = ".")]
    output: Option<PathBuf>,

    /// Output filename (auto-detected from .desktop if not set)
    #[arg(short = 'n', long, env = "OUTNAME")]
    name: Option<String>,

    /// Target architecture
    #[arg(long, env = "ARCH")]
    arch: Option<String>,

    /// Path to uruntime binary
    #[arg(long, env = "RUNTIME")]
    runtime: Option<PathBuf>,

    /// URL to download uruntime from
    #[arg(long, env = "URUNTIME_LINK")]
    runtime_url: Option<String>,

    /// Update information string
    #[arg(short, long, env = "UPINFO")]
    update_info: Option<String>,

    /// DWARFS compression options
    #[arg(long, env = "DWARFS_COMP")]
    dwarfs_comp: Option<String>,

    /// Enable DWARFS profile optimization
    #[arg(long, env = "OPTIMIZE_LAUNCH", default_value = "false")]
    optimize_launch: bool,

    /// Path to DWARFS profile
    #[arg(long, env = "DWARFSPROF")]
    dwarfs_profile: Option<PathBuf>,

    /// Path to mkdwarfs binary
    #[arg(long, env = "DWARFS_CMD")]
    mkdwarfs: Option<PathBuf>,

    /// URL to download mkdwarfs from
    #[arg(long, env = "DWARFS_LINK")]
    dwarfs_url: Option<String>,

    /// Temporary directory
    #[arg(long, env = "TMPDIR", default_value = "/tmp")]
    tmpdir: Option<PathBuf>,

    /// Increase verbosity (can be repeated: -v, -vv)
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress informational output (can be repeated: -q, -qq)
    #[arg(short = 'q', long, action = clap::ArgAction::Count, conflicts_with = "verbose")]
    quiet: u8,
}

fn main() {
    let cli = Cli::parse();

    // Initialize logger: verbosity is (verbose count) - (quiet count)
    let verbosity = cli.verbose as i8 - cli.quiet as i8;
    appimagetool::log::init(verbosity);

    let config = appimagetool::config::Config::from_cli_args(
        cli.appdir,
        cli.output,
        cli.name,
        cli.arch,
        cli.runtime,
        cli.runtime_url,
        cli.update_info,
        cli.dwarfs_comp,
        cli.optimize_launch,
        cli.dwarfs_profile,
        cli.mkdwarfs,
        cli.dwarfs_url,
        cli.tmpdir,
    )
    .unwrap_or_else(|e| {
        appimagetool::log_error!("{e}");
        std::process::exit(1);
    });

    if let Err(e) = appimagetool::appimage::build(&config) {
        appimagetool::log_error!("{e}");
        std::process::exit(1);
    }
}
