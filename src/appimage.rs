use std::io::Write;
use std::path::Path;

use crate::config::Config;
use crate::desktop::{self, DesktopEntry};
use crate::dwarfs;
use crate::error::Result;
use crate::uruntime;

/// Run the full AppImage build pipeline.
pub fn build(config: &Config) -> Result<()> {
    // Validate AppDir
    DesktopEntry::check_apprun(&config.appdir)?;
    DesktopEntry::check_dir_icon(&config.appdir)?;

    // Parse desktop entry
    let desktop = DesktopEntry::from_appdir(&config.appdir)?;

    // Determine app name and version
    let app_name = desktop.name.clone();
    let version = config.version.as_deref().unwrap_or("UNKNOWN");

    // Handle devel release: patch desktop entry
    if config.devel_release {
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
    }

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
    uruntime::configure_runtime(&runtime_path, config)?;

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
    if config.update_info.is_some() {
        generate_zsync(&output_path, &output_name, &config.output_dir)?;
    }

    // Write appinfo file
    write_appinfo(&config.output_dir, &app_name, version, &config.arch)?;

    eprintln!("All done! AppImage at: {}", output_path.display());

    Ok(())
}

/// Generate a .zsync file using the zsyncmake binary.
/// TODO: Replace with zsync-rs library in Phase 4.
fn generate_zsync(appimage: &Path, filename: &str, output_dir: &Path) -> Result<()> {
    eprintln!("Generating .zsync file...");

    let status = std::process::Command::new("zsyncmake")
        .arg("-u")
        .arg(filename)
        .arg(appimage)
        .current_dir(output_dir)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!(
                "WARNING: zsyncmake failed (status {}), skipping .zsync generation",
                s.code().unwrap_or(-1)
            );
            return Ok(());
        }
        Err(_) => {
            eprintln!("WARNING: zsyncmake not found, skipping .zsync generation");
            return Ok(());
        }
    }

    // zsyncmake sometimes places the .zsync file in PWD instead of alongside the input
    let zsync_in_pwd = std::path::PathBuf::from(format!("{filename}.zsync"));
    let zsync_expected = output_dir.join(format!("{filename}.zsync"));
    if zsync_in_pwd.exists() && !zsync_expected.exists() {
        std::fs::rename(&zsync_in_pwd, &zsync_expected)?;
    }

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
