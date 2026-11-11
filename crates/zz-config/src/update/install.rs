mod download;
mod error;
mod mac_bundle;
mod tarball;

use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicBool, Ordering},
};

use semver::Version;
use serde::{Deserialize, Serialize};
use zz_daemon_client::unmasked::SpawnUnmasked as _;

pub use error::UpdateError;

use super::{Release, ReleaseAsset};

const LOG_TARGET: &str = "zz::update";
const BUNDLE_NAME: &str = "zz.app";
const MARKER_FILE_NAME: &str = "staged.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Target {
    MacBundle { app: PathBuf },
    LinuxTarball { prefix: PathBuf },
    AppImage { path: PathBuf },
}

impl Target {
    fn staging(&self) -> PathBuf {
        match self {
            Self::MacBundle { app } => app.with_file_name(".zz-update.app"),
            Self::LinuxTarball { prefix } => prefix.join("lib/.zz-update"),
            Self::AppImage { path } => path.with_file_name(hidden_sibling(path, "update")),
        }
    }

    fn aside(&self) -> PathBuf {
        match self {
            Self::MacBundle { app } => app.with_file_name(".zz-old.app"),
            Self::LinuxTarball { prefix } => prefix.join("lib/.zz-old"),
            Self::AppImage { path } => path.with_file_name(hidden_sibling(path, "old")),
        }
    }

    fn probe(&self) -> PathBuf {
        let name = format!(".zz-update-probe-{}", std::process::id());
        match self {
            Self::MacBundle { app: path } | Self::AppImage { path } => path.with_file_name(name),
            Self::LinuxTarball { prefix } => prefix.join("lib").join(name),
        }
    }
}

fn hidden_sibling(path: &Path, suffix: &str) -> OsString {
    let mut name = OsString::from(".");
    name.push(path.file_name().unwrap_or(OsStr::new("zz.AppImage")));
    name.push(".");
    name.push(suffix);
    name
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub os: &'static str,
    pub executable: PathBuf,
    pub appimage: Option<PathBuf>,
    pub homebrew: bool,
}

impl Facts {
    pub fn current() -> Option<Self> {
        Some(Self {
            os: env::consts::OS,
            executable: env::current_exe().ok()?,
            appimage: env::var_os("APPIMAGE")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute()),
            homebrew: homebrew_cask().is_some(),
        })
    }
}

pub fn detect() -> Option<Target> {
    target_of(&Facts::current()?).filter(|target| writable(&target.probe()))
}

pub fn target_of(facts: &Facts) -> Option<Target> {
    match facts.os {
        "macos" if !facts.homebrew => {
            mac_bundle(&facts.executable).map(|app| Target::MacBundle { app })
        }
        "linux" => match &facts.appimage {
            Some(path) => Some(Target::AppImage { path: path.clone() }),
            None => tarball_prefix(&facts.executable).map(|prefix| Target::LinuxTarball {
                prefix: prefix.to_path_buf(),
            }),
        },
        _ => None,
    }
}

fn mac_bundle(executable: &Path) -> Option<PathBuf> {
    if executable
        .components()
        .any(|component| component.as_os_str() == "AppTranslocation")
    {
        return None;
    }
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.file_name()? == BUNDLE_NAME)
        .then(|| app.to_path_buf())
}

pub fn tarball_prefix(executable: &Path) -> Option<&Path> {
    let prefix = executable.parent()?.parent()?.parent()?;
    (executable.ends_with("lib/zz/zz") && prefix != Path::new("/usr")).then_some(prefix)
}

pub fn homebrew_cask() -> Option<(PathBuf, &'static str)> {
    ["/opt/homebrew", "/usr/local"]
        .into_iter()
        .map(PathBuf::from)
        .find_map(|prefix| {
            ["zz", "zz@beta"]
                .into_iter()
                .find(|cask| prefix.join("Caskroom").join(cask).is_dir())
                .map(|cask| (prefix, cask))
        })
}

fn writable(probe: &Path) -> bool {
    match fs::create_dir(probe) {
        Ok(()) => fs::remove_dir(probe).is_ok(),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => fs::remove_dir(probe).is_ok(),
        Err(_) => false,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assets {
    pub file: ReleaseAsset,
    pub checksum: ReleaseAsset,
}

pub fn assets_for(release: &Release, target: &Target) -> Option<Assets> {
    assets_for_arch(release, target, env::consts::ARCH)
}

fn assets_for_arch(release: &Release, target: &Target, arch: &str) -> Option<Assets> {
    let version = &release.version;
    let linux_arch = matches!(arch, "x86_64" | "aarch64").then_some(arch);
    let name = match target {
        Target::MacBundle { .. } => {
            (arch == "aarch64").then(|| format!("zz-{version}-macos-arm64.dmg"))
        }
        Target::LinuxTarball { .. } => {
            linux_arch.map(|arch| format!("zz-{version}-linux-{arch}.tar.gz"))
        }
        Target::AppImage { .. } => {
            linux_arch.map(|arch| format!("zz-{version}-linux-{arch}.AppImage"))
        }
    }?;
    let checksum = format!("{name}.sha256");
    let find = |wanted: &str| {
        release
            .assets
            .iter()
            .find(|asset| asset.name == wanted)
            .cloned()
    };
    Some(Assets {
        file: find(&name)?,
        checksum: find(&checksum)?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Downloading { received: u64, total: Option<u64> },
    Verifying,
    Staging,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged {
    pub version: Version,
    pub target: Target,
    pub path: PathBuf,
}

pub fn prepare(
    release: &Release,
    target: &Target,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(Progress),
) -> Result<Staged, UpdateError> {
    let assets = assets_for(release, target).ok_or(UpdateError::Unavailable)?;
    let probe = target.probe();
    if !writable(&probe) {
        return Err(UpdateError::NotWritable(
            probe.parent().unwrap_or(&probe).to_path_buf(),
        ));
    }
    let directory = update_directory()?;
    remove_path(&directory.join(MARKER_FILE_NAME))?;
    fs::create_dir_all(&directory)
        .map_err(|source| UpdateError::io("create", &directory, source))?;

    let expected = download::expected_checksum(&assets.checksum.url)?;
    let part = directory.join(format!("{}.part", assets.file.name));
    let actual = download::fetch(&assets.file.url, &part, cancel, &mut on_progress)?;
    on_progress(Progress::Verifying);
    download::verify(&part, &expected, &actual)?;
    if cancel.load(Ordering::Relaxed) {
        let _ = fs::remove_file(&part);
        return Err(UpdateError::Cancelled);
    }

    on_progress(Progress::Staging);
    let file = directory.join(&assets.file.name);
    fs::rename(&part, &file).map_err(|source| UpdateError::io("move", &part, source))?;
    let staged = Staged {
        version: release.version.clone(),
        target: target.clone(),
        path: target.staging(),
    };
    let result = match target {
        Target::MacBundle { app } => mac_bundle::stage(app, &file, &staged.path, &directory),
        Target::LinuxTarball { prefix } => tarball::stage(prefix, &file, &staged.path),
        Target::AppImage { .. } => stage_appimage(&file, &staged.path),
    };
    let _ = remove_path(&file);
    result?;
    write_marker(&directory, &staged)?;
    Ok(staged)
}

fn stage_appimage(download: &Path, staged: &Path) -> Result<(), UpdateError> {
    remove_path(staged)?;
    move_entry(download, staged)?;
    make_executable(staged).map_err(|source| UpdateError::io("mark executable", staged, source))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub fn apply(staged: &Staged) -> Result<PathBuf, UpdateError> {
    if fs::symlink_metadata(&staged.path).is_err() {
        return Err(UpdateError::io(
            "find the staged update at",
            &staged.path,
            io::Error::from(ErrorKind::NotFound),
        ));
    }
    let relaunch = match &staged.target {
        Target::MacBundle { app } => {
            swap(app, &staged.path, &staged.target.aside())?;
            mac_bundle::link_cli(&app.join("Contents/MacOS/cli"));
            app.clone()
        }
        Target::LinuxTarball { prefix } => {
            let relaunch = tarball::install(prefix, &staged.path)?;
            tarball::refresh_desktop_database(prefix);
            relaunch
        }
        Target::AppImage { path } => {
            swap(path, &staged.path, &staged.target.aside())?;
            path.clone()
        }
    };
    if let Ok(directory) = update_directory()
        && let Err(error) = remove_path(&directory.join(MARKER_FILE_NAME))
    {
        log::warn!(target: LOG_TARGET, "{error}");
    }
    Ok(relaunch)
}

pub fn staged() -> Option<Staged> {
    read_marker(&update_directory().ok()?)
        .filter(|staged| fs::symlink_metadata(&staged.path).is_ok())
}

pub fn staged_for(version: &Version, target: &Target) -> Option<Staged> {
    staged().filter(|staged| staged.version == *version && staged.target == *target)
}

pub fn clear_staged() -> Result<(), UpdateError> {
    let directory = update_directory()?;
    if let Some(staged) = read_marker(&directory) {
        remove_path(&staged.path)?;
    }
    remove_path(&directory)
}

#[derive(Serialize, Deserialize)]
struct Marker {
    version: String,
    target: Target,
    path: PathBuf,
}

fn read_marker(directory: &Path) -> Option<Staged> {
    parse_marker(&fs::read_to_string(directory.join(MARKER_FILE_NAME)).ok()?)
}

fn parse_marker(text: &str) -> Option<Staged> {
    let marker: Marker = serde_json::from_str(text).ok()?;
    if marker.path != marker.target.staging() {
        return None;
    }
    Some(Staged {
        version: Version::parse(&marker.version).ok()?,
        target: marker.target,
        path: marker.path,
    })
}

fn write_marker(directory: &Path, staged: &Staged) -> Result<(), UpdateError> {
    let path = directory.join(MARKER_FILE_NAME);
    let marker = Marker {
        version: staged.version.to_string(),
        target: staged.target.clone(),
        path: staged.path.clone(),
    };
    let json = serde_json::to_vec_pretty(&marker).map_err(|error| {
        UpdateError::io(
            "write",
            &path,
            io::Error::new(ErrorKind::InvalidData, error),
        )
    })?;
    crate::atomic_write(&path, &json).map_err(|source| UpdateError::io("write", &path, source))
}

fn update_directory() -> Result<PathBuf, UpdateError> {
    cache_root()
        .map(|root| {
            root.join(zz_protocol::app_identity::DIRECTORY)
                .join("update")
        })
        .ok_or(UpdateError::NoCacheDirectory)
}

fn cache_root() -> Option<PathBuf> {
    let absolute = |key: &str| {
        env::var_os(key)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    if cfg!(target_os = "macos") {
        absolute("HOME").map(|home| home.join("Library/Caches"))
    } else if cfg!(windows) {
        absolute("LOCALAPPDATA")
    } else {
        absolute("XDG_CACHE_HOME").or_else(|| absolute("HOME").map(|home| home.join(".cache")))
    }
}

fn run(command: &mut Command) -> Result<Output, UpdateError> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .output_unmasked()
        .map_err(|source| UpdateError::io("run", &program, source))?;
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned();
    Err(UpdateError::Tool {
        program,
        status: output.status,
        stderr,
    })
}

fn swap(current: &Path, staged: &Path, aside: &Path) -> Result<(), UpdateError> {
    remove_path(aside)?;
    let moved = match fs::rename(current, aside) {
        Ok(()) => true,
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(source) => return Err(UpdateError::io("move aside", current, source)),
    };
    if let Err(source) = fs::rename(staged, current) {
        if moved && let Err(error) = fs::rename(aside, current) {
            log::error!(
                target: LOG_TARGET,
                "could not put {} back from {}: {error}",
                current.display(),
                aside.display()
            );
        }
        return Err(UpdateError::io("move the update to", current, source));
    }
    if let Err(error) = remove_path(aside) {
        log::warn!(target: LOG_TARGET, "{error}");
    }
    Ok(())
}

fn move_entry(from: &Path, to: &Path) -> Result<(), UpdateError> {
    match fs::rename(from, to) {
        Err(error) if error.kind() == ErrorKind::CrossesDevices => copy_entry(from, to)
            .and_then(|()| fs::remove_file(from))
            .map_err(|source| UpdateError::io("move", from, source)),
        result => result.map_err(|source| UpdateError::io("move", from, source)),
    }
}

#[cfg(unix)]
fn copy_entry(from: &Path, to: &Path) -> io::Result<()> {
    if fs::symlink_metadata(from)?.file_type().is_symlink() {
        let target = fs::read_link(from)?;
        let _ = fs::remove_file(to);
        std::os::unix::fs::symlink(target, to)
    } else {
        fs::copy(from, to).map(drop)
    }
}

#[cfg(not(unix))]
fn copy_entry(from: &Path, to: &Path) -> io::Result<()> {
    fs::copy(from, to).map(drop)
}

fn remove_path(path: &Path) -> Result<(), UpdateError> {
    let removed = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => Err(error),
    };
    removed.map_err(|source| UpdateError::io("remove", path, source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).expect("valid version")
    }

    fn mac(executable: &str) -> Facts {
        Facts {
            os: "macos",
            executable: PathBuf::from(executable),
            ..Facts::default()
        }
    }

    fn linux(executable: &str) -> Facts {
        Facts {
            os: "linux",
            executable: PathBuf::from(executable),
            ..Facts::default()
        }
    }

    #[test]
    fn detection_finds_only_installs_the_app_can_replace() {
        let translocated = mac(
            "/private/var/folders/x1/abc/T/AppTranslocation/0A1B2C3D-4E5F/d/zz.app/Contents/MacOS/zz",
        );
        let applications = mac("/Applications/zz.app/Contents/MacOS/zz");
        let brewed = Facts {
            homebrew: true,
            ..applications.clone()
        };
        let appimage = Facts {
            appimage: Some(PathBuf::from("/home/me/Apps/zz.AppImage")),
            ..linux("/tmp/.mount_zzAbC/usr/lib/zz/zz")
        };
        let cases = [
            (translocated, None),
            (
                applications,
                Some(Target::MacBundle {
                    app: PathBuf::from("/Applications/zz.app"),
                }),
            ),
            (brewed, None),
            (mac("/Applications/zz Dev.app/Contents/MacOS/zz"), None),
            (linux("/usr/lib/zz/zz"), None),
            (
                linux("/home/me/.local/lib/zz/zz"),
                Some(Target::LinuxTarball {
                    prefix: PathBuf::from("/home/me/.local"),
                }),
            ),
            (
                appimage,
                Some(Target::AppImage {
                    path: PathBuf::from("/home/me/Apps/zz.AppImage"),
                }),
            ),
            (
                Facts {
                    os: "windows",
                    ..linux("C:/Program Files/zz/lib/zz/zz")
                },
                None,
            ),
        ];
        for (facts, expected) in cases {
            assert_eq!(target_of(&facts), expected, "{facts:?}");
        }
    }

    #[test]
    fn each_target_takes_its_own_asset_and_needs_its_checksum() {
        let names = [
            "zz-0.23.0-macos-arm64.dmg",
            "zz-0.23.0-macos-arm64.dmg.sha256",
            "zz-0.23.0-linux-x86_64.tar.gz",
            "zz-0.23.0-linux-x86_64.tar.gz.sha256",
            "zz-0.23.0-linux-aarch64.tar.gz",
            "zz-0.23.0-linux-aarch64.AppImage",
            "zz-0.23.0-linux-aarch64.AppImage.sha256",
            "zz-0.23.0-linux-x86_64.deb",
            "zz-0.23.0-linux-x86_64.deb.sha256",
        ];
        let release = Release {
            version: version("0.23.0"),
            url: "https://github.com/autistic-computer/zebrilus/releases/tag/v0.23.0".to_owned(),
            assets: names
                .iter()
                .map(|name| ReleaseAsset {
                    name: (*name).to_owned(),
                    url: format!(
                        "https://github.com/autistic-computer/zebrilus/releases/download/v0.23.0/{name}"
                    ),
                })
                .collect(),
            wire: None,
        };
        let bundle = Target::MacBundle {
            app: PathBuf::from("/Applications/zz.app"),
        };
        let tarball = Target::LinuxTarball {
            prefix: PathBuf::from("/home/me/.local"),
        };
        let appimage = Target::AppImage {
            path: PathBuf::from("/home/me/Apps/zz.AppImage"),
        };
        let chosen = |target: &Target, arch: &str| {
            assets_for_arch(&release, target, arch)
                .map(|assets| (assets.file.name, assets.checksum.name))
        };
        let pair = |name: &str| Some((name.to_owned(), format!("{name}.sha256")));

        assert_eq!(
            chosen(&bundle, "aarch64"),
            pair("zz-0.23.0-macos-arm64.dmg")
        );
        assert_eq!(chosen(&bundle, "x86_64"), None);
        assert_eq!(
            chosen(&tarball, "x86_64"),
            pair("zz-0.23.0-linux-x86_64.tar.gz")
        );
        assert_eq!(chosen(&tarball, "aarch64"), None);
        assert_eq!(
            chosen(&appimage, "aarch64"),
            pair("zz-0.23.0-linux-aarch64.AppImage")
        );
        assert_eq!(chosen(&appimage, "x86_64"), None);
        assert_eq!(chosen(&tarball, "riscv64"), None);
    }
}
