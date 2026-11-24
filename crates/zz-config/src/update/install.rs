mod deb;
mod homebrew;
mod mac_bundle;
mod tarball;
mod windows_setup;

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

use super::{Channel, Progress, Release, ReleaseAsset, UpdateError, download};

const LOG_TARGET: &str = "zz::update";
const BUNDLE_NAME: &str = "zz.app";
const MARKER_FILE_NAME: &str = "staged.json";
const INSTALL_SCRIPT_URL: &str = "https://zzmux.sh/install.sh";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Target {
    MacBundle {
        app: PathBuf,
        #[serde(default)]
        elevated: bool,
    },
    Homebrew {
        brew: PathBuf,
        cask: String,
        app: PathBuf,
    },
    LinuxTarball {
        prefix: PathBuf,
    },
    AppImage {
        path: PathBuf,
    },
    LinuxDeb {
        executable: PathBuf,
    },
    WindowsSetup,
}

impl Target {
    fn staging(&self, directory: &Path) -> PathBuf {
        match self {
            Self::MacBundle {
                app,
                elevated: false,
            } => app.with_file_name(".zz-update.app"),
            Self::MacBundle { elevated: true, .. } => directory.join(".zz-update.app"),
            Self::Homebrew { app, .. } => app.clone(),
            Self::LinuxTarball { prefix } => prefix.join("lib/.zz-update"),
            Self::AppImage { path } => path.with_file_name(hidden_sibling(path, "update")),
            Self::LinuxDeb { .. } => directory.join("zz-update.deb"),
            Self::WindowsSetup => directory.join("zz-update-setup.exe"),
        }
    }

    fn aside(&self) -> Option<PathBuf> {
        match self {
            Self::MacBundle { app, .. } => Some(app.with_file_name(".zz-old.app")),
            Self::LinuxTarball { prefix } => Some(prefix.join("lib/.zz-old")),
            Self::AppImage { path } => Some(path.with_file_name(hidden_sibling(path, "old"))),
            Self::Homebrew { .. } | Self::LinuxDeb { .. } | Self::WindowsSetup => None,
        }
    }

    fn probe(&self) -> Option<PathBuf> {
        let name = format!(".zz-update-probe-{}", std::process::id());
        match self {
            Self::MacBundle {
                app: path,
                elevated: false,
            }
            | Self::AppImage { path } => Some(path.with_file_name(name)),
            Self::LinuxTarball { prefix } => Some(prefix.join("lib").join(name)),
            Self::MacBundle { elevated: true, .. }
            | Self::Homebrew { .. }
            | Self::LinuxDeb { .. }
            | Self::WindowsSetup => None,
        }
    }

    const fn owns_staged_path(&self) -> bool {
        !matches!(self, Self::Homebrew { .. })
    }

    pub fn available_for(&self, release: &Release) -> bool {
        self.available_for_arch(release, env::consts::ARCH)
    }

    fn available_for_arch(&self, release: &Release, arch: &str) -> bool {
        match self {
            Self::Homebrew { .. } => arch == "aarch64",
            _ => assets_for_arch(release, self, arch).is_some(),
        }
    }

    pub fn fallback_command(&self) -> Option<String> {
        match self {
            Self::Homebrew { brew, cask, .. } => {
                Some(format!("{} upgrade --cask {cask}", brew.display()))
            }
            Self::MacBundle { .. }
            | Self::LinuxTarball { .. }
            | Self::AppImage { .. }
            | Self::LinuxDeb { .. }
            | Self::WindowsSetup => None,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualUpdate {
    pub owner: String,
    pub command: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallPlan {
    InApp(Target),
    Manual(ManualUpdate),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyOutcome {
    Relaunch(PathBuf),
    Quit,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub os: &'static str,
    pub arch: &'static str,
    pub executable: PathBuf,
    pub appimage: Option<PathBuf>,
    pub homebrew: Option<(PathBuf, String)>,
    pub deb_owned: bool,
    pub pacman_owned: bool,
    pub aur_helper: Option<String>,
    pub pkexec: bool,
    pub is_root: bool,
    pub writable_target: bool,
}

impl Facts {
    pub fn current() -> Option<Self> {
        let os = env::consts::OS;
        let arch = env::consts::ARCH;
        let executable = env::current_exe().ok()?;
        let appimage = env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute());
        let homebrew = (os == "macos")
            .then(homebrew_cask)
            .flatten()
            .filter(|(prefix, _)| homebrew::brew_binary(prefix).is_some())
            .map(|(prefix, cask)| (prefix, cask.to_owned()));
        let pacman_owned = os == "linux" && is_pacman_owned();
        let deb_owned = os == "linux" && !pacman_owned && deb::is_dpkg_owned(&executable);
        let aur_helper = (os == "linux")
            .then(|| ["paru", "yay"].into_iter().find(|helper| on_path(helper)))
            .flatten()
            .map(str::to_owned);
        let pkexec = os == "linux" && on_path("pkexec");
        let is_root = deb::is_root();
        let writable_target = probe_writable(os, &executable, appimage.as_deref());
        Some(Self {
            os,
            arch,
            executable,
            appimage,
            homebrew,
            deb_owned,
            pacman_owned,
            aur_helper,
            pkexec,
            is_root,
            writable_target,
        })
    }
}

fn probe_writable(os: &str, executable: &Path, appimage: Option<&Path>) -> bool {
    let name = format!(".zz-update-probe-{}", std::process::id());
    let probe = match os {
        "macos" => mac_bundle(executable).map(|app| app.with_file_name(name)),
        "linux" => match appimage {
            Some(path) => Some(path.with_file_name(name)),
            None => tarball_prefix(executable).map(|prefix| prefix.join("lib").join(name)),
        },
        _ => None,
    };
    probe.is_some_and(|path| writable(&path))
}

fn is_pacman_owned() -> bool {
    fs::read_dir("/var/lib/pacman/local").is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("zz-bin-") || name.starts_with("zz-beta-bin-")
        })
    })
}

fn on_path(program: &str) -> bool {
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|dir| homebrew::executable(&dir.join(program)))
    })
}

pub fn detect() -> Option<Target> {
    target_of(&Facts::current()?)
}

pub fn plan(release: &Release, channel: Channel) -> InstallPlan {
    let facts = Facts::current().unwrap_or_else(|| Facts {
        os: env::consts::OS,
        arch: env::consts::ARCH,
        ..Facts::default()
    });
    plan_for(release, channel, &facts)
}

pub fn plan_for(release: &Release, channel: Channel, facts: &Facts) -> InstallPlan {
    if let Some(target) = target_of(facts)
        && target.available_for_arch(release, facts.arch)
    {
        return InstallPlan::InApp(target);
    }
    InstallPlan::Manual(manual_of(facts, channel))
}

pub fn target_of(facts: &Facts) -> Option<Target> {
    match facts.os {
        "macos" => {
            let app = mac_bundle(&facts.executable)?;
            match &facts.homebrew {
                Some((prefix, cask)) => Some(Target::Homebrew {
                    brew: prefix.join("bin/brew"),
                    cask: cask.clone(),
                    app,
                }),
                None => Some(Target::MacBundle {
                    app,
                    elevated: !facts.writable_target,
                }),
            }
        }
        "linux" => {
            if let Some(path) = &facts.appimage {
                return facts
                    .writable_target
                    .then(|| Target::AppImage { path: path.clone() });
            }
            if facts.pacman_owned {
                return None;
            }
            if facts.deb_owned {
                return (facts.is_root || facts.pkexec).then(|| Target::LinuxDeb {
                    executable: facts.executable.clone(),
                });
            }
            tarball_prefix(&facts.executable).and_then(|prefix| {
                facts.writable_target.then(|| Target::LinuxTarball {
                    prefix: prefix.to_path_buf(),
                })
            })
        }
        "windows" => (facts.arch == "x86_64").then_some(Target::WindowsSetup),
        _ => None,
    }
}

pub fn manual_of(facts: &Facts, channel: Channel) -> ManualUpdate {
    match facts.os {
        "macos" => {
            if let Some((prefix, cask)) = &facts.homebrew {
                ManualUpdate {
                    owner: "Homebrew".to_owned(),
                    command: Some(format!(
                        "{}/bin/brew upgrade --cask {cask}",
                        prefix.display()
                    )),
                }
            } else {
                let owner = if is_translocated(&facts.executable) {
                    "a translocated app bundle"
                } else {
                    "macOS"
                };
                ManualUpdate {
                    owner: owner.to_owned(),
                    command: (facts.arch == "aarch64")
                        .then(|| install_script_command(channel, None)),
                }
            }
        }
        "linux" if facts.appimage.is_some() => ManualUpdate {
            owner: "AppImage".to_owned(),
            command: None,
        },
        "linux" if facts.pacman_owned => {
            let package = match channel {
                Channel::Stable => "zz-bin",
                Channel::Beta => "zz-beta-bin",
            };
            ManualUpdate {
                owner: format!("pacman ({package})"),
                command: facts
                    .aur_helper
                    .as_deref()
                    .map(|helper| format!("{helper} -S {package}")),
            }
        }
        "linux" if facts.deb_owned => ManualUpdate {
            owner: "apt (zz)".to_owned(),
            command: Some(install_script_command(channel, None)),
        },
        "linux" if facts.executable.starts_with("/nix/store") => ManualUpdate {
            owner: "Nix".to_owned(),
            command: None,
        },
        "linux" => ManualUpdate {
            owner: "this install".to_owned(),
            command: Some(install_script_command(
                channel,
                tarball_prefix(&facts.executable),
            )),
        },
        "windows" => ManualUpdate {
            owner: "Windows".to_owned(),
            command: None,
        },
        _ => ManualUpdate {
            owner: "this platform".to_owned(),
            command: None,
        },
    }
}

fn install_script_command(channel: Channel, prefix: Option<&Path>) -> String {
    let mut arguments = Vec::new();
    if channel == Channel::Beta {
        arguments.push("--beta".to_owned());
    }
    if let Some(prefix) = prefix {
        arguments.push(format!("--prefix '{}'", prefix.display()));
    }
    if arguments.is_empty() {
        format!("curl -fsSL {INSTALL_SCRIPT_URL} | sh")
    } else {
        format!(
            "curl -fsSL {INSTALL_SCRIPT_URL} | sh -s -- {}",
            arguments.join(" ")
        )
    }
}

fn is_translocated(executable: &Path) -> bool {
    executable
        .components()
        .any(|component| component.as_os_str() == "AppTranslocation")
}

fn mac_bundle(executable: &Path) -> Option<PathBuf> {
    if is_translocated(executable) {
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
        Target::Homebrew { .. } => None,
        Target::LinuxTarball { .. } => {
            linux_arch.map(|arch| format!("zz-{version}-linux-{arch}.tar.gz"))
        }
        Target::AppImage { .. } => {
            linux_arch.map(|arch| format!("zz-{version}-linux-{arch}.AppImage"))
        }
        Target::LinuxDeb { .. } => linux_arch.map(|arch| format!("zz-{version}-linux-{arch}.deb")),
        Target::WindowsSetup => {
            (arch == "x86_64").then(|| format!("zz-{version}-windows-x64-setup.exe"))
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
    let directory = update_directory()?;
    if let Target::Homebrew { brew, cask, app } = target {
        homebrew::upgrade(brew, cask, app, &release.version, cancel, &mut on_progress)?;
        fs::create_dir_all(&directory)
            .map_err(|source| UpdateError::io("create", &directory, source))?;
        let staged = Staged {
            version: release.version.clone(),
            target: target.clone(),
            path: app.clone(),
        };
        write_marker(&directory, &staged)?;
        return Ok(staged);
    }

    let assets = assets_for(release, target).ok_or(UpdateError::Unavailable)?;
    if let Some(probe) = target.probe()
        && !writable(&probe)
    {
        return Err(UpdateError::NotWritable(
            probe.parent().unwrap_or(&probe).to_path_buf(),
        ));
    }
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
        path: target.staging(&directory),
    };
    let result = match target {
        Target::MacBundle { app, .. } => mac_bundle::stage(app, &file, &staged.path, &directory),
        Target::LinuxTarball { prefix } => tarball::stage(prefix, &file, &staged.path),
        Target::AppImage { .. } => stage_appimage(&file, &staged.path),
        Target::LinuxDeb { .. } => deb::stage(&file, &staged.path),
        Target::WindowsSetup => windows_setup::stage(&file, &staged.path),
        Target::Homebrew { .. } => unreachable!(),
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

pub fn apply(staged: &Staged) -> Result<ApplyOutcome, UpdateError> {
    if fs::symlink_metadata(&staged.path).is_err() {
        return Err(UpdateError::io(
            "find the staged update at",
            &staged.path,
            io::Error::from(ErrorKind::NotFound),
        ));
    }
    let outcome = match &staged.target {
        Target::MacBundle {
            app,
            elevated: false,
        } => {
            let aside = staged.target.aside().expect("mac bundle has an aside path");
            swap(app, &staged.path, &aside)?;
            mac_bundle::link_cli(&app.join("Contents/MacOS/cli"));
            ApplyOutcome::Relaunch(app.clone())
        }
        Target::MacBundle {
            app,
            elevated: true,
        } => {
            let aside = staged.target.aside().expect("mac bundle has an aside path");
            mac_bundle::elevated_swap(app, &staged.path, &aside)?;
            mac_bundle::link_cli(&app.join("Contents/MacOS/cli"));
            ApplyOutcome::Relaunch(app.clone())
        }
        Target::Homebrew { app, .. } => ApplyOutcome::Relaunch(app.clone()),
        Target::LinuxTarball { prefix } => {
            let relaunch = tarball::install(prefix, &staged.path)?;
            tarball::refresh_desktop_database(prefix);
            ApplyOutcome::Relaunch(relaunch)
        }
        Target::AppImage { path } => {
            let aside = staged.target.aside().expect("appimage has an aside path");
            swap(path, &staged.path, &aside)?;
            ApplyOutcome::Relaunch(path.clone())
        }
        Target::LinuxDeb { executable } => {
            deb::install(&staged.path)?;
            ApplyOutcome::Relaunch(executable.clone())
        }
        Target::WindowsSetup => {
            windows_setup::launch(&staged.path)?;
            ApplyOutcome::Quit
        }
    };
    if let Ok(directory) = update_directory()
        && let Err(error) = remove_path(&directory.join(MARKER_FILE_NAME))
    {
        log::warn!(target: LOG_TARGET, "{error}");
    }
    Ok(outcome)
}

pub fn staged() -> Option<Staged> {
    read_marker(&update_directory().ok()?)
        .filter(|staged| fs::symlink_metadata(&staged.path).is_ok())
}

pub fn staged_for(version: &Version, target: &Target) -> Option<Staged> {
    staged().filter(|staged| staged.version == *version && staged.target == *target)
}

pub fn clear_staged() -> Result<(), UpdateError> {
    clear_staged_in(&update_directory()?)
}

fn clear_staged_in(directory: &Path) -> Result<(), UpdateError> {
    if let Some(staged) = read_marker(directory)
        && staged.target.owns_staged_path()
    {
        remove_path(&staged.path)?;
    }
    remove_path(directory)
}

#[derive(Serialize, Deserialize)]
struct Marker {
    version: String,
    target: Target,
    path: PathBuf,
}

fn read_marker(directory: &Path) -> Option<Staged> {
    parse_marker(
        &fs::read_to_string(directory.join(MARKER_FILE_NAME)).ok()?,
        directory,
    )
}

fn parse_marker(text: &str, directory: &Path) -> Option<Staged> {
    let marker: Marker = serde_json::from_str(text).ok()?;
    if marker.path != marker.target.staging(directory) {
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
            arch: "aarch64",
            executable: PathBuf::from(executable),
            writable_target: true,
            ..Facts::default()
        }
    }

    fn linux(executable: &str) -> Facts {
        Facts {
            os: "linux",
            arch: "x86_64",
            executable: PathBuf::from(executable),
            writable_target: true,
            ..Facts::default()
        }
    }

    fn release_with(names: &[&str]) -> Release {
        Release {
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
        }
    }

    #[test]
    fn detection_covers_writable_elevated_homebrew_deb_and_windows_installs() {
        let translocated = mac(
            "/private/var/folders/x1/abc/T/AppTranslocation/0A1B2C3D-4E5F/d/zz.app/Contents/MacOS/zz",
        );
        let applications = mac("/Applications/zz.app/Contents/MacOS/zz");
        let unwritable_mac = Facts {
            writable_target: false,
            ..applications.clone()
        };
        let brewed = Facts {
            homebrew: Some((PathBuf::from("/opt/homebrew"), "zz@beta".to_owned())),
            ..applications.clone()
        };
        let appimage = Facts {
            appimage: Some(PathBuf::from("/home/me/Apps/zz.AppImage")),
            ..linux("/tmp/.mount_zzAbC/usr/lib/zz/zz")
        };
        let deb_pkexec = Facts {
            deb_owned: true,
            pkexec: true,
            writable_target: false,
            ..linux("/usr/lib/zz/zz")
        };
        let deb_no_pkexec = Facts {
            pkexec: false,
            ..deb_pkexec.clone()
        };
        let pacman = Facts {
            pacman_owned: true,
            aur_helper: Some("paru".to_owned()),
            writable_target: false,
            ..linux("/usr/lib/zz/zz")
        };
        let windows = Facts {
            os: "windows",
            arch: "x86_64",
            executable: PathBuf::from("C:/Program Files/zz/zz.exe"),
            ..Facts::default()
        };
        let cases = [
            (translocated, None),
            (
                applications,
                Some(Target::MacBundle {
                    app: PathBuf::from("/Applications/zz.app"),
                    elevated: false,
                }),
            ),
            (
                unwritable_mac,
                Some(Target::MacBundle {
                    app: PathBuf::from("/Applications/zz.app"),
                    elevated: true,
                }),
            ),
            (
                brewed,
                Some(Target::Homebrew {
                    brew: PathBuf::from("/opt/homebrew/bin/brew"),
                    cask: "zz@beta".to_owned(),
                    app: PathBuf::from("/Applications/zz.app"),
                }),
            ),
            (mac("/Applications/zz Dev.app/Contents/MacOS/zz"), None),
            (
                deb_pkexec,
                Some(Target::LinuxDeb {
                    executable: PathBuf::from("/usr/lib/zz/zz"),
                }),
            ),
            (deb_no_pkexec, None),
            (pacman, None),
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
            (windows, Some(Target::WindowsSetup)),
        ];
        for (facts, expected) in cases {
            assert_eq!(target_of(&facts), expected, "{facts:?}");
        }
    }

    #[test]
    fn each_target_takes_its_own_asset_and_needs_its_checksum() {
        let release = release_with(&[
            "zz-0.23.0-macos-arm64.dmg",
            "zz-0.23.0-macos-arm64.dmg.sha256",
            "zz-0.23.0-linux-x86_64.tar.gz",
            "zz-0.23.0-linux-x86_64.tar.gz.sha256",
            "zz-0.23.0-linux-aarch64.tar.gz",
            "zz-0.23.0-linux-aarch64.AppImage",
            "zz-0.23.0-linux-aarch64.AppImage.sha256",
            "zz-0.23.0-linux-x86_64.deb",
            "zz-0.23.0-linux-x86_64.deb.sha256",
            "zz-0.23.0-windows-x64-setup.exe",
            "zz-0.23.0-windows-x64-setup.exe.sha256",
        ]);
        let bundle = Target::MacBundle {
            app: PathBuf::from("/Applications/zz.app"),
            elevated: false,
        };
        let tarball = Target::LinuxTarball {
            prefix: PathBuf::from("/home/me/.local"),
        };
        let appimage = Target::AppImage {
            path: PathBuf::from("/home/me/Apps/zz.AppImage"),
        };
        let deb = Target::LinuxDeb {
            executable: PathBuf::from("/usr/lib/zz/zz"),
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
        assert_eq!(chosen(&deb, "x86_64"), pair("zz-0.23.0-linux-x86_64.deb"));
        assert_eq!(
            chosen(&Target::WindowsSetup, "x86_64"),
            pair("zz-0.23.0-windows-x64-setup.exe")
        );
        assert_eq!(chosen(&Target::WindowsSetup, "aarch64"), None);
    }

    #[test]
    fn manual_fallback_explains_owner_and_command_for_unmanaged_installs() {
        let empty = release_with(&[]);
        let pacman = Facts {
            pacman_owned: true,
            aur_helper: Some("paru".to_owned()),
            writable_target: false,
            ..linux("/usr/lib/zz/zz")
        };
        assert_eq!(
            plan_for(&empty, Channel::Stable, &pacman),
            InstallPlan::Manual(ManualUpdate {
                owner: "pacman (zz-bin)".to_owned(),
                command: Some("paru -S zz-bin".to_owned()),
            })
        );
        assert_eq!(
            plan_for(
                &empty,
                Channel::Beta,
                &Facts {
                    aur_helper: None,
                    ..pacman
                }
            ),
            InstallPlan::Manual(ManualUpdate {
                owner: "pacman (zz-beta-bin)".to_owned(),
                command: None,
            })
        );
        let deb_without_pkexec = Facts {
            deb_owned: true,
            pkexec: false,
            writable_target: false,
            ..linux("/usr/lib/zz/zz")
        };
        assert_eq!(
            plan_for(&empty, Channel::Stable, &deb_without_pkexec),
            InstallPlan::Manual(ManualUpdate {
                owner: "apt (zz)".to_owned(),
                command: Some("curl -fsSL https://zzmux.sh/install.sh | sh".to_owned()),
            })
        );
        let nix = Facts {
            writable_target: false,
            ..linux("/nix/store/abc-zz/lib/zz/zz")
        };
        assert_eq!(
            plan_for(&empty, Channel::Stable, &nix),
            InstallPlan::Manual(ManualUpdate {
                owner: "Nix".to_owned(),
                command: None,
            })
        );
    }

    #[test]
    fn clearing_a_homebrew_marker_never_deletes_the_live_app_bundle() {
        let root = tempfile::tempdir().expect("temp dir");
        let app = root.path().join("zz.app");
        fs::create_dir_all(app.join("Contents")).expect("create live app");
        let cache = root.path().join("cache");
        fs::create_dir_all(&cache).expect("create cache");
        let staged = Staged {
            version: version("0.23.0"),
            target: Target::Homebrew {
                brew: PathBuf::from("/opt/homebrew/bin/brew"),
                cask: "zz".to_owned(),
                app: app.clone(),
            },
            path: app.clone(),
        };
        write_marker(&cache, &staged).expect("write marker");

        clear_staged_in(&cache).expect("clear marker");

        assert!(app.join("Contents").is_dir());
        assert!(!cache.exists());
    }
}
