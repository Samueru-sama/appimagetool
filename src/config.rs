use std::env;
use std::path::PathBuf;

fn env_opt(name: &str) -> Option<String> {
    env::var(name).ok()
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
        tmpdir: Option<PathBuf>,
    ) -> crate::error::Result<Self> {
        let arch = arch
            .or_else(|| env_opt("ARCH"))
            .unwrap_or_else(|| match env::consts::ARCH {
                "x86_64" => "x86_64".to_string(),
                "aarch64" => "aarch64".to_string(),
                other => other.to_string(),
            });

        let appdir = appdir
            .or_else(|| env_opt("APPDIR").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("./AppDir"));

        let tmpdir = tmpdir
            .or_else(|| env_opt("TMPDIR").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/tmp"));

        let optimize_launch = optimize_launch || env_opt("OPTIMIZE_LAUNCH").as_deref() == Some("1");

        let keep_mount = env_opt("URUNTIME_PRELOAD").as_deref() == Some("1");
        let devel_release = env_opt("DEVEL_RELEASE").as_deref() == Some("1");

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

        let dwarfs_comp = dwarfs_comp
            .or_else(|| env_opt("DWARFS_COMP"))
            .unwrap_or_else(|| "zstd:level=22 -S26 -B6".to_string());

        let runtime_url = runtime_url.or_else(|| env_opt("URUNTIME_LINK"));

        Ok(Config {
            appdir,
            output_dir: output
                .or_else(|| env_opt("OUTPATH").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from(".")),
            output_name: output_name.or_else(|| env_opt("OUTNAME")),
            arch,
            runtime: runtime.or_else(|| env_opt("RUNTIME").map(PathBuf::from)),
            runtime_url,
            dwarfs_comp,
            update_info,
            env_vars,
            dwarfs_profile,
            optimize_launch,
            version: env_opt("VERSION").or_else(|| {
                let v = dirs_home().join("version");
                if v.exists() {
                    std::fs::read_to_string(&v)
                        .ok()
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            }),
            mkdwarfs: mkdwarfs.or_else(|| env_opt("DWARFS_CMD").map(PathBuf::from)),
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
