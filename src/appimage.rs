use std::io::Write;
use std::path::Path;

use zsync_rs::ControlFile;

use crate::config::Config;
use crate::desktop::{self, DesktopEntry};
use crate::dwarfs;
use crate::error::Result;
use crate::uruntime;
use crate::util;

/// Run the full AppImage build pipeline.
pub fn build(config: &Config) -> Result<()> {
    // Validate AppDir
    DesktopEntry::check_apprun(&config.appdir)?;
    DesktopEntry::check_dir_icon(&config.appdir)?;

    // Sort .env: move all "unset" lines to the end.
    // The dotenv library used by sharun requires unset lines last.
    sort_env_file(&config.appdir)?;

    // Parse desktop entry
    let desktop = DesktopEntry::from_appdir(&config.appdir)?;

    // Determine app name: APPNAME env var overrides desktop entry.
    // Sanitize the same way the shell script does.
    let app_name_raw = std::env::var("APPNAME")
        .ok()
        .unwrap_or_else(|| desktop.name.clone());
    let app_name = util::sanitize_filename(&app_name_raw);
    let version = config.version.as_deref().unwrap_or("UNKNOWN");

    // Handle devel release: patch desktop entry and update info
    let update_info = if config.devel_release {
        let content = std::fs::read_to_string(&desktop.path)?;
        if !content.contains("Nightly") {
            let patched = content
                .lines()
                .map(|line| {
                    if line.starts_with("Name=") {
                        format!("{line} Nightly")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(&desktop.path, patched)?;
        }
        // Replace 'latest' with 'nightly' in update info
        config
            .update_info
            .as_ref()
            .map(|info| info.replace("|latest|", "|nightly|"))
    } else {
        config.update_info.clone()
    };

    // Add X-AppImage-* metadata to desktop entry
    desktop.add_appimage_metadata(&app_name, version, &config.arch)?;

    // Compute output filename
    let output_name = config
        .output_name
        .clone()
        .unwrap_or_else(|| desktop::compute_output_name(&app_name, Some(version), &config.arch));

    std::fs::create_dir_all(&config.output_dir)?;
    let output_path = config.output_dir.join(&output_name);

    // Resolve runtime
    let runtime_path = uruntime::resolve_runtime(config)?;
    eprintln!("Using runtime: {}", runtime_path.display());

    // Configure runtime (ELF section editing)
    uruntime::configure_runtime(
        &runtime_path,
        update_info.as_deref(),
        &config.env_vars,
        config.keep_mount,
    )?;

    // Resolve mkdwarfs
    let mkdwarfs = dwarfs::resolve_mkdwarfs(config)?;
    eprintln!("Using mkdwarfs: {}", mkdwarfs.display());

    // DWARFS profile optimization (optional)
    let profile = if config.optimize_launch {
        let tmp_appimage = config.tmpdir.join(".analyze");
        dwarfs::build_profile_image(&mkdwarfs, &config.appdir, &runtime_path, &tmp_appimage)?;

        // Make it executable
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_appimage, std::fs::Permissions::from_mode(0o755))?;

        let profile_path = config
            .dwarfs_profile
            .as_ref()
            .cloned()
            .unwrap_or_else(|| config.appdir.join(".dwarfsprofile"));

        dwarfs::run_profiling(&tmp_appimage, &profile_path, &config.tmpdir, 10)?;

        Some(profile_path)
    } else {
        config.dwarfs_profile.clone()
    };

    // Build the final AppImage
    dwarfs::build_appimage(
        &mkdwarfs,
        &config.appdir,
        &runtime_path,
        &output_path,
        &config.dwarfs_comp,
        profile.as_deref(),
    )?;

    // Make output executable
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&output_path, std::fs::Permissions::from_mode(0o755))?;

    // Generate zsync file if update info is set
    if update_info.is_some() {
        generate_zsync(&output_path, &output_name, &config.output_dir)?;
    }

    // Write appinfo file
    write_appinfo(&config.output_dir, &app_name, version, &config.arch)?;

    eprintln!("All done! AppImage at: {}", output_path.display());

    Ok(())
}

/// Generate a .zsync file using the zsync-rs library.
fn generate_zsync(appimage: &Path, filename: &str, output_dir: &Path) -> Result<()> {
    eprintln!("Generating .zsync file...");

    let mut file = std::fs::File::open(appimage)?;
    let control = ControlFile::generate(&mut file, filename, filename, None)?;

    let zsync_path = output_dir.join(format!("{filename}.zsync"));
    let mut out = std::fs::File::create(&zsync_path)?;
    control.write(&mut out)?;

    eprintln!("Wrote {}", zsync_path.display());
    Ok(())
}

/// Write an appinfo metadata file next to the output AppImage.
fn write_appinfo(output_dir: &Path, name: &str, version: &str, arch: &str) -> Result<()> {
    let path = output_dir.join("appinfo");
    let mut f = std::fs::File::create(&path)?;
    writeln!(f, "X-AppImage-Name={name}")?;
    writeln!(f, "X-AppImage-Version={version}")?;
    writeln!(f, "X-AppImage-Arch={arch}")?;
    Ok(())
}

/// Sort the AppDir's `.env` file so all lines starting with `unset`
/// come after every other line. The dotenv library used by sharun
/// requires this ordering.
fn sort_env_file(appdir: &Path) -> Result<()> {
    let env_path = appdir.join(".env");
    if !env_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&env_path)?;
    let mut regular = Vec::new();
    let mut unsets = Vec::new();

    for line in content.lines() {
        if line.starts_with("unset") {
            unsets.push(line);
        } else {
            regular.push(line);
        }
    }

    let mut sorted = String::new();
    for line in &regular {
        sorted.push_str(line);
        sorted.push('\n');
    }
    for line in &unsets {
        sorted.push_str(line);
        sorted.push('\n');
    }

    std::fs::write(&env_path, sorted)?;
    Ok(())
}
