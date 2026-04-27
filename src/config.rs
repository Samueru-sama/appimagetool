use std::env;
use std::path::PathBuf;

fn env_opt(name: &str) -> Option<String> {
    env::var(name).ok()
}

fn env_truthy(name: &str) -> bool {
    matches!(
        env_opt(name).as_deref().map(str::trim),
        Some("1" | "true" | "TRUE" | "True" | "yes" | "YES" | "Yes" | "on" | "ON" | "On")
    )
}

#[derive(Debug)]
pub struct Config {
    pub appdir: PathBuf,
    pub output_dir: PathBuf,
    pub output_name: Option<String>,
    pub arch: String,
    pub runtime: Option<PathBuf>,
    pub runtime_url: Option<String>,
    pub dwarfs_comp: String,
    pub update_info: Option<String>,
    pub env_vars: Vec<String>,
    pub dwarfs_profile: Option<PathBuf>,
    pub optimize_launch: bool,
    pub version: Option<String>,
    pub mkdwarfs: Option<PathBuf>,
    pub dwarfs_url: Option<String>,
    pub tmpdir: PathBuf,
    pub keep_mount: bool,
    pub devel_release: bool,
}

impl Config {
    #[allow(clippy::too_many_arguments)]
    pub fn from_cli_args(
        appdir: Option<PathBuf>,
        output: Option<PathBuf>,
        output_name: Option<String>,
        arch: Option<String>,
        runtime: Option<PathBuf>,
        runtime_url: Option<String>,
        update_info: Option<String>,
        dwarfs_comp: Option<String>,
        optimize_launch: bool,
        dwarfs_profile: Option<PathBuf>,
        mkdwarfs: Option<PathBuf>,
        dwarfs_url: Option<String>,
        tmpdir: Option<PathBuf>,
    ) -> crate::error::Result<Self> {
        // Env vars with clap `env =` are already resolved by the CLI.
        // These .or_else() calls handle the library case (no clap).

        let arch = arch.unwrap_or_else(|| match env::consts::ARCH {
            "x86_64" => "x86_64".to_string(),
            "aarch64" => "aarch64".to_string(),
            other => other.to_string(),
        });

        let appdir = appdir.unwrap_or_else(|| PathBuf::from("./AppDir"));
        let tmpdir = tmpdir.unwrap_or_else(|| PathBuf::from("/tmp"));

        let optimize_launch = optimize_launch || env_truthy("OPTIMIZE_LAUNCH");

        let keep_mount = env_truthy("URUNTIME_PRELOAD");
        let devel_release = env_truthy("DEVEL_RELEASE");

        let update_info = update_info.or_else(|| env_opt("UPINFO")).or_else(|| {
            env::var("GITHUB_REPOSITORY").ok().map(|repo| {
                let parts: Vec<&str> = repo.split('/').collect();
                if parts.len() == 2 {
                    format!(
                        "gh-releases-zsync|{}|{}|latest|*{arch}.AppImage.zsync",
                        parts[0], parts[1]
                    )
                } else {
                    repo
                }
            })
        });

        let env_vars = env_opt("ADD_PERMA_ENV_VARS")
            .map(|s| s.lines().map(|l| l.to_string()).collect())
            .unwrap_or_default();

        let dwarfs_profile = dwarfs_profile
            .or_else(|| env_opt("DWARFSPROF").map(PathBuf::from))
            .or_else(|| {
                if optimize_launch {
                    Some(appdir.join(".dwarfsprofile"))
                } else {
                    None
                }
            });

        let dwarfs_comp = dwarfs_comp.unwrap_or_else(|| "zstd:level=22 -S26 -B6".to_string());

        Ok(Config {
            appdir: appdir.clone(),
            output_dir: output.unwrap_or_else(|| PathBuf::from(".")),
            output_name,
            arch,
            runtime,
            runtime_url,
            dwarfs_comp,
            update_info,
            env_vars,
            dwarfs_profile,
            optimize_launch,
            version: env_opt("VERSION")
                .or_else(|| {
                    let v = dirs_home().join("version");
                    if v.exists() {
                        std::fs::read_to_string(&v)
                            .ok()
                            .map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
                .map(|v| strip_epoch(&v)),
            mkdwarfs,
            dwarfs_url: dwarfs_url.or_else(|| env_opt("DWARFS_LINK")),
            tmpdir,
            keep_mount,
            devel_release,
        })
    }
}

fn dirs_home() -> PathBuf {
    env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("~"))
}

/// Strip the epoch prefix from a version string.
/// Shell equivalent: `${VERSION#*:}` — removes everything up to
/// and including the first colon.  `"1:2.0.1"` → `"2.0.1"`.
fn strip_epoch(version: &str) -> String {
    match version.find(':') {
        Some(i) => version[i + 1..].to_string(),
        None => version.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_epoch() {
        assert_eq!(strip_epoch("1:2.0.1"), "2.0.1");
        assert_eq!(strip_epoch("2.0.1"), "2.0.1");
        assert_eq!(strip_epoch("1:"), "");
        assert_eq!(strip_epoch(""), "");
        assert_eq!(strip_epoch("0:1.0.0-alpha"), "1.0.0-alpha");
    }

    #[test]
    fn test_config_custom_compression() {
        let config = Config::from_cli_args(
            Some(PathBuf::from("/tmp/AppDir")),
            None,
            None,
            None,
            None,
            None,
            None,
            Some("zstd:level=1".to_string()),
            false,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(config.dwarfs_comp, "zstd:level=1");
    }

    #[test]
    fn test_config_update_info_passthrough() {
        let config = Config::from_cli_args(
            Some(PathBuf::from("/tmp/AppDir")),
            None,
            None,
            None,
            None,
            None,
            Some("gh-releases-zsync|org|repo|latest|*.AppImage.zsync".to_string()),
            None,
            false,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            config.update_info.as_deref(),
            Some("gh-releases-zsync|org|repo|latest|*.AppImage.zsync")
        );
    }

    #[test]
    fn test_config_output_name_override() {
        let config = Config::from_cli_args(
            Some(PathBuf::from("/tmp/AppDir")),
            None,
            Some("custom.AppImage".to_string()),
            None,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(config.output_name.as_deref(), Some("custom.AppImage"));
    }

    #[test]
    fn test_config_runtime_path() {
        let config = Config::from_cli_args(
            Some(PathBuf::from("/tmp/AppDir")),
            None,
            None,
            None,
            Some(PathBuf::from("/opt/uruntime")),
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            config.runtime.as_deref(),
            Some(std::path::Path::new("/opt/uruntime"))
        );
    }

    #[test]
    fn test_config_env_vars_from_string() {
        let vars: Vec<String> = "FOO=bar\nBAZ=qux".lines().map(|l| l.to_string()).collect();
        assert_eq!(vars, vec!["FOO=bar", "BAZ=qux"]);
    }

    #[test]
    fn test_dirs_home_returns_something() {
        let home = dirs_home();
        assert!(!home.as_os_str().is_empty());
    }
}
