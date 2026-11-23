use std::{
    cmp::Ordering,
    env,
    error::Error,
    fmt,
    fs::{self, File, OpenOptions, TryLockError},
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

use flate2::read::GzDecoder;
use semver::Version;
use zz_daemon_client::unmasked::SpawnUnmasked as _;
use zz_protocol::{
    MIN_CLIENT_REVISION, MIN_DAEMON_REVISION, PROTOCOL_VERSION, REVISION, app_identity,
};

use crate::update::{Progress, UpdateError, download};

pub const OPT_OUT_ENV: &str = "ZZ_OWNED_COPY";
const BASE_URL_ENV: &str = "ZZ_UPDATE_BASE_URL";
const RELEASE_DOWNLOADS: &str = "https://github.com/autistic-computer/zebrilus/releases/download";
const BINARY: &str = if cfg!(windows) { "zz.exe" } else { "zz" };
const BUILD_INFO: &str = "build-info";
const LOCK: &str = ".lock";
const STAGING: &str = ".staging-";
const ASIDE: &str = "zz.old-";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildInfo {
    pub version: Version,
    pub epoch: u16,
    pub revision: u32,
    pub min_client_revision: u32,
    pub min_daemon_revision: u32,
}

impl BuildInfo {
    pub fn current() -> Self {
        Self {
            version: Version::parse(env!("CARGO_PKG_VERSION")).expect("the zz version is semver"),
            epoch: PROTOCOL_VERSION,
            revision: REVISION,
            min_client_revision: MIN_CLIENT_REVISION,
            min_daemon_revision: MIN_DAEMON_REVISION,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut version = None;
        let mut epoch = None;
        let mut revision = None;
        let mut min_client_revision = None;
        let mut min_daemon_revision = None;
        for (key, value) in text.lines().filter_map(|line| line.trim().split_once('=')) {
            match key {
                "version" => version = Version::parse(value).ok(),
                "epoch" => epoch = value.parse().ok(),
                "revision" => revision = value.parse().ok(),
                "min-client-revision" => min_client_revision = value.parse().ok(),
                "min-daemon-revision" => min_daemon_revision = value.parse().ok(),
                _ => {}
            }
        }
        Some(Self {
            version: version?,
            epoch: epoch?,
            revision: revision?,
            min_client_revision: min_client_revision?,
            min_daemon_revision: min_daemon_revision?,
        })
    }
}

impl fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "version={}\nepoch={}\nrevision={}\nmin-client-revision={}\nmin-daemon-revision={}",
            self.version,
            self.epoch,
            self.revision,
            self.min_client_revision,
            self.min_daemon_revision
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub info: BuildInfo,
    pub binary: PathBuf,
}

#[derive(Debug)]
pub enum InstallError {
    NoDataDirectory,
    Busy,
    Current(Version),
    Older(Version),
    NoHeadlessBuild { version: Version, platform: String },
    Unusable { version: Version, reason: String },
    Update(UpdateError),
}

impl From<UpdateError> for InstallError {
    fn from(error: UpdateError) -> Self {
        Self::Update(error)
    }
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDataDirectory => f.write_str("could not find the user's data directory"),
            Self::Busy => f.write_str("an update is already running"),
            Self::Current(version) => write!(f, "already zz {version}"),
            Self::Older(version) => write!(
                f,
                "zz {version} is older than this zz; `zz update --remove` goes back to the packaged one"
            ),
            Self::NoHeadlessBuild { version, platform } => {
                write!(f, "zz {version} has no headless build for {platform}")
            }
            Self::Unusable { version, reason } => write!(f, "the downloaded zz {version} {reason}"),
            Self::Update(error) => write!(f, "{error}"),
        }
    }
}

impl Error for InstallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Update(error) => Some(error),
            _ => None,
        }
    }
}

pub fn directory() -> Option<PathBuf> {
    Some(
        app_identity::platform_data_dir()?
            .join(app_identity::DIRECTORY)
            .join("owned"),
    )
}

pub fn installed() -> Option<Installed> {
    installed_in(&directory()?)
}

fn installed_in(directory: &Path) -> Option<Installed> {
    let text = fs::read_to_string(directory.join(BUILD_INFO)).ok()?;
    Some(Installed {
        info: BuildInfo::parse(&text)?,
        binary: directory.join(BINARY),
    })
}

pub fn install(
    version: &Version,
    mut progress: impl FnMut(Progress),
) -> Result<Installed, InstallError> {
    let directory = directory().ok_or(InstallError::NoDataDirectory)?;
    let asset = headless_asset(version, env::consts::OS, env::consts::ARCH)
        .ok_or_else(|| no_headless_build(version))?;
    let origin = env::var(BASE_URL_ENV)
        .ok()
        .filter(|origin| !origin.is_empty())
        .unwrap_or_else(|| RELEASE_DOWNLOADS.to_owned());
    let running = BuildInfo::current().version;
    install_in(
        &directory,
        &origin,
        &asset,
        version,
        &running,
        &mut progress,
    )
}

fn install_in(
    directory: &Path,
    origin: &str,
    asset: &str,
    version: &Version,
    running: &Version,
    progress: &mut impl FnMut(Progress),
) -> Result<Installed, InstallError> {
    let _lock = lock(directory)?;
    sweep(directory);
    match version.cmp(running) {
        Ordering::Equal => return Err(InstallError::Current(version.clone())),
        Ordering::Less => return Err(InstallError::Older(version.clone())),
        Ordering::Greater => {}
    }
    let staging = directory.join(format!("{STAGING}{}", std::process::id()));
    fs::create_dir(&staging).map_err(|source| UpdateError::io("create", &staging, source))?;
    let result = stage(&staging, origin, asset, version, progress).and_then(|info| {
        swap(directory, &staging)?;
        Ok(Installed {
            info,
            binary: directory.join(BINARY),
        })
    });
    sweep(directory);
    result
}

fn stage(
    staging: &Path,
    origin: &str,
    asset: &str,
    version: &Version,
    progress: &mut impl FnMut(Progress),
) -> Result<BuildInfo, InstallError> {
    let url = asset_url(origin, version, asset);
    let expected =
        download::expected_checksum(&format!("{url}.sha256")).map_err(|error| match error {
            UpdateError::NotFound(_) => no_headless_build(version),
            error => error.into(),
        })?;
    let archive = staging.join(asset);
    let actual = download::fetch(&url, &archive, &AtomicBool::new(false), progress)?;
    progress(Progress::Verifying);
    download::verify(&archive, &expected, &actual)?;
    progress(Progress::Staging);
    let binary = staging.join(BINARY);
    let folder = asset.strip_suffix(".tar.gz").unwrap_or(asset);
    unpack(&archive, folder, &binary, version)?;
    let info = self_check(&binary, version)?;
    let pending = staging.join(BUILD_INFO);
    fs::write(&pending, format!("{info}\n"))
        .map_err(|source| UpdateError::io("write", &pending, source))?;
    Ok(info)
}

fn unpack(
    archive: &Path,
    folder: &str,
    binary: &Path,
    version: &Version,
) -> Result<(), InstallError> {
    let unreadable = |source: io::Error| UpdateError::io("unpack", archive, source);
    let file = File::open(archive).map_err(unreadable)?;
    let wanted = Path::new(folder).join(BINARY);
    let mut tarball = tar::Archive::new(GzDecoder::new(file));
    for entry in tarball.entries().map_err(unreadable)? {
        let mut entry = entry.map_err(unreadable)?;
        if entry.header().entry_type().is_file() && *entry.path().map_err(unreadable)? == *wanted {
            entry.unpack(binary).map_err(unreadable)?;
            return Ok(());
        }
    }
    Err(InstallError::Unusable {
        version: version.clone(),
        reason: format!("carries no {}", wanted.display()),
    })
}

fn self_check(binary: &Path, version: &Version) -> Result<BuildInfo, InstallError> {
    let unusable = |reason: String| InstallError::Unusable {
        version: version.clone(),
        reason,
    };
    let output = Command::new(binary)
        .arg("build-info")
        .env(OPT_OUT_ENV, "0")
        .output_unmasked()
        .map_err(|error| unusable(format!("does not start here: {error}")))?;
    if !output.status.success() {
        return Err(unusable(format!("does not start here ({})", output.status)));
    }
    let info = BuildInfo::parse(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| unusable("prints no build-info".to_owned()))?;
    if info.version != *version {
        return Err(unusable(format!("reports zz {}", info.version)));
    }
    Ok(info)
}

fn swap(directory: &Path, staging: &Path) -> Result<(), InstallError> {
    let build_info = directory.join(BUILD_INFO);
    remove_file(&build_info)?;
    let binary = directory.join(BINARY);
    if fs::symlink_metadata(&binary).is_ok() {
        rename(&binary, &aside(directory))?;
    }
    rename(&staging.join(BINARY), &binary)?;
    rename(&staging.join(BUILD_INFO), &build_info)
}

pub fn remove() -> Result<Option<BuildInfo>, InstallError> {
    let directory = directory().ok_or(InstallError::NoDataDirectory)?;
    if !directory.is_dir() {
        return Ok(None);
    }
    let _lock = lock(&directory)?;
    let removed = installed_in(&directory).map(|copy| copy.info);
    remove_file(&directory.join(BUILD_INFO))?;
    let binary = directory.join(BINARY);
    if fs::symlink_metadata(&binary).is_ok() {
        rename(&binary, &aside(&directory))?;
    }
    sweep(&directory);
    Ok(removed)
}

fn lock(directory: &Path) -> Result<File, InstallError> {
    fs::create_dir_all(directory).map_err(|source| UpdateError::io("create", directory, source))?;
    let path = directory.join(LOCK);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|source| UpdateError::io("open", &path, source))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(InstallError::Busy),
        Err(TryLockError::Error(source)) => Err(UpdateError::io("lock", &path, source).into()),
    }
}

fn sweep(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(STAGING) {
            let _ = fs::remove_dir_all(entry.path());
        } else if name.starts_with(ASIDE) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn aside(directory: &Path) -> PathBuf {
    let mut n = 0u32;
    loop {
        let path = directory.join(format!("{ASIDE}{n}"));
        if fs::symlink_metadata(&path).is_err() {
            return path;
        }
        n += 1;
    }
}

fn rename(from: &Path, to: &Path) -> Result<(), InstallError> {
    fs::rename(from, to).map_err(|source| UpdateError::io("move", from, source).into())
}

fn remove_file(path: &Path) -> Result<(), InstallError> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(UpdateError::io("remove", path, error).into())
        }
        _ => Ok(()),
    }
}

fn no_headless_build(version: &Version) -> InstallError {
    InstallError::NoHeadlessBuild {
        version: version.clone(),
        platform: format!("{}-{}", env::consts::OS, env::consts::ARCH),
    }
}

fn headless_asset(version: &Version, os: &str, arch: &str) -> Option<String> {
    let platform = match (os, arch) {
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("macos", "aarch64") => "macos-arm64",
        ("windows", "x86_64") => "windows-x64",
        _ => return None,
    };
    Some(format!("zz-{version}-headless-{platform}.tar.gz"))
}

fn asset_url(origin: &str, version: &Version, asset: &str) -> String {
    format!("{}/v{version}/{asset}", origin.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead as _, BufReader, Write as _},
        net::TcpListener,
        thread,
    };

    use super::*;

    fn build(version: &str, epoch: u16, revision: u32, min_client_revision: u32) -> BuildInfo {
        BuildInfo {
            version: Version::parse(version).expect("valid version"),
            epoch,
            revision,
            min_client_revision,
            min_daemon_revision: 1,
        }
    }

    fn listing(directory: &Path) -> Vec<(String, Option<Vec<u8>>)> {
        let mut entries = fs::read_dir(directory)
            .expect("read the owned directory")
            .map(|entry| {
                let entry = entry.expect("directory entry");
                let contents = entry
                    .file_type()
                    .expect("entry type")
                    .is_file()
                    .then(|| fs::read(entry.path()).expect("read entry"));
                (entry.file_name().to_string_lossy().into_owned(), contents)
            })
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }

    fn serve(files: Vec<(String, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
        let origin = format!("http://{}", listener.local_addr().expect("local address"));
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone the stream"));
                let mut request = String::new();
                let _ = reader.read_line(&mut request);
                let mut header = String::new();
                while reader.read_line(&mut header).is_ok_and(|read| read > 2) {
                    header.clear();
                }
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let (status, body) = files
                    .iter()
                    .find(|(name, _)| name == path)
                    .map_or(("404 Not Found", &[][..]), |(_, body)| ("200 OK", body));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body);
            }
        });
        origin
    }

    #[test]
    fn each_platform_downloads_its_own_headless_archive() {
        let version = Version::new(0, 31, 0);
        let url = |os, arch| {
            headless_asset(&version, os, arch)
                .map(|asset| asset_url(&format!("{RELEASE_DOWNLOADS}/"), &version, &asset))
        };
        assert_eq!(
            url("linux", "x86_64").as_deref(),
            Some(
                "https://github.com/autistic-computer/zebrilus/releases/download/v0.31.0/zz-0.31.0-headless-linux-x86_64.tar.gz"
            )
        );
        assert_eq!(
            url("linux", "aarch64").as_deref(),
            Some(
                "https://github.com/autistic-computer/zebrilus/releases/download/v0.31.0/zz-0.31.0-headless-linux-aarch64.tar.gz"
            )
        );
        assert_eq!(
            url("macos", "aarch64").as_deref(),
            Some(
                "https://github.com/autistic-computer/zebrilus/releases/download/v0.31.0/zz-0.31.0-headless-macos-arm64.tar.gz"
            )
        );
        assert_eq!(
            url("windows", "x86_64").as_deref(),
            Some(
                "https://github.com/autistic-computer/zebrilus/releases/download/v0.31.0/zz-0.31.0-headless-windows-x64.tar.gz"
            )
        );
        assert_eq!(url("macos", "x86_64"), None);
        assert_eq!(url("windows", "aarch64"), None);
    }

    #[test]
    fn a_download_with_a_wrong_checksum_leaves_the_owned_copy_as_it_was() {
        let root = tempfile::tempdir().expect("temporary directory");
        let owned = root.path().join("owned");
        fs::create_dir(&owned).expect("create the owned directory");
        fs::write(owned.join(BINARY), b"zz 0.30.1").expect("write the copy");
        fs::write(
            owned.join(BUILD_INFO),
            format!("{}\n", build("0.30.1", 125, 9, 1)),
        )
        .expect("write its build-info");
        fs::write(owned.join(LOCK), b"").expect("write the lock file");
        let before = listing(&owned);
        let asset = "zz-0.31.0-headless-linux-x86_64.tar.gz";
        let origin = serve(vec![
            (
                format!("/v0.31.0/{asset}.sha256"),
                format!("{}  {asset}\n", "0".repeat(64)).into_bytes(),
            ),
            (format!("/v0.31.0/{asset}"), b"abc".to_vec()),
        ]);

        let error = install_in(
            &owned,
            &origin,
            asset,
            &Version::new(0, 31, 0),
            &Version::new(0, 30, 0),
            &mut |_| {},
        )
        .expect_err("a checksum mismatch");

        assert!(
            matches!(
                &error,
                InstallError::Update(UpdateError::ChecksumMismatch { expected, actual })
                    if *expected == "0".repeat(64)
                        && actual == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            ),
            "{error:?}"
        );
        assert_eq!(listing(&owned), before);
    }

    #[test]
    fn a_swap_stopped_between_its_renames_leaves_no_build_info() {
        let root = tempfile::tempdir().expect("temporary directory");
        let owned = root.path().join("owned");
        let staging = owned.join(format!("{STAGING}1"));
        fs::create_dir_all(&staging).expect("create the staging directory");
        fs::write(owned.join(BINARY), b"zz 0.30.1").expect("write the copy");
        fs::write(
            owned.join(BUILD_INFO),
            format!("{}\n", build("0.30.1", 125, 9, 1)),
        )
        .expect("write its build-info");
        fs::write(
            staging.join(BUILD_INFO),
            format!("{}\n", build("0.31.0", 125, 9, 1)),
        )
        .expect("write the staged build-info");

        let error = swap(&owned, &staging).expect_err("the staged binary is missing");

        assert!(
            matches!(&error, InstallError::Update(UpdateError::Io { action: "move", path, .. }) if *path == staging.join(BINARY)),
            "{error:?}"
        );
        assert_eq!(installed_in(&owned), None);
        assert_eq!(
            listing(&owned),
            vec![
                (format!("{STAGING}1"), None),
                (format!("{ASIDE}0"), Some(b"zz 0.30.1".to_vec())),
            ]
        );
    }
}
