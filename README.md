# appimagetool

A Rust implementation of **appimagetool** for the [Anylinux-AppImages](https://github.com/pkgforge-dev/Anylinux-AppImages) project.

Takes a prepared AppDir and produces a finished AppImage using DWARFS compression and the [uruntime](https://github.com/VHSgunzo/uruntime) AppImage runtime.

## Features

- **DWARFS compression** — efficient delta-friendly AppImages
- **uruntime integration** — automatic download & ELF section configuration
- **Profile-guided optimization** — optional DWARFS hotness profiling for faster launches
- **zsync generation** — automatic delta update support via update information
- **Decentralized** — no central repository required
- **Single binary** — no runtime dependencies (no Python, no C++ libs)

## Usage

### Basic

```sh
appimagetool /path/to/AppDir
```

### With options

```sh
appimagetool \
  --appdir ./build/AppDir \
  --output ./dist \
  --update-info "gh-releases-zsync|org|repo|latest|*-x86_64.AppImage.zsync" \
  --arch x86_64
```

### All options

```
Usage: appimagetool [OPTIONS]

Options:
      --appdir <APPDIR>            Path to the AppDir directory [env: APPDIR] [default: ./AppDir]
  -o, --output <OUTPUT>            Output directory [env: OUTPATH] [default: .]
  -n, --name <NAME>                Output filename (auto-detected from .desktop if not set) [env: OUTNAME]
      --arch <ARCH>                Target architecture [env: ARCH]
      --runtime <RUNTIME>          Path to uruntime binary [env: RUNTIME]
      --runtime-url <RUNTIME_URL>  URL to download uruntime from [env: URUNTIME_LINK]
  -u, --update-info <UPDATE_INFO>  Update information string [env: UPINFO]
      --dwarfs-comp <DWARFS_COMP>  DWARFS compression options [env: DWARFS_COMP]
      --optimize-launch            Enable DWARFS profile optimization [env: OPTIMIZE_LAUNCH]
      --dwarfs-profile <PROFILE>   Path to DWARFS profile [env: DWARFSPROF]
      --mkdwarfs <MKDWARFS>        Path to mkdwarfs binary [env: DWARFS_CMD]
      --dwarfs-url <DWARFS_URL>    URL to download mkdwarfs from [env: DWARFS_LINK]
      --tmpdir <TMPDIR>            Temporary directory [env: TMPDIR] [default: /tmp]
  -h, --help                       Print help
  -V, --version                    Print version
```

### Environment variables

All CLI options have corresponding environment variables (shown above). Additional env vars:

| Variable | Description |
|---|---|
| `VERSION` | Version string embedded in the AppImage filename and metadata |
| `APPNAME` | Override the application name (default: from .desktop `Name`) |
| `GITHUB_REPOSITORY` | Auto-generates update info as `gh-releases-zsync\|owner\|repo\|latest\|*<arch>.AppImage.zsync` |
| `ADD_PERMA_ENV_VARS` | Permanent env vars to embed in the runtime (newline-separated) |
| `URUNTIME_PRELOAD` | Set to `1` to keep the FUSE mount after launch |
| `DEVEL_RELEASE` | Set to `1` to mark as a nightly/development build |
| `DWARFS_LINK` | Custom URL for downloading mkdwarfs |
| `DWARFSPROF` | Path to a pre-built DWARFS profile |

### AppDir requirements

The AppDir must contain:

1. **`AppRun`** — executable entry point script
2. **`.DirIcon`** — icon file (PNG or SVG)
3. **Exactly one `.desktop` file** in the root directory

### Output filename

By default the output filename is computed as:

```
<AppName>-<Version>-anylinux-<Arch>.AppImage
```

Where `AppName` comes from the .desktop `Name` key, `Version` from the `VERSION` env var, and `Arch` from `--arch` or auto-detected.

## Building

```sh
cargo build --release
```

### Running tests

```sh
# Unit + integration tests
cargo test --all-features

# Full end-to-end test (requires mkdwarfs + uruntime installed)
cargo test --all-features -- --ignored
```

## Architecture

```
src/
├── main.rs       # CLI entry point (clap)
├── lib.rs        # Library root
├── appimage.rs   # Build pipeline orchestration
├── config.rs     # Configuration from CLI args + env vars
├── desktop.rs    # .desktop file parsing and metadata
├── dwarfs.rs     # mkdwarfs resolution, image building, profiling
├── elf.rs        # ELF section read/write/patch
├── error.rs      # Error types with actionable hints
├── uruntime.rs   # Runtime download, caching, configuration
└── util.rs       # Download with retry, sanitize, ELF detection
```

## License

Same as the parent project.
