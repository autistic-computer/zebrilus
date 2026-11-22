use std::{
    collections::HashSet,
    fmt,
    fs::File,
    io::{self, Read as _},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::fs_util::{atomic_write, restrict_to_current_user};
use crate::{MAX_BROWSER_PROFILE_NAME_BYTES, normalize_browser_profile_name};

pub(crate) const MAX_LOCAL_STATE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PROFILE_CACHE_BYTES: u64 = 128 * 1024;
const MAX_DISCOVERED_PROFILES: usize = 64;
const MAX_LABEL_BYTES: usize = 256;
pub(crate) const CHROME_PROFILE_PREFIX: &str = "chrome:";
const PROFILE_CACHE_VERSION: u8 = 3;
const PROFILE_CACHE_FILE_NAME: &str = "chrome-profile-metadata.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ChromiumBrowser {
    Chrome,
    ChromeBeta,
    ChromeCanary,
    Chromium,
    Brave,
    Edge,
    Vivaldi,
    Arc,
    ChromeFlatpak,
    ChromiumFlatpak,
    BraveFlatpak,
    EdgeFlatpak,
    ChromiumSnap,
    BraveSnap,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct LinuxSecretSpec {
    pub application: &'static str,
    pub service: &'static str,
    pub account: &'static str,
    pub kwallet_folder: &'static str,
    pub kwallet_key: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedBrowserProfile {
    pub browser: ChromiumBrowser,
    pub storage_key: String,
}

impl ChromiumBrowser {
    pub(crate) const ALL: &'static [Self] = &[
        Self::Chrome,
        Self::ChromeBeta,
        Self::ChromeCanary,
        Self::Chromium,
        Self::Brave,
        Self::Edge,
        Self::Vivaldi,
        Self::Arc,
        Self::ChromeFlatpak,
        Self::ChromiumFlatpak,
        Self::BraveFlatpak,
        Self::EdgeFlatpak,
        Self::ChromiumSnap,
        Self::BraveSnap,
    ];

    pub(crate) const fn prefix(self) -> &'static str {
        match self {
            Self::Chrome => CHROME_PROFILE_PREFIX,
            Self::ChromeBeta => "chrome-beta:",
            Self::ChromeCanary => "chrome-canary:",
            Self::Chromium => "chromium:",
            Self::Brave => "brave:",
            Self::Edge => "edge:",
            Self::Vivaldi => "vivaldi:",
            Self::Arc => "arc:",
            Self::ChromeFlatpak => "chrome-flatpak:",
            Self::ChromiumFlatpak => "chromium-flatpak:",
            Self::BraveFlatpak => "brave-flatpak:",
            Self::EdgeFlatpak => "edge-flatpak:",
            Self::ChromiumSnap => "chromium-snap:",
            Self::BraveSnap => "brave-snap:",
        }
    }

    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::ChromeBeta => "Chrome Beta",
            Self::ChromeCanary => "Chrome Canary",
            Self::Chromium => "Chromium",
            Self::Brave => "Brave",
            Self::Edge => "Edge",
            Self::Vivaldi => "Vivaldi",
            Self::Arc => "Arc",
            Self::ChromeFlatpak => "Chrome (Flatpak)",
            Self::ChromiumFlatpak => "Chromium (Flatpak)",
            Self::BraveFlatpak => "Brave (Flatpak)",
            Self::EdgeFlatpak => "Edge (Flatpak)",
            Self::ChromiumSnap => "Chromium (Snap)",
            Self::BraveSnap => "Brave (Snap)",
        }
    }

    pub(crate) fn user_data_dir(self) -> Option<PathBuf> {
        browser_user_data_dir(self)
    }

    pub(crate) fn local_state_path(self) -> Option<PathBuf> {
        self.user_data_dir().map(|root| root.join("Local State"))
    }

    #[cfg(target_os = "macos")]
    pub(crate) const fn macos_keychain_entries(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Chrome => &[("Chrome Safe Storage", "Chrome")],
            Self::ChromeBeta => &[
                ("Chrome Safe Storage", "Chrome"),
                ("Chrome Beta Safe Storage", "Chrome Beta"),
            ],
            Self::ChromeCanary => &[
                ("Chrome Canary Safe Storage", "Chrome Canary"),
                ("Chrome Safe Storage", "Chrome"),
            ],
            Self::Chromium => &[("Chromium Safe Storage", "Chromium")],
            Self::Brave => &[("Brave Safe Storage", "Brave")],
            Self::Edge => &[("Microsoft Edge Safe Storage", "Microsoft Edge")],
            Self::Vivaldi => &[("Vivaldi Safe Storage", "Vivaldi")],
            Self::Arc => &[("Arc Safe Storage", "Arc")],
            Self::ChromeFlatpak
            | Self::ChromiumFlatpak
            | Self::BraveFlatpak
            | Self::EdgeFlatpak
            | Self::ChromiumSnap
            | Self::BraveSnap => &[],
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) const fn linux_secret_specs(self) -> &'static [LinuxSecretSpec] {
        const CHROME: LinuxSecretSpec = LinuxSecretSpec {
            application: "chrome",
            service: "Chrome Safe Storage",
            account: "Chrome",
            kwallet_folder: "Chrome Keys",
            kwallet_key: "Chrome Safe Storage",
        };
        const CHROMIUM: LinuxSecretSpec = LinuxSecretSpec {
            application: "chromium",
            service: "Chromium Safe Storage",
            account: "Chromium",
            kwallet_folder: "Chromium Keys",
            kwallet_key: "Chromium Safe Storage",
        };
        const BRAVE: LinuxSecretSpec = LinuxSecretSpec {
            application: "brave",
            service: "Brave Safe Storage",
            account: "Brave",
            kwallet_folder: "Brave Keys",
            kwallet_key: "Brave Safe Storage",
        };
        const EDGE: LinuxSecretSpec = LinuxSecretSpec {
            application: "microsoft-edge",
            service: "Microsoft Edge Safe Storage",
            account: "Microsoft Edge",
            kwallet_folder: "Microsoft Edge Keys",
            kwallet_key: "Microsoft Edge Safe Storage",
        };
        const VIVALDI: LinuxSecretSpec = LinuxSecretSpec {
            application: "vivaldi",
            service: "Vivaldi Safe Storage",
            account: "Vivaldi",
            kwallet_folder: "Vivaldi Keys",
            kwallet_key: "Vivaldi Safe Storage",
        };

        match self {
            Self::Chrome | Self::ChromeBeta | Self::ChromeCanary | Self::ChromeFlatpak => {
                &[CHROME, CHROMIUM]
            }
            Self::Chromium | Self::ChromiumFlatpak | Self::ChromiumSnap => &[CHROMIUM, CHROME],
            Self::Brave | Self::BraveFlatpak | Self::BraveSnap => &[BRAVE, CHROMIUM],
            Self::Edge | Self::EdgeFlatpak => &[CHROMIUM, EDGE],
            Self::Vivaldi => &[VIVALDI, CHROMIUM],
            Self::Arc => &[CHROMIUM],
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DetectedChromeProfile {
    pub profile: String,
    pub display_name: String,
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CachedChromeProfiles {
    version: u8,
    profiles: Vec<DetectedChromeProfile>,
}

impl DetectedChromeProfile {
    pub fn menu_label(&self) -> String {
        self.menu_label_for_list(false)
    }

    pub fn menu_label_for_list(&self, multiple_browsers: bool) -> String {
        let base = match self.email.as_deref() {
            Some(email) if self.display_name.eq_ignore_ascii_case(email) => email.to_owned(),
            Some(email) => format!("{} · {email}", self.display_name),
            None => self.display_name.clone(),
        };
        match self.browser.as_deref() {
            Some(browser)
                if multiple_browsers || browser != ChromiumBrowser::Chrome.display_name() =>
            {
                format!("{browser} · {base}")
            }
            _ => base,
        }
    }

    pub fn has_multiple_browsers(profiles: &[Self]) -> bool {
        let mut first = None;
        for profile in profiles {
            let browser = profile
                .browser
                .as_deref()
                .unwrap_or(ChromiumBrowser::Chrome.display_name());
            match first {
                None => first = Some(browser),
                Some(existing) if existing != browser => return true,
                Some(_) => {}
            }
        }
        false
    }
}

#[derive(Debug)]
pub enum ChromeProfileDiscoveryError {
    Io(io::Error),
    TooLarge(u64),
    Json(serde_json::Error),
}

impl fmt::Display for ChromeProfileDiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error)
                if cfg!(target_os = "macos") && error.kind() == io::ErrorKind::PermissionDenied =>
            {
                formatter.write_str(
                    "macOS blocked access to browser profile metadata. Give this app Full Disk \
                     Access in System Settings > Privacy & Security, then quit and reopen it.",
                )
            }
            Self::Io(error) => {
                write!(
                    formatter,
                    "could not read browser profile metadata: {error}"
                )
            }
            Self::TooLarge(bytes) => write!(
                formatter,
                "browser profile metadata is too large ({bytes} bytes)"
            ),
            Self::Json(error) => write!(
                formatter,
                "could not parse browser profile metadata: {error}"
            ),
        }
    }
}

impl std::error::Error for ChromeProfileDiscoveryError {}

impl From<io::Error> for ChromeProfileDiscoveryError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ChromeProfileDiscoveryError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub fn discover_profiles() -> Result<Vec<DetectedChromeProfile>, ChromeProfileDiscoveryError> {
    let mut profiles = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut first_error = None;
    let mut permission_error = None;

    for &browser in ChromiumBrowser::ALL {
        let Some(path) = browser.local_state_path() else {
            continue;
        };
        if !seen_paths.insert(path.clone()) {
            continue;
        }
        match discover_profiles_for_browser_at(browser, &path) {
            Ok(found) => {
                let remaining = MAX_DISCOVERED_PROFILES.saturating_sub(profiles.len());
                profiles.extend(found.into_iter().take(remaining));
                if profiles.len() >= MAX_DISCOVERED_PROFILES {
                    break;
                }
            }
            Err(ChromeProfileDiscoveryError::Io(error))
                if error.kind() == io::ErrorKind::NotFound => {}
            Err(ChromeProfileDiscoveryError::Io(error))
                if error.kind() == io::ErrorKind::PermissionDenied =>
            {
                if permission_error.is_none() {
                    permission_error = Some(ChromeProfileDiscoveryError::Io(error));
                }
            }
            Err(error) => {
                log::warn!(
                    "could not discover {} profiles at {}: {error}",
                    browser.display_name(),
                    path.display()
                );
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    if profiles.is_empty()
        && let Some(error) = permission_error.or(first_error)
    {
        return Err(error);
    }
    Ok(profiles)
}

pub fn load_cached_profiles(
    cache_dir: &Path,
) -> Result<Vec<DetectedChromeProfile>, ChromeProfileDiscoveryError> {
    match load_cached_profiles_at(&cache_dir.join(PROFILE_CACHE_FILE_NAME)) {
        Err(ChromeProfileDiscoveryError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            Ok(Vec::new())
        }
        result => result,
    }
}

pub fn save_cached_profiles(
    cache_dir: &Path,
    profiles: &[DetectedChromeProfile],
) -> Result<(), ChromeProfileDiscoveryError> {
    save_cached_profiles_at(&cache_dir.join(PROFILE_CACHE_FILE_NAME), profiles)
}

fn load_cached_profiles_at(
    path: &Path,
) -> Result<Vec<DetectedChromeProfile>, ChromeProfileDiscoveryError> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_PROFILE_CACHE_BYTES {
        return Err(ChromeProfileDiscoveryError::TooLarge(size));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or_default());
    file.by_ref()
        .take(MAX_PROFILE_CACHE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_PROFILE_CACHE_BYTES {
        return Err(ChromeProfileDiscoveryError::TooLarge(
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        ));
    }

    let cache: CachedChromeProfiles = serde_json::from_slice(&bytes)?;
    if cache.version != PROFILE_CACHE_VERSION {
        return Ok(Vec::new());
    }
    Ok(sanitize_cached_profiles(cache.profiles))
}

fn save_cached_profiles_at(
    path: &Path,
    profiles: &[DetectedChromeProfile],
) -> Result<(), ChromeProfileDiscoveryError> {
    let contents = serde_json::to_vec(&CachedChromeProfiles {
        version: PROFILE_CACHE_VERSION,
        profiles: profiles.to_vec(),
    })?;
    atomic_write(path, &contents)?;
    restrict_to_current_user(path)?;
    Ok(())
}

fn sanitize_cached_profiles(profiles: Vec<DetectedChromeProfile>) -> Vec<DetectedChromeProfile> {
    let mut seen = HashSet::new();
    profiles
        .into_iter()
        .filter_map(|profile| {
            let name = normalize_browser_profile_name(&profile.profile).ok()?;
            let resolved = parse_prefixed_profile(&name)?;
            if !seen.insert(name.clone()) {
                return None;
            }
            let display_name = clean_cached_label(&profile.display_name)?;
            let email = profile.email.as_deref().and_then(clean_cached_label);
            let browser = profile
                .browser
                .as_deref()
                .and_then(clean_cached_label)
                .or_else(|| Some(resolved.browser.display_name().to_owned()));
            Some(DetectedChromeProfile {
                profile: name,
                display_name,
                email,
                browser,
            })
        })
        .take(MAX_DISCOVERED_PROFILES)
        .collect()
}

fn clean_cached_label(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.len() <= MAX_LABEL_BYTES && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}

fn discover_profiles_for_browser_at(
    browser: ChromiumBrowser,
    path: &Path,
) -> Result<Vec<DetectedChromeProfile>, ChromeProfileDiscoveryError> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_LOCAL_STATE_BYTES {
        return Err(ChromeProfileDiscoveryError::TooLarge(size));
    }

    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or_default());
    file.by_ref()
        .take(MAX_LOCAL_STATE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_LOCAL_STATE_BYTES {
        return Err(ChromeProfileDiscoveryError::TooLarge(
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        ));
    }
    parse_profiles_for_browser(browser, &bytes)
}

#[cfg(test)]
fn parse_profiles(bytes: &[u8]) -> Result<Vec<DetectedChromeProfile>, ChromeProfileDiscoveryError> {
    parse_profiles_for_browser(ChromiumBrowser::Chrome, bytes)
}

fn parse_profiles_for_browser(
    browser: ChromiumBrowser,
    bytes: &[u8],
) -> Result<Vec<DetectedChromeProfile>, ChromeProfileDiscoveryError> {
    let root: Value = serde_json::from_slice(bytes)?;
    let Some(profile) = root.get("profile") else {
        return Ok(Vec::new());
    };
    let Some(info_cache) = profile.get("info_cache").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };

    let mut seen = HashSet::new();
    let mut ordered_keys = profile
        .get("profiles_order")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|storage_key| info_cache.contains_key(*storage_key) && seen.insert(*storage_key))
        .collect::<Vec<_>>();
    let mut remaining_keys = info_cache
        .keys()
        .map(String::as_str)
        .filter(|storage_key| seen.insert(*storage_key))
        .collect::<Vec<_>>();
    remaining_keys.sort_unstable();
    ordered_keys.extend(remaining_keys);

    let profiles = ordered_keys
        .into_iter()
        .filter_map(|storage_key| {
            profile_from_cache_entry(browser, storage_key, &info_cache[storage_key])
        })
        .take(MAX_DISCOVERED_PROFILES)
        .collect::<Vec<_>>();
    Ok(profiles)
}

fn profile_from_cache_entry(
    browser: ChromiumBrowser,
    storage_key: &str,
    fields: &Value,
) -> Option<DetectedChromeProfile> {
    let prefix = browser.prefix();
    let storage_key = clean_storage_key(storage_key, prefix)?;
    let fields = fields.as_object()?;
    if fields
        .get("is_ephemeral")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }

    let email = clean_field(fields, "user_name").map(str::to_owned);
    let display_name = clean_field(fields, "name")
        .filter(|name| !name.is_empty())
        .or_else(|| clean_field(fields, "gaia_name").filter(|name| !name.is_empty()))
        .map(str::to_owned)
        .or_else(|| email.clone())
        .unwrap_or_else(|| storage_key.to_owned());
    let profile = normalize_browser_profile_name(&format!("{prefix}{storage_key}")).ok()?;

    Some(DetectedChromeProfile {
        profile,
        display_name,
        email,
        browser: Some(browser.display_name().to_owned()),
    })
}

pub fn chrome_storage_key(profile: &str) -> Option<&str> {
    clean_storage_key(
        profile.strip_prefix(CHROME_PROFILE_PREFIX)?,
        CHROME_PROFILE_PREFIX,
    )
}

pub(crate) fn parse_prefixed_profile(profile: &str) -> Option<ResolvedBrowserProfile> {
    for &browser in ChromiumBrowser::ALL {
        let prefix = browser.prefix();
        if let Some(rest) = profile.strip_prefix(prefix) {
            let storage_key = clean_storage_key(rest, prefix)?;
            return Some(ResolvedBrowserProfile {
                browser,
                storage_key: storage_key.to_owned(),
            });
        }
    }
    None
}

pub(crate) fn resolve_profile_source(profile: &str) -> Option<ResolvedBrowserProfile> {
    if profile == crate::DEFAULT_BROWSER_PROFILE {
        return Some(ResolvedBrowserProfile {
            browser: ChromiumBrowser::Chrome,
            storage_key: "Default".to_owned(),
        });
    }
    for &browser in ChromiumBrowser::ALL {
        let prefix = browser.prefix();
        if let Some(rest) = profile.strip_prefix(prefix) {
            let storage_key = clean_storage_key(rest, prefix)?;
            return Some(ResolvedBrowserProfile {
                browser,
                storage_key: storage_key.to_owned(),
            });
        }
    }
    Some(ResolvedBrowserProfile {
        browser: ChromiumBrowser::Chrome,
        storage_key: "Default".to_owned(),
    })
}

fn clean_storage_key<'a>(storage_key: &'a str, prefix: &str) -> Option<&'a str> {
    let storage_key = storage_key.trim();
    (!storage_key.is_empty()
        && storage_key.len() <= MAX_BROWSER_PROFILE_NAME_BYTES.saturating_sub(prefix.len())
        && !storage_key.chars().any(char::is_control)
        && !storage_key.contains(['/', '\\'])
        && !matches!(storage_key, "." | ".."))
    .then_some(storage_key)
}

fn clean_field<'a>(fields: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    let value = fields.get(key)?.as_str()?.trim();
    if value.is_empty() || value.len() > MAX_LABEL_BYTES || value.chars().any(char::is_control) {
        return None;
    }
    Some(value)
}

pub fn chrome_user_data_dir() -> Option<PathBuf> {
    ChromiumBrowser::Chrome.user_data_dir()
}

#[cfg(target_os = "macos")]
fn browser_user_data_dir(browser: ChromiumBrowser) -> Option<PathBuf> {
    let home = objc2_foundation::NSHomeDirectory().to_string();
    if home.is_empty() {
        return None;
    }
    let app_support = PathBuf::from(home).join("Library/Application Support");
    let relative = match browser {
        ChromiumBrowser::Chrome => "Google/Chrome",
        ChromiumBrowser::ChromeBeta => "Google/Chrome Beta",
        ChromiumBrowser::ChromeCanary => "Google/Chrome Canary",
        ChromiumBrowser::Chromium => "Chromium",
        ChromiumBrowser::Brave => "BraveSoftware/Brave-Browser",
        ChromiumBrowser::Edge => "Microsoft Edge",
        ChromiumBrowser::Vivaldi => "Vivaldi",
        ChromiumBrowser::Arc => "Arc/User Data",
        ChromiumBrowser::ChromeFlatpak
        | ChromiumBrowser::ChromiumFlatpak
        | ChromiumBrowser::BraveFlatpak
        | ChromiumBrowser::EdgeFlatpak
        | ChromiumBrowser::ChromiumSnap
        | ChromiumBrowser::BraveSnap => return None,
    };
    Some(app_support.join(relative))
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn browser_user_data_dir(browser: ChromiumBrowser) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".config")));

    match browser {
        ChromiumBrowser::Chrome => Some(config_dir?.join("google-chrome")),
        ChromiumBrowser::ChromeBeta => Some(config_dir?.join("google-chrome-beta")),
        ChromiumBrowser::ChromeCanary => Some(config_dir?.join("google-chrome-unstable")),
        ChromiumBrowser::Chromium => Some(config_dir?.join("chromium")),
        ChromiumBrowser::Brave => Some(config_dir?.join("BraveSoftware/Brave-Browser")),
        ChromiumBrowser::Edge => Some(config_dir?.join("microsoft-edge")),
        ChromiumBrowser::Vivaldi => Some(config_dir?.join("vivaldi")),
        ChromiumBrowser::ChromeFlatpak => {
            Some(home?.join(".var/app/com.google.Chrome/config/google-chrome"))
        }
        ChromiumBrowser::ChromiumFlatpak => {
            Some(home?.join(".var/app/org.chromium.Chromium/config/chromium"))
        }
        ChromiumBrowser::BraveFlatpak => {
            Some(home?.join(".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser"))
        }
        ChromiumBrowser::EdgeFlatpak => {
            Some(home?.join(".var/app/com.microsoft.Edge/config/microsoft-edge"))
        }
        ChromiumBrowser::ChromiumSnap => Some(home?.join("snap/chromium/common/chromium")),
        ChromiumBrowser::BraveSnap => {
            Some(home?.join("snap/brave/current/.config/BraveSoftware/Brave-Browser"))
        }
        ChromiumBrowser::Arc => None,
    }
}

#[cfg(target_os = "windows")]
fn browser_user_data_dir(browser: ChromiumBrowser) -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)?;
    let relative = match browser {
        ChromiumBrowser::Chrome => "Google/Chrome/User Data",
        ChromiumBrowser::ChromeBeta => "Google/Chrome Beta/User Data",
        ChromiumBrowser::ChromeCanary => "Google/Chrome SxS/User Data",
        ChromiumBrowser::Chromium => "Chromium/User Data",
        ChromiumBrowser::Brave => "BraveSoftware/Brave-Browser/User Data",
        ChromiumBrowser::Edge => "Microsoft/Edge/User Data",
        ChromiumBrowser::Vivaldi => "Vivaldi/User Data",
        ChromiumBrowser::Arc => {
            "Packages/TheBrowserCompany.Arc_ttt1ap7aakyb4/LocalCache/Local/Arc/User Data"
        }
        ChromiumBrowser::ChromeFlatpak
        | ChromiumBrowser::ChromiumFlatpak
        | ChromiumBrowser::BraveFlatpak
        | ChromiumBrowser::EdgeFlatpak
        | ChromiumBrowser::ChromiumSnap
        | ChromiumBrowser::BraveSnap => return None,
    };
    Some(local_app_data.join(relative))
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "windows"
)))]
fn browser_user_data_dir(_browser: ChromiumBrowser) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_stable_profiles_in_chromes_order() {
        let profiles = parse_profiles(
            br#"{
                "profile": {
                    "profiles_order": ["Default", "Profile 5", "Profile 4", "Profile 3"],
                    "info_cache": {
                        "Default": {
                            "name": "Work",
                            "gaia_id": "gaia-one",
                            "user_name": "dev@example.com"
                        },
                        "Profile 2": {
                            "name": "Signed out",
                            "gaia_id": "",
                            "user_name": ""
                        },
                        "Profile 3": {
                            "name": "Temporary",
                            "gaia_id": "gaia-three",
                            "user_name": "temp@example.com",
                            "is_ephemeral": true
                        },
                        "Profile 4": {
                            "name": "Personal",
                            "gaia_id": "gaia-four",
                            "user_name": "me@example.com"
                        },
                        "Profile 5": {
                            "name": "Managed",
                            "gaia_id": "gaia-five"
                        }
                    }
                }
            }"#,
        )
        .expect("valid Chrome Local State fixture");

        assert_eq!(profiles.len(), 4);
        assert_eq!(profiles[0].profile, "chrome:Default");
        assert_eq!(profiles[0].menu_label(), "Work · dev@example.com");
        assert_eq!(
            profiles[0].menu_label_for_list(true),
            "Chrome · Work · dev@example.com"
        );
        assert_eq!(profiles[1].profile, "chrome:Profile 5");
        assert_eq!(profiles[1].menu_label(), "Managed");
        assert_eq!(profiles[2].profile, "chrome:Profile 4");
        assert_eq!(profiles[2].menu_label(), "Personal · me@example.com");
        assert_eq!(profiles[3].profile, "chrome:Profile 2");
        assert_eq!(profiles[3].menu_label(), "Signed out");
    }

    #[test]
    fn resolves_and_labels_multiple_chromium_browsers() {
        let brave = parse_profiles_for_browser(
            ChromiumBrowser::Brave,
            br#"{"profile":{"info_cache":{"Default":{"name":"Personal"}}}}"#,
        )
        .expect("parse Brave fixture");
        assert_eq!(brave.len(), 1);
        assert_eq!(brave[0].profile, "brave:Default");
        assert_eq!(brave[0].browser.as_deref(), Some("Brave"));
        assert_eq!(brave[0].menu_label(), "Brave · Personal");
        assert_eq!(
            resolve_profile_source("brave:Default"),
            Some(ResolvedBrowserProfile {
                browser: ChromiumBrowser::Brave,
                storage_key: "Default".to_owned(),
            })
        );
        assert_eq!(resolve_profile_source("brave:../Default"), None);
    }

    #[test]
    fn rejects_storage_keys_that_could_escape_browser_root() {
        for profile in [
            "chrome:../Default",
            "chrome:Profile/2",
            "chrome:Profile\\2",
            "chrome:.",
        ] {
            assert_eq!(chrome_storage_key(profile), None, "accepted {profile:?}");
            assert_eq!(
                resolve_profile_source(profile),
                None,
                "accepted {profile:?}"
            );
        }
        assert_eq!(chrome_storage_key("chrome:Profile 2"), Some("Profile 2"));
    }

    #[test]
    fn cached_profile_labels_round_trip_and_revalidate() {
        let directory = tempdir().expect("temporary profile cache directory");
        let path = directory.path().join(PROFILE_CACHE_FILE_NAME);
        let profiles = vec![
            DetectedChromeProfile {
                profile: "chrome:Default".to_owned(),
                display_name: "Work".to_owned(),
                email: Some("dev@example.com".to_owned()),
                browser: Some("Chrome".to_owned()),
            },
            DetectedChromeProfile {
                profile: "brave:Profile 1".to_owned(),
                display_name: "Personal".to_owned(),
                email: None,
                browser: Some("Brave".to_owned()),
            },
        ];

        save_cached_profiles_at(&path, &profiles).expect("save bounded profile cache");
        assert_eq!(
            load_cached_profiles_at(&path).expect("reload bounded profile cache"),
            profiles
        );
        assert!(DetectedChromeProfile::has_multiple_browsers(&profiles));
        let source = fs::read_to_string(path).expect("read serialized cache");
        assert!(!source.contains("gaia"));

        let sanitized = sanitize_cached_profiles(vec![
            DetectedChromeProfile {
                profile: "chrome:Default".to_owned(),
                display_name: " Work ".to_owned(),
                email: Some("dev@example.com".to_owned()),
                browser: None,
            },
            DetectedChromeProfile {
                profile: "chrome:Default".to_owned(),
                display_name: "Duplicate".to_owned(),
                email: None,
                browser: None,
            },
            DetectedChromeProfile {
                profile: "default".to_owned(),
                display_name: "Wrong namespace".to_owned(),
                email: None,
                browser: None,
            },
        ]);
        assert_eq!(sanitized.len(), 1);
        assert_eq!(sanitized[0].display_name, "Work");
        assert_eq!(sanitized[0].browser.as_deref(), Some("Chrome"));
    }
}
