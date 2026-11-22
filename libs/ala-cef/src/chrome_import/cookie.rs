use std::{
    collections::{BTreeMap, HashSet},
    io,
    path::{Path, PathBuf},
};

use crate::{
    BrowserCookie, BrowserCookiePartitionKey, BrowserCookiePriority, BrowserCookieSameSite,
    CookieImportBatch, CookieImportError,
    cookie::{RawCookieInput, current_unix_micros, normalize_raw_cookie},
};
#[cfg(not(target_os = "windows"))]
use aes::Aes128;
#[cfg(not(target_os = "windows"))]
use cbc::cipher::{BlockModeDecrypt as _, KeyIvInit as _, block_padding::Pkcs7};
#[cfg(not(target_os = "windows"))]
use pbkdf2::pbkdf2_hmac;
use rusqlite::Connection;
#[cfg(not(target_os = "windows"))]
use sha1::Sha1;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use url::Url;
#[cfg(target_os = "windows")]
use zeroize::Zeroize as _;
use zeroize::Zeroizing;

#[cfg(target_os = "windows")]
use super::profiles::MAX_LOCAL_STATE_BYTES;
use super::{
    fs_util::{AsSqliteError, SqliteSnapshotError, with_sqlite_snapshot},
    profiles::{
        CHROME_PROFILE_PREFIX, ChromiumBrowser, ResolvedBrowserProfile, chrome_storage_key,
        resolve_profile_source,
    },
};

const DEFAULT_CHROME_PROFILE: &str = "Default";
const CHROMIUM_EPOCH_OFFSET_MICROS: i64 = 11_644_473_600_000_000;
const MAX_COOKIE_DATABASE_BYTES: u64 = 512 * 1024 * 1024;
#[cfg(not(target_os = "windows"))]
const SALT: &[u8] = b"saltysalt";
#[cfg(not(target_os = "windows"))]
const IV: [u8; 16] = [b' '; 16];
#[cfg(target_os = "linux")]
const KEYRING_TIMEOUT_SECS: u64 = 5;

#[cfg(not(target_os = "windows"))]
type Aes128CbcDecryptor = cbc::Decryptor<Aes128>;
#[cfg(not(target_os = "windows"))]
type DerivedKeys = Zeroizing<Vec<[u8; 16]>>;

#[derive(Debug, Error)]
pub enum ChromeCookieImportError {
    #[error("automatic browser cookie import is not supported on this platform")]
    UnsupportedPlatform,
    #[error("the selected browser profile is invalid")]
    InvalidProfile,
    #[error("the browser cookies database was not found")]
    DatabaseNotFound,
    #[error(
        "the browser is holding its cookies database open — close it first, or use Paste cookies / \
         Import cookie file"
    )]
    DatabaseLocked,
    #[error("the browser cookies database is unexpectedly large ({0} bytes)")]
    DatabaseTooLarge(u64),
    #[error("the selected browser profile did not have any cookies")]
    NoCookiesInProfile,
    #[error("the browser cookie encryption key could not be unlocked")]
    CredentialAccess,
    #[error(
        "Chrome 127 or newer sealed these cookies with app-bound (v20) encryption, which only \
         Chrome's own elevated app-bound service can unlock ({skipped} cookies skipped — use \
         Paste cookies or Import cookie file)"
    )]
    AppBoundEncryption { skipped: usize },
    #[error("the browser profile had no cookies that can be imported ({skipped} skipped)")]
    NoUsableCookies { skipped: usize },
    #[error("could not read the browser cookies database: {0}")]
    Io(#[from] io::Error),
    #[error("could not query the browser cookies database: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("could not normalize browser cookies: {0}")]
    Normalize(#[from] CookieImportError),
}

impl ChromeCookieImportError {
    pub fn is_permission_denied(&self) -> bool {
        matches!(self, Self::Io(error) if error.kind() == io::ErrorKind::PermissionDenied)
    }
}

impl From<SqliteSnapshotError> for ChromeCookieImportError {
    fn from(error: SqliteSnapshotError) -> Self {
        match error {
            SqliteSnapshotError::DatabaseLocked => Self::DatabaseLocked,
            SqliteSnapshotError::DatabaseTooLarge(bytes) => Self::DatabaseTooLarge(bytes),
            SqliteSnapshotError::Io(error) => Self::Io(error),
            SqliteSnapshotError::Database(error) => Self::Database(error),
        }
    }
}

impl AsSqliteError for ChromeCookieImportError {
    fn as_sqlite_error(&self) -> Option<&rusqlite::Error> {
        match self {
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "these flags mirror independent columns in Chrome's cookie schema"
)]
struct RawChromeCookie {
    host_key: String,
    name: String,
    value: String,
    path: String,
    expires_utc: i64,
    creation_utc: i64,
    last_update_utc: i64,
    encrypted_value: Vec<u8>,
    secure: bool,
    http_only: bool,
    has_expires: bool,
    same_site: i64,
    priority: i64,
    source_scheme: i64,
    source_port: i64,
    top_frame_site_key: String,
    has_cross_site_ancestor: bool,
    partitioned: bool,
    same_party: bool,
}

struct ChromiumKeys {
    #[cfg(not(target_os = "windows"))]
    v10: DerivedKeys,
    #[cfg(not(target_os = "windows"))]
    v11: DerivedKeys,
    #[cfg(target_os = "windows")]
    master: Zeroizing<[u8; AES_GCM_KEY_BYTES]>,
    credential_unavailable: bool,
}

#[must_use]
pub const fn automatic_import_supported() -> bool {
    cfg!(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "windows"
    ))
}

pub fn import_all_cookies(
    source_profile: &str,
) -> Result<CookieImportBatch, ChromeCookieImportError> {
    if !automatic_import_supported() {
        return Err(ChromeCookieImportError::UnsupportedPlatform);
    }

    let resolved =
        resolve_profile_source(source_profile).ok_or(ChromeCookieImportError::InvalidProfile)?;
    let (user_data_dir, database) = browser_cookie_database(&resolved)?;
    let browser = resolved.browser;
    import_cookie_database(&database, || load_chromium_keys(browser, &user_data_dir))
}

pub fn source_storage_key(profile: &str) -> Result<&str, ChromeCookieImportError> {
    if profile == crate::DEFAULT_BROWSER_PROFILE {
        return Ok(DEFAULT_CHROME_PROFILE);
    }
    if profile.starts_with(CHROME_PROFILE_PREFIX) {
        return chrome_storage_key(profile).ok_or(ChromeCookieImportError::InvalidProfile);
    }
    Ok(DEFAULT_CHROME_PROFILE)
}

fn browser_cookie_database(
    resolved: &ResolvedBrowserProfile,
) -> Result<(PathBuf, PathBuf), ChromeCookieImportError> {
    let root = resolved
        .browser
        .user_data_dir()
        .ok_or(ChromeCookieImportError::DatabaseNotFound)?;
    let profile = root.join(&resolved.storage_key);
    for relative in ["Network/Cookies", "Cookies"] {
        let candidate = profile.join(relative);
        match candidate.try_exists() {
            Ok(true) => return Ok((root, candidate)),
            Ok(false) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(ChromeCookieImportError::DatabaseNotFound)
}

fn import_cookie_database(
    database: &Path,
    mut load_keys: impl FnMut() -> Result<ChromiumKeys, ChromeCookieImportError>,
) -> Result<CookieImportBatch, ChromeCookieImportError> {
    let (meta_version, rows) = with_sqlite_snapshot(
        database,
        "chrome-cookies-",
        "Cookies",
        MAX_COOKIE_DATABASE_BYTES,
        |connection| {
            let meta_version = chromium_meta_version(connection);
            let rows = read_cookie_rows(connection)?;
            Ok::<_, ChromeCookieImportError>((meta_version, rows))
        },
    )?;
    if rows.is_empty() {
        return Err(ChromeCookieImportError::NoCookiesInProfile);
    }

    let now_unix_micros = current_unix_micros();
    let mut skipped = 0usize;
    let mut keys = None;
    let mut key_error = false;
    let mut encrypted_failures = 0usize;
    let mut app_bound = 0usize;
    let mut normalized_candidates = 0usize;
    let mut ranked_cookies = Vec::with_capacity(rows.len());

    for row in rows {
        if row.same_party || (row.partitioned && row.top_frame_site_key.is_empty()) {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let value = if !row.value.is_empty() || row.encrypted_value.is_empty() {
            row.value.clone()
        } else if is_app_bound_value(&row.encrypted_value) {
            app_bound = app_bound.saturating_add(1);
            skipped = skipped.saturating_add(1);
            continue;
        } else {
            let Ok(key_set) = keys.get_or_insert_with(&mut load_keys) else {
                key_error = true;
                encrypted_failures = encrypted_failures.saturating_add(1);
                skipped = skipped.saturating_add(1);
                continue;
            };
            let Some(value) = decrypt_cookie_value(
                &row.encrypted_value,
                &row.host_key,
                meta_version >= 24,
                key_set,
            ) else {
                key_error |= key_set.credential_unavailable;
                encrypted_failures = encrypted_failures.saturating_add(1);
                skipped = skipped.saturating_add(1);
                continue;
            };
            value
        };

        normalized_candidates = normalized_candidates.saturating_add(1);
        match normalize_chrome_row(&row, value, now_unix_micros) {
            Ok(cookie) => ranked_cookies.push((cookie, row.last_update_utc, row.creation_utc)),
            Err(()) => skipped = skipped.saturating_add(1),
        }
    }

    let (cookies, duplicates) = deduplicate_normalized_cookies(ranked_cookies);
    skipped = skipped.saturating_add(duplicates);

    if cookies.is_empty() {
        if app_bound > 0 {
            return Err(ChromeCookieImportError::AppBoundEncryption { skipped: app_bound });
        }
        if (key_error || encrypted_failures > 0) && normalized_candidates == 0 {
            return Err(ChromeCookieImportError::CredentialAccess);
        }
        return Err(ChromeCookieImportError::NoUsableCookies { skipped });
    }

    Ok(CookieImportBatch {
        cookies,
        skipped,
        app_bound_skipped: app_bound,
    })
}

fn normalize_chrome_row(
    row: &RawChromeCookie,
    value: String,
    now_unix_micros: i64,
) -> Result<BrowserCookie, ()> {
    let same_site = match row.same_site {
        -1 | 3 => BrowserCookieSameSite::Unspecified,
        0 => BrowserCookieSameSite::NoRestriction,
        1 => BrowserCookieSameSite::Lax,
        2 => BrowserCookieSameSite::Strict,
        _ => return Err(()),
    };
    let priority = match row.priority {
        0 => BrowserCookiePriority::Low,
        1 => BrowserCookiePriority::Medium,
        2 => BrowserCookiePriority::High,
        _ => return Err(()),
    };
    let source_scheme = match row.source_scheme {
        0 => None,
        1 => Some("nonsecure"),
        2 => Some("secure"),
        _ => return Err(()),
    };
    let expires_unix_micros = if row.has_expires && row.expires_utc > 0 {
        Some(row.expires_utc.saturating_sub(CHROMIUM_EPOCH_OFFSET_MICROS))
    } else {
        None
    };
    let partition_key = if row.top_frame_site_key.is_empty() {
        None
    } else {
        Some(BrowserCookiePartitionKey {
            top_level_site: row.top_frame_site_key.clone(),
            has_cross_site_ancestor: row.has_cross_site_ancestor,
        })
    };
    let source_port = i32::try_from(row.source_port).ok().filter(|port| *port > 0);
    let path = if row.path.is_empty() {
        "/"
    } else {
        row.path.as_str()
    };

    normalize_raw_cookie(
        RawCookieInput {
            name: row.name.clone(),
            value,
            domain: &row.host_key,
            host_only: !row.host_key.starts_with('.'),
            path: Some(path),
            secure: row.secure,
            http_only: row.http_only,
            expires_unix_micros,
            same_site,
            priority,
            source_scheme,
            source_port,
            partition_key,
        },
        now_unix_micros,
    )
    .map_err(|_| ())
}

fn chromium_meta_version(connection: &Connection) -> i64 {
    connection
        .query_row("SELECT value FROM meta WHERE key = 'version'", [], |row| {
            row.get::<_, String>(0)
        })
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

fn read_cookie_rows(
    connection: &Connection,
) -> Result<Vec<RawChromeCookie>, ChromeCookieImportError> {
    let columns = cookie_columns(connection)?;
    let optional = |name: &str, fallback: &str| {
        if columns.contains(name) {
            name.to_owned()
        } else {
            fallback.to_owned()
        }
    };
    let sql = format!(
        "SELECT host_key, name, value, path, expires_utc, encrypted_value, \
         is_secure, is_httponly, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {} \
         FROM cookies",
        optional("has_expires", "CASE WHEN expires_utc = 0 THEN 0 ELSE 1 END"),
        optional("samesite", "-1"),
        optional("priority", "1"),
        optional("source_scheme", "0"),
        optional("source_port", "-1"),
        optional("top_frame_site_key", "''"),
        optional("is_partitioned", "0"),
        optional("is_same_party", "0"),
        optional("creation_utc", "0"),
        optional("last_update_utc", "0"),
        optional("has_cross_site_ancestor", "0"),
    );

    let mut statement = connection.prepare(&sql)?;
    statement
        .query_map([], raw_chrome_cookie)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ChromeCookieImportError::from)
}

fn raw_chrome_cookie(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawChromeCookie> {
    Ok(RawChromeCookie {
        host_key: row.get(0)?,
        name: row.get(1)?,
        value: row.get(2)?,
        path: row.get(3)?,
        expires_utc: row.get(4)?,
        encrypted_value: row.get(5)?,
        secure: row.get::<_, i64>(6)? != 0,
        http_only: row.get::<_, i64>(7)? != 0,
        has_expires: row.get::<_, i64>(8)? != 0,
        same_site: row.get(9)?,
        priority: row.get(10)?,
        source_scheme: row.get(11)?,
        source_port: row.get(12)?,
        top_frame_site_key: row.get(13)?,
        partitioned: row.get::<_, i64>(14)? != 0,
        same_party: row.get::<_, i64>(15)? != 0,
        creation_utc: row.get(16)?,
        last_update_utc: row.get(17)?,
        has_cross_site_ancestor: row.get::<_, i64>(18)? != 0,
    })
}

fn deduplicate_normalized_cookies(
    cookies: Vec<(BrowserCookie, i64, i64)>,
) -> (Vec<BrowserCookie>, usize) {
    let original_len = cookies.len();
    let mut unique: BTreeMap<_, (BrowserCookie, (i64, i64, i64))> = BTreeMap::new();
    for (cookie, last_update_utc, creation_utc) in cookies {
        let identity_domain = if cookie.domain.is_empty() {
            Url::parse(&cookie.source_url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_default()
        } else {
            cookie.domain.clone()
        };
        let identity = (
            cookie.name.clone(),
            identity_domain,
            cookie.path.clone(),
            cookie.partition_key.clone(),
        );
        let rank = (
            last_update_utc,
            creation_utc,
            cookie.expires_unix_micros.unwrap_or(i64::MAX),
        );
        match unique.entry(identity) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert((cookie, rank));
            }
            std::collections::btree_map::Entry::Occupied(mut entry) if rank > entry.get().1 => {
                entry.insert((cookie, rank));
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
    }
    let duplicates = original_len.saturating_sub(unique.len());
    (
        unique.into_values().map(|(cookie, _)| cookie).collect(),
        duplicates,
    )
}

fn cookie_columns(connection: &Connection) -> Result<HashSet<String>, rusqlite::Error> {
    let mut statement = connection.prepare("PRAGMA table_info(cookies)")?;
    statement
        .query_map([], |row| row.get(1))?
        .collect::<Result<HashSet<_>, _>>()
}

#[cfg(target_os = "windows")]
fn is_app_bound_value(encrypted: &[u8]) -> bool {
    encrypted.starts_with(APP_BOUND_PREFIX)
}

#[cfg(not(target_os = "windows"))]
const fn is_app_bound_value(_encrypted: &[u8]) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
fn decrypt_cookie_value(
    encrypted: &[u8],
    host_key: &str,
    has_host_digest: bool,
    keys: &ChromiumKeys,
) -> Option<String> {
    let (prefix, ciphertext) = encrypted.split_at_checked(3)?;
    let candidates = match prefix {
        b"v10" => keys.v10.as_slice(),
        b"v11" => keys.v11.as_slice(),
        _ => return None,
    };
    for key in candidates {
        let mut buffer = ciphertext.to_vec();
        let Ok(plaintext) =
            Aes128CbcDecryptor::new(key.into(), &IV.into()).decrypt_padded::<Pkcs7>(&mut buffer)
        else {
            continue;
        };
        let value = if has_host_digest {
            let digest = Sha256::digest(host_key.as_bytes());
            let Some(value) = plaintext.strip_prefix(digest.as_slice()) else {
                continue;
            };
            value
        } else {
            plaintext
        };
        if let Ok(value) = String::from_utf8(value.to_vec()) {
            return Some(value);
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn derive_key(password: &[u8], iterations: u32) -> [u8; 16] {
    let mut key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(password, SALT, iterations, &mut key);
    key
}

#[cfg(target_os = "macos")]
fn load_chromium_keys(
    browser: ChromiumBrowser,
    _user_data_dir: &Path,
) -> Result<ChromiumKeys, ChromeCookieImportError> {
    use security_framework::passwords::get_generic_password;

    let mut derived = Vec::new();
    for &(service, account) in browser.macos_keychain_entries() {
        if let Ok(secret) = get_generic_password(service, account).map(Zeroizing::new)
            && !secret.is_empty()
        {
            let key = derive_key(&secret, 1003);
            if !derived.contains(&key) {
                derived.push(key);
            }
        }
    }
    if derived.is_empty() {
        return Err(ChromeCookieImportError::CredentialAccess);
    }
    Ok(ChromiumKeys {
        v10: Zeroizing::new(derived.clone()),
        v11: Zeroizing::new(derived),
        credential_unavailable: false,
    })
}

#[cfg(target_os = "linux")]
#[allow(
    clippy::unnecessary_wraps,
    reason = "platform key loaders share one fallible callback signature"
)]
fn load_chromium_keys(
    browser: ChromiumBrowser,
    _user_data_dir: &Path,
) -> Result<ChromiumKeys, ChromeCookieImportError> {
    let mut v10 = vec![derive_key(b"peanuts", 1), derive_key(b"", 1)];
    let mut v11 = Vec::new();
    let secrets = linux_safe_storage_secrets(browser);
    for secret in &secrets {
        let key = derive_key(secret, 1);
        if !v11.contains(&key) {
            v11.push(key);
        }
        if !v10.contains(&key) {
            v10.push(key);
        }
    }
    let empty_key = derive_key(b"", 1);
    if !v11.contains(&empty_key) {
        v11.push(empty_key);
    }
    Ok(ChromiumKeys {
        v10: Zeroizing::new(v10),
        v11: Zeroizing::new(v11),
        credential_unavailable: secrets.is_empty(),
    })
}

#[cfg(target_os = "linux")]
fn linux_safe_storage_secrets(browser: ChromiumBrowser) -> Vec<Zeroizing<Vec<u8>>> {
    let specs = browser.linux_secret_specs();
    let timeout = std::time::Duration::from_secs(KEYRING_TIMEOUT_SECS);
    smol::block_on(async {
        let mut secrets = Vec::new();
        let secret_service = smol::future::or(read_secret_service_secrets(specs), async {
            smol::Timer::after(timeout).await;
            Vec::new()
        })
        .await;
        push_unique_secrets(&mut secrets, secret_service);

        let kwallet = smol::future::or(read_kwallet_secrets(specs), async {
            smol::Timer::after(timeout).await;
            Vec::new()
        })
        .await;
        push_unique_secrets(&mut secrets, kwallet);

        secrets
    })
}

#[cfg(target_os = "linux")]
fn push_unique_secrets(target: &mut Vec<Zeroizing<Vec<u8>>>, incoming: Vec<Zeroizing<Vec<u8>>>) {
    for secret in incoming {
        if !secret.is_empty()
            && !target
                .iter()
                .any(|existing| existing.as_slice() == secret.as_slice())
        {
            target.push(secret);
        }
    }
}

#[cfg(target_os = "linux")]
async fn read_secret_service_secrets(
    specs: &[super::profiles::LinuxSecretSpec],
) -> Vec<Zeroizing<Vec<u8>>> {
    let Ok(service) = oo7::dbus::Service::new().await else {
        return Vec::new();
    };
    let mut collections = Vec::new();
    if let Ok(default) = service.default_collection().await {
        collections.push(default);
    }
    if let Ok(all) = service.collections().await {
        collections.extend(all);
    }

    let mut queries: Vec<Vec<(&str, &str)>> = Vec::new();
    for spec in specs {
        queries.push(vec![
            ("xdg:schema", "chrome_libsecret_os_crypt_password_v2"),
            ("application", spec.application),
        ]);
    }
    for spec in specs {
        queries.push(vec![("service", spec.service), ("account", spec.account)]);
    }
    for spec in specs {
        queries.push(vec![("application", spec.application)]);
    }

    let mut secrets = Vec::new();
    for collection in collections {
        if matches!(collection.is_locked().await, Ok(true)) {
            let _ = collection.unlock(None).await;
        }
        for attributes in &queries {
            let Ok(items) = collection.search_items(attributes).await else {
                continue;
            };
            for item in items {
                if matches!(item.is_locked().await, Ok(true)) {
                    let _ = item.unlock(None).await;
                }
                if let Ok(secret) = item.secret().await {
                    let bytes = secret.as_bytes();
                    if !bytes.is_empty()
                        && !secrets
                            .iter()
                            .any(|existing: &Zeroizing<Vec<u8>>| existing.as_slice() == bytes)
                    {
                        secrets.push(Zeroizing::new(bytes.to_vec()));
                    }
                }
            }
        }
    }
    secrets
}

#[cfg(target_os = "linux")]
async fn read_kwallet_secrets(
    specs: &[super::profiles::LinuxSecretSpec],
) -> Vec<Zeroizing<Vec<u8>>> {
    let Ok(connection) = zbus::Connection::session().await else {
        return Vec::new();
    };
    let mut secrets = Vec::new();
    for (bus_name, object_path) in [
        ("org.kde.kwalletd6", "/modules/kwalletd6"),
        ("org.kde.kwalletd5", "/modules/kwalletd5"),
    ] {
        let Ok(reply) = connection
            .call_method(
                Some(bus_name),
                object_path,
                Some("org.kde.KWallet"),
                "networkWallet",
                &(),
            )
            .await
        else {
            continue;
        };
        let Ok(wallet) = reply.body().deserialize::<String>() else {
            continue;
        };
        if wallet.is_empty() {
            continue;
        }
        let Ok(open_reply) = connection
            .call_method(
                Some(bus_name),
                object_path,
                Some("org.kde.KWallet"),
                "open",
                &(wallet.as_str(), 0i64, "ala-cef"),
            )
            .await
        else {
            continue;
        };
        let Ok(handle) = open_reply.body().deserialize::<i32>() else {
            continue;
        };
        if handle < 0 {
            continue;
        }
        for spec in specs {
            if let Ok(password_reply) = connection
                .call_method(
                    Some(bus_name),
                    object_path,
                    Some("org.kde.KWallet"),
                    "readPassword",
                    &(handle, spec.kwallet_folder, spec.kwallet_key, "ala-cef"),
                )
                .await
                && let Ok(password) = password_reply.body().deserialize::<String>()
                && !password.is_empty()
            {
                let bytes = password.into_bytes();
                if !secrets
                    .iter()
                    .any(|existing: &Zeroizing<Vec<u8>>| existing.as_slice() == bytes.as_slice())
                {
                    secrets.push(Zeroizing::new(bytes));
                }
            }
        }
        let _ = connection
            .call_method(
                Some(bus_name),
                object_path,
                Some("org.kde.KWallet"),
                "close",
                &(handle, false, "ala-cef"),
            )
            .await;
    }
    secrets
}

#[cfg(target_os = "windows")]
const DPAPI_KEY_TAG: &[u8] = b"DPAPI";
#[cfg(target_os = "windows")]
const APP_BOUND_PREFIX: &[u8] = b"v20";
#[cfg(target_os = "windows")]
const AES_GCM_KEY_BYTES: usize = 32;
#[cfg(target_os = "windows")]
const AES_GCM_NONCE_BYTES: usize = 12;

#[cfg(target_os = "windows")]
fn load_chromium_keys(
    _browser: ChromiumBrowser,
    user_data_dir: &Path,
) -> Result<ChromiumKeys, ChromeCookieImportError> {
    Ok(ChromiumKeys {
        master: local_state_master_key(&user_data_dir.join("Local State"))
            .ok_or(ChromeCookieImportError::CredentialAccess)?,
        credential_unavailable: false,
    })
}

#[cfg(target_os = "windows")]
fn local_state_master_key(path: &Path) -> Option<Zeroizing<[u8; AES_GCM_KEY_BYTES]>> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    if std::fs::metadata(path).ok()?.len() > MAX_LOCAL_STATE_BYTES {
        return None;
    }
    let state: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let encoded = state.get("os_crypt")?.get("encrypted_key")?.as_str()?;
    let tagged = Zeroizing::new(BASE64.decode(encoded).ok()?);
    let key = dpapi_unprotect(tagged.strip_prefix(DPAPI_KEY_TAG)?)?;
    Some(Zeroizing::new(
        <[u8; AES_GCM_KEY_BYTES]>::try_from(key.as_slice()).ok()?,
    ))
}

#[cfg(target_os = "windows")]
fn decrypt_cookie_value(
    encrypted: &[u8],
    host_key: &str,
    has_host_digest: bool,
    keys: &ChromiumKeys,
) -> Option<String> {
    let (plaintext, host_bound) = if let Some(sealed) = encrypted
        .strip_prefix(b"v10")
        .or_else(|| encrypted.strip_prefix(b"v11"))
    {
        (decrypt_aes_gcm(sealed, &keys.master)?, has_host_digest)
    } else if is_app_bound_value(encrypted) {
        return None;
    } else {
        (dpapi_unprotect(encrypted)?, false)
    };
    let value = if host_bound {
        plaintext.strip_prefix(Sha256::digest(host_key.as_bytes()).as_slice())?
    } else {
        plaintext.as_slice()
    };
    String::from_utf8(value.to_vec()).ok()
}

#[cfg(target_os = "windows")]
fn decrypt_aes_gcm(sealed: &[u8], key: &[u8; AES_GCM_KEY_BYTES]) -> Option<Zeroizing<Vec<u8>>> {
    use aes_gcm::{Aes256Gcm, KeyInit as _, aead::Aead as _};

    let (nonce, ciphertext) = sealed.split_at_checked(AES_GCM_NONCE_BYTES)?;
    let nonce: &[u8; AES_GCM_NONCE_BYTES] = nonce.try_into().ok()?;
    Aes256Gcm::new(key.into())
        .decrypt(nonce.into(), ciphertext)
        .ok()
        .map(Zeroizing::new)
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "CryptUnprotectData is a raw Win32 entry point with no safe wrapper"
)]
fn dpapi_unprotect(protected: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    use windows::Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptUnprotectData},
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(protected.len()).ok()?,
        pbData: protected.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe { CryptUnprotectData(&raw const input, None, None, None, None, 0, &raw mut output) }
        .ok()?;
    let length = usize::try_from(output.cbData).unwrap_or_default();
    let plaintext = (!output.pbData.is_null() && length > 0).then(|| {
        Zeroizing::new(unsafe { std::slice::from_raw_parts(output.pbData, length) }.to_vec())
    });
    if !output.pbData.is_null() && length > 0 {
        unsafe { std::slice::from_raw_parts_mut(output.pbData, length) }.zeroize();
    }
    let _ = unsafe { LocalFree(Some(HLOCAL(output.pbData.cast()))) };
    plaintext
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn load_chromium_keys(
    _browser: ChromiumBrowser,
    _user_data_dir: &Path,
) -> Result<ChromiumKeys, ChromeCookieImportError> {
    Err(ChromeCookieImportError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use crate::MAX_COOKIE_IMPORT_COUNT;
    #[cfg(not(target_os = "windows"))]
    use cbc::cipher::{BlockModeEncrypt as _, block_padding::Pkcs7};
    use rusqlite::params;
    use tempfile::tempdir;

    use super::*;

    #[cfg(not(target_os = "windows"))]
    type Aes128CbcEncryptor = cbc::Encryptor<Aes128>;

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn decrypts_chromium_values_with_the_host_digest() {
        let host = ".example.com";
        let key = derive_key(b"test password", 1003);
        assert_eq!(
            key,
            [
                0x24, 0x53, 0xfa, 0x64, 0x51, 0xa5, 0x91, 0xb0, 0x79, 0xc2, 0xa0, 0x35, 0x4c, 0xaa,
                0x2e, 0xfe,
            ]
        );
        let mut plaintext = Sha256::digest(host.as_bytes()).to_vec();
        plaintext.extend_from_slice(b"session-value");
        let ciphertext = Aes128CbcEncryptor::new(&key.into(), &IV.into())
            .encrypt_padded_vec::<Pkcs7>(&plaintext);
        let mut encrypted = b"v10".to_vec();
        encrypted.extend(ciphertext);
        let keys = ChromiumKeys {
            v10: Zeroizing::new(vec![key]),
            v11: Zeroizing::new(Vec::new()),
            credential_unavailable: false,
        };

        assert_eq!(
            decrypt_cookie_value(&encrypted, host, true, &keys).as_deref(),
            Some("session-value")
        );
        assert_eq!(
            decrypt_cookie_value(&encrypted, "other.example", true, &keys),
            None
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn derives_linux_chromium_key() {
        assert_eq!(
            derive_key(b"peanuts", 1),
            [
                0xfd, 0x62, 0x1f, 0xe5, 0xa2, 0xb4, 0x02, 0x53, 0x9d, 0xfa, 0x14, 0x7c, 0xa9, 0x27,
                0x27, 0x78,
            ]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn decrypts_aes_gcm_nist_vector_and_v11_prefix() {
        let key = [
            0x31, 0xbd, 0xad, 0xd9, 0x66, 0x98, 0xc2, 0x04, 0xaa, 0x9c, 0xe1, 0x44, 0x8e, 0xa9,
            0x4a, 0xe1, 0xfb, 0x4a, 0x9a, 0x0b, 0x3c, 0x9d, 0x77, 0x3b, 0x51, 0xbb, 0x18, 0x22,
            0x66, 0x6b, 0x8f, 0x22,
        ];
        let mut sealed = [
            0x0d, 0x18, 0xe0, 0x6c, 0x7c, 0x72, 0x5a, 0xc9, 0xe3, 0x62, 0xe1, 0xce, 0xfa, 0x43,
            0x62, 0x18, 0x96, 0x61, 0xd1, 0x63, 0xfc, 0xd6, 0xa5, 0x6d, 0x8b, 0xf0, 0x40, 0x5a,
            0xd6, 0x36, 0xac, 0x1b, 0xbe, 0xdd, 0x5c, 0xc3, 0xee, 0x72, 0x7d, 0xc2, 0xab, 0x4a,
            0x94, 0x89,
        ];
        let plaintext = [
            0x2d, 0xb5, 0x16, 0x8e, 0x93, 0x25, 0x56, 0xf8, 0x08, 0x9a, 0x06, 0x22, 0x98, 0x1d,
            0x01, 0x7d,
        ];

        assert_eq!(
            decrypt_aes_gcm(&sealed, &key).as_deref().map(Vec::as_slice),
            Some(plaintext.as_slice())
        );
        sealed[43] ^= 1;
        assert!(decrypt_aes_gcm(&sealed, &key).is_none());
    }

    #[test]
    fn preserves_non_secure_https_cookies_imports_chips_and_handles_extended_samesite() {
        let directory = tempdir().expect("temporary Chrome profile");
        let database = directory.path().join("Cookies");
        let connection = Connection::open(&database).expect("create cookie database");
        connection
            .execute_batch(
                "CREATE TABLE meta (key LONGVARCHAR NOT NULL UNIQUE PRIMARY KEY, value LONGVARCHAR); \
                 INSERT INTO meta (key, value) VALUES ('version', '24'); \
                 CREATE TABLE cookies ( \
                    host_key TEXT NOT NULL, name TEXT NOT NULL, value TEXT NOT NULL, \
                    path TEXT NOT NULL, expires_utc INTEGER NOT NULL, encrypted_value BLOB NOT NULL, \
                    is_secure INTEGER NOT NULL, is_httponly INTEGER NOT NULL, has_expires INTEGER NOT NULL, \
                    samesite INTEGER NOT NULL, priority INTEGER NOT NULL, source_scheme INTEGER NOT NULL, \
                    source_port INTEGER NOT NULL, top_frame_site_key TEXT NOT NULL, \
                    has_cross_site_ancestor INTEGER NOT NULL, is_same_party INTEGER NOT NULL \
                 );",
            )
            .expect("create cookie schema");
        let future_expiry = CHROMIUM_EPOCH_OFFSET_MICROS + 4_102_444_800_123_456;
        connection
            .execute(
                "INSERT INTO cookies VALUES ('app.example.com', 'https-non-secure', 'val', '/', ?1, x'', 0, 0, 1, 3, 1, 2, 443, '', 0, 0)",
                params![future_expiry],
            )
            .expect("insert https non-secure cookie");
        connection
            .execute(
                "INSERT INTO cookies VALUES ('.embed.example', 'chips', 'tok', '/', ?1, x'', 1, 1, 1, 0, 2, 2, 443, 'https://top.example', 1, 0)",
                params![future_expiry],
            )
            .expect("insert CHIPS cookie");
        connection
            .execute(
                "INSERT INTO cookies VALUES ('.example.com', 'same-party', 'secret', '/', ?1, x'', 1, 1, 1, 1, 1, 2, 443, '', 0, 1)",
                params![future_expiry],
            )
            .expect("insert unsupported SameParty cookie");
        drop(connection);

        let batch = import_cookie_database(&database, || {
            panic!("plaintext cookies must not open the credential store")
        })
        .expect("import cookies");
        assert_eq!(batch.cookies.len(), 2);
        assert_eq!(batch.skipped, 1);
        let non_secure = batch
            .cookies
            .iter()
            .find(|cookie| cookie.name == "https-non-secure")
            .expect("https non-secure cookie");
        assert!(!non_secure.secure);
        assert_eq!(non_secure.source_url, "https://app.example.com/");
        assert_eq!(non_secure.expires_unix_micros, Some(4_102_444_800_123_456));
        assert_eq!(non_secure.same_site, BrowserCookieSameSite::Unspecified);

        let chips = batch
            .cookies
            .iter()
            .find(|cookie| cookie.name == "chips")
            .expect("CHIPS cookie");
        assert_eq!(
            chips.partition_key,
            Some(BrowserCookiePartitionKey {
                top_level_site: "https://top.example".to_owned(),
                has_cross_site_ancestor: true,
            })
        );
    }

    #[test]
    fn deduplicates_using_update_timestamps_and_keeps_partitioned_separate() {
        let directory = tempdir().expect("temporary Chrome profile");
        let database = directory.path().join("Cookies");
        let connection = Connection::open(&database).expect("create cookie database");
        connection
            .execute_batch(
                "CREATE TABLE meta (key LONGVARCHAR NOT NULL UNIQUE PRIMARY KEY, value LONGVARCHAR); \
                 INSERT INTO meta (key, value) VALUES ('version', '24'); \
                 CREATE TABLE cookies ( \
                    host_key TEXT NOT NULL, name TEXT NOT NULL, value TEXT NOT NULL, \
                    path TEXT NOT NULL, expires_utc INTEGER NOT NULL, encrypted_value BLOB NOT NULL, \
                    is_secure INTEGER NOT NULL, is_httponly INTEGER NOT NULL, \
                    top_frame_site_key TEXT NOT NULL, last_update_utc INTEGER NOT NULL \
                 );",
            )
            .expect("create cookie schema");
        let future_expiry = CHROMIUM_EPOCH_OFFSET_MICROS + 4_102_444_800_000_000;
        connection
            .execute(
                "INSERT INTO cookies VALUES ('one.example', 'session', 'older-persistent', '/', ?1, x'', 1, 1, '', 100)",
                params![future_expiry],
            )
            .expect("insert older persistent");
        connection
            .execute(
                "INSERT INTO cookies VALUES ('one.example', 'session', 'newer-session', '/', 0, x'', 1, 1, '', 200)",
                [],
            )
            .expect("insert newer session cookie");
        connection
            .execute(
                "INSERT INTO cookies VALUES ('one.example', 'session', 'partitioned', '/', ?1, x'', 1, 1, 'https://top.example', 150)",
                params![future_expiry],
            )
            .expect("insert partitioned sibling");
        drop(connection);

        let batch = import_cookie_database(&database, || {
            panic!("plaintext cookies must not open the credential store")
        })
        .expect("import deduplicated cookies");

        assert_eq!(batch.cookies.len(), 2);
        assert_eq!(batch.skipped, 1);
        let unpartitioned = batch
            .cookies
            .iter()
            .find(|cookie| cookie.partition_key.is_none())
            .expect("unpartitioned cookie");
        assert_eq!(unpartitioned.value, "newer-session");
        let partitioned = batch
            .cookies
            .iter()
            .find(|cookie| cookie.partition_key.is_some())
            .expect("partitioned cookie");
        assert_eq!(partitioned.value, "partitioned");
    }

    #[test]
    fn handles_large_profiles_beyond_file_import_limit_and_live_wal() {
        let directory = tempdir().expect("temporary Chrome profile");
        let database = directory.path().join("Cookies");
        let connection = Connection::open(&database).expect("create cookie database");
        let journal_mode = connection
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0))
            .expect("enable WAL mode");
        assert_eq!(journal_mode, "wal");
        connection
            .execute_batch(
                "PRAGMA wal_autocheckpoint=0; \
                 CREATE TABLE meta (key LONGVARCHAR NOT NULL UNIQUE PRIMARY KEY, value LONGVARCHAR); \
                 INSERT INTO meta (key, value) VALUES ('version', '24'); \
                 CREATE TABLE cookies ( \
                    host_key TEXT NOT NULL, name TEXT NOT NULL, value TEXT NOT NULL, \
                    path TEXT NOT NULL, expires_utc INTEGER NOT NULL, encrypted_value BLOB NOT NULL, \
                    is_secure INTEGER NOT NULL, is_httponly INTEGER NOT NULL \
                 );",
            )
            .expect("create schema");
        let tx = connection.unchecked_transaction().expect("begin tx");
        {
            let mut stmt = tx
                .prepare("INSERT INTO cookies VALUES ('example.com', ?1, 'v', '/', 0, x'', 1, 1)")
                .expect("prepare insert");
            for index in 0..=MAX_COOKIE_IMPORT_COUNT {
                stmt.execute(params![format!("c{index}")]).expect("insert");
            }
        }
        tx.commit().expect("commit wal");

        let batch = import_cookie_database(&database, || {
            panic!("plaintext cookies must not open the credential store")
        })
        .expect("snapshot active WAL with >10k cookies");
        assert_eq!(batch.cookies.len(), MAX_COOKIE_IMPORT_COUNT + 1);
        assert_eq!(batch.skipped, 0);
    }

    #[test]
    fn maps_profiles_to_safe_chrome_storage_keys() {
        assert_eq!(
            source_storage_key(crate::DEFAULT_BROWSER_PROFILE).expect("default source"),
            DEFAULT_CHROME_PROFILE
        );
        assert_eq!(
            source_storage_key("chrome:Profile 3").expect("named source"),
            "Profile 3"
        );
        assert!(source_storage_key("chrome:../Default").is_err());
        assert_eq!(
            source_storage_key("My profile").expect("custom source"),
            DEFAULT_CHROME_PROFILE
        );
    }

    #[test]
    fn identifies_permission_denied_io_errors() {
        let denied = ChromeCookieImportError::from(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied,
        ));
        let missing =
            ChromeCookieImportError::from(std::io::Error::from(std::io::ErrorKind::NotFound));

        assert!(denied.is_permission_denied());
        assert!(!missing.is_permission_denied());
    }
}
