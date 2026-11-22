use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use url::Url;

pub const MAX_COOKIE_IMPORT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_COOKIE_IMPORT_COUNT: usize = 10_000;
const MAX_COOKIE_NAME_AND_VALUE_BYTES: usize = 4096;
const UNIX_MILLIS_THRESHOLD_SECS: f64 = 1_000_000_000_000.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrowserCookieSameSite {
    #[default]
    Unspecified,
    NoRestriction,
    Lax,
    Strict,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrowserCookiePriority {
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BrowserCookiePartitionKey {
    pub top_level_site: String,
    pub has_cross_site_ancestor: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct BrowserCookie {
    pub source_url: String,
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires_unix_micros: Option<i64>,
    pub same_site: BrowserCookieSameSite,
    pub priority: BrowserCookiePriority,
    pub partition_key: Option<BrowserCookiePartitionKey>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CookieImportBatch {
    pub cookies: Vec<BrowserCookie>,
    pub skipped: usize,
    pub app_bound_skipped: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CookieImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub rejected: usize,
    pub app_bound_skipped: usize,
    pub persisted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SiteDataClearResult {
    pub message_id: i32,
    pub success: bool,
}

#[derive(Debug, Error)]
pub enum CookieImportError {
    #[error("the cookie file is empty")]
    Empty,
    #[error("the cookie file is larger than 8 MiB")]
    TooLarge,
    #[error("the cookie file contains more than 10000 entries")]
    TooMany,
    #[error("the Cookie-Editor JSON is invalid: {0}")]
    InvalidJson(#[source] serde_json::Error),
    #[error("the cookie file does not contain any supported cookies ({skipped} skipped)")]
    NoUsableCookies { skipped: usize },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonCookie {
    name: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    secure: bool,
    #[serde(default, alias = "http_only")]
    http_only: bool,
    #[serde(default, alias = "host_only")]
    host_only: Option<bool>,
    #[serde(default, alias = "expiration_date", alias = "expires")]
    expiration_date: Option<f64>,
    #[serde(default)]
    session: Option<bool>,
    #[serde(default, alias = "same_site")]
    same_site: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default, alias = "partition_key")]
    partition_key: Option<Value>,
    #[serde(default)]
    partitioned: bool,
    #[serde(default, alias = "source_scheme")]
    source_scheme: Option<String>,
    #[serde(default, alias = "source_port")]
    source_port: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowError {
    Invalid,
    Unsupported,
}

#[cfg(feature = "chrome-import")]
#[allow(
    clippy::struct_excessive_bools,
    reason = "cookie normalization accepts Chromium schema attributes directly"
)]
pub(crate) struct RawCookieInput<'a> {
    pub name: String,
    pub value: String,
    pub domain: &'a str,
    pub host_only: bool,
    pub path: Option<&'a str>,
    pub secure: bool,
    pub http_only: bool,
    pub expires_unix_micros: Option<i64>,
    pub same_site: BrowserCookieSameSite,
    pub priority: BrowserCookiePriority,
    pub source_scheme: Option<&'a str>,
    pub source_port: Option<i32>,
    pub partition_key: Option<BrowserCookiePartitionKey>,
}

pub fn parse_cookie_import(input: &str) -> Result<CookieImportBatch, CookieImportError> {
    if input.len() > MAX_COOKIE_IMPORT_BYTES {
        return Err(CookieImportError::TooLarge);
    }
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let detected = input.trim_start();
    if detected.is_empty() {
        return Err(CookieImportError::Empty);
    }
    let now_unix_micros = current_unix_micros();

    if matches!(detected.as_bytes().first(), Some(b'[' | b'{')) {
        parse_json(detected.trim_end(), now_unix_micros)
    } else {
        parse_netscape(input, now_unix_micros)
    }
}

fn parse_json(input: &str, now_unix_micros: i64) -> Result<CookieImportBatch, CookieImportError> {
    let root: Value = serde_json::from_str(input).map_err(CookieImportError::InvalidJson)?;
    let rows = match root {
        Value::Array(rows) => rows,
        Value::Object(mut object) => match object.remove("cookies") {
            Some(Value::Array(rows)) => rows,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    if rows.len() > MAX_COOKIE_IMPORT_COUNT {
        return Err(CookieImportError::TooMany);
    }

    let mut cookies = Vec::with_capacity(rows.len());
    let mut skipped = 0;
    for row in rows {
        let Ok(row) = serde_json::from_value::<JsonCookie>(row) else {
            skipped += 1;
            continue;
        };
        match json_cookie(row, now_unix_micros) {
            Ok(cookie) => cookies.push(cookie),
            Err(RowError::Invalid | RowError::Unsupported) => skipped += 1,
        }
    }
    finish_batch(cookies, skipped)
}

fn json_cookie(row: JsonCookie, now_unix_micros: i64) -> Result<BrowserCookie, RowError> {
    let partition_key = parse_json_partition_key(row.partition_key.as_ref(), row.partitioned)?;

    let parsed_url = row
        .url
        .as_deref()
        .map(Url::parse)
        .transpose()
        .map_err(|_| RowError::Invalid)?;
    if parsed_url
        .as_ref()
        .is_some_and(|url| !matches!(url.scheme(), "http" | "https"))
    {
        return Err(RowError::Invalid);
    }

    let raw_domain = row
        .domain
        .or_else(|| {
            parsed_url
                .as_ref()
                .and_then(Url::host_str)
                .map(str::to_owned)
        })
        .ok_or(RowError::Invalid)?;
    let host_only = row
        .host_only
        .unwrap_or_else(|| !raw_domain.trim().starts_with('.'));
    let (host, domain) = normalize_cookie_domain(&raw_domain, host_only)?;

    let path = normalize_cookie_path(row.path.as_deref())?;
    let secure = row.secure;
    let scheme = source_scheme(
        row.source_scheme.as_deref(),
        parsed_url.as_ref().map(Url::scheme),
        secure,
    )?;
    let port = row
        .source_port
        .filter(|port| *port > 0)
        .or_else(|| parsed_url.as_ref().and_then(url::Url::port).map(i32::from));
    let source_url = cookie_source_url(&host, &path, scheme, port)?;
    let expires_unix_micros = if row.session == Some(true) {
        None
    } else {
        row.expiration_date
            .map(expiration_micros)
            .transpose()?
            .flatten()
    };
    if expires_unix_micros.is_some_and(|expires| expires <= now_unix_micros) {
        return Err(RowError::Unsupported);
    }

    validate_cookie_fields(
        &row.name,
        &row.value,
        host_only,
        &path,
        secure,
        partition_key.is_some(),
    )?;
    let same_site = same_site(row.same_site.as_deref())?;
    if same_site == BrowserCookieSameSite::NoRestriction && !secure {
        return Err(RowError::Unsupported);
    }

    Ok(BrowserCookie {
        source_url,
        name: row.name,
        value: row.value,
        domain,
        path,
        secure,
        http_only: row.http_only,
        expires_unix_micros,
        same_site,
        priority: priority(row.priority.as_deref())?,
        partition_key,
    })
}

#[cfg(feature = "chrome-import")]
pub(crate) fn normalize_raw_cookie(
    input: RawCookieInput<'_>,
    now_unix_micros: i64,
) -> Result<BrowserCookie, RowError> {
    let partition_key = input
        .partition_key
        .map(|key| normalize_partition_key(&key.top_level_site, key.has_cross_site_ancestor))
        .transpose()?;
    let (host, domain) = normalize_cookie_domain(input.domain, input.host_only)?;
    let path = normalize_cookie_path(input.path)?;
    let scheme = source_scheme(input.source_scheme, None, input.secure)?;
    let port = input.source_port.filter(|port| *port > 0);
    let source_url = cookie_source_url(&host, &path, scheme, port)?;
    if input
        .expires_unix_micros
        .is_some_and(|expires| expires <= now_unix_micros)
    {
        return Err(RowError::Unsupported);
    }
    validate_cookie_fields(
        &input.name,
        &input.value,
        input.host_only,
        &path,
        input.secure,
        partition_key.is_some(),
    )?;
    if input.same_site == BrowserCookieSameSite::NoRestriction && !input.secure {
        return Err(RowError::Unsupported);
    }
    Ok(BrowserCookie {
        source_url,
        name: input.name,
        value: input.value,
        domain,
        path,
        secure: input.secure,
        http_only: input.http_only,
        expires_unix_micros: input.expires_unix_micros,
        same_site: input.same_site,
        priority: input.priority,
        partition_key,
    })
}

fn parse_netscape(
    input: &str,
    now_unix_micros: i64,
) -> Result<CookieImportBatch, CookieImportError> {
    let mut cookies = Vec::new();
    let mut skipped = 0;
    let mut entries = 0;

    for line in input.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || (line.starts_with('#') && !line.starts_with("#HttpOnly_")) {
            continue;
        }
        entries += 1;
        if entries > MAX_COOKIE_IMPORT_COUNT {
            return Err(CookieImportError::TooMany);
        }
        match netscape_cookie(line, now_unix_micros) {
            Ok(cookie) => cookies.push(cookie),
            Err(RowError::Invalid | RowError::Unsupported) => skipped += 1,
        }
    }
    finish_batch(cookies, skipped)
}

fn netscape_cookie(line: &str, now_unix_micros: i64) -> Result<BrowserCookie, RowError> {
    let http_only = line.starts_with("#HttpOnly_");
    let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
    let mut fields = line.split('\t');
    let raw_domain = fields.next().ok_or(RowError::Invalid)?.trim();
    let include_subdomains = parse_bool(fields.next().ok_or(RowError::Invalid)?)?;
    let path = normalize_cookie_path(fields.next())?;
    let secure = parse_bool(fields.next().ok_or(RowError::Invalid)?)?;
    let expiration = fields
        .next()
        .ok_or(RowError::Invalid)?
        .parse::<i64>()
        .map_err(|_| RowError::Invalid)?;
    let name = fields.next().ok_or(RowError::Invalid)?.to_owned();
    let value = fields.next().ok_or(RowError::Invalid)?.to_owned();
    if fields.next().is_some() {
        return Err(RowError::Invalid);
    }

    let host_only = !include_subdomains;
    let (host, domain) = normalize_cookie_domain(raw_domain, host_only)?;
    validate_cookie_fields(&name, &value, host_only, &path, secure, false)?;
    let expires_unix_micros = match expiration {
        -1 | 0 => None,
        value if value > 0 => Some(value.checked_mul(1_000_000).ok_or(RowError::Invalid)?),
        _ => return Err(RowError::Invalid),
    };
    if expires_unix_micros.is_some_and(|expires| expires <= now_unix_micros) {
        return Err(RowError::Unsupported);
    }

    Ok(BrowserCookie {
        source_url: cookie_source_url(&host, &path, if secure { "https" } else { "http" }, None)?,
        name,
        value,
        domain,
        path,
        secure,
        http_only,
        expires_unix_micros,
        same_site: BrowserCookieSameSite::Unspecified,
        priority: BrowserCookiePriority::Medium,
        partition_key: None,
    })
}

fn finish_batch(
    cookies: Vec<BrowserCookie>,
    skipped: usize,
) -> Result<CookieImportBatch, CookieImportError> {
    if cookies.is_empty() {
        return Err(CookieImportError::NoUsableCookies { skipped });
    }
    Ok(CookieImportBatch {
        cookies,
        skipped,
        app_bound_skipped: 0,
    })
}

fn parse_json_partition_key(
    value: Option<&Value>,
    partitioned: bool,
) -> Result<Option<BrowserCookiePartitionKey>, RowError> {
    let parsed = match value {
        None | Some(Value::Null) => None,
        Some(Value::String(site)) => {
            let site = site.trim();
            if site.is_empty() {
                None
            } else {
                Some(normalize_partition_key(site, false)?)
            }
        }
        Some(Value::Object(object)) => {
            let site = object
                .get("topLevelSite")
                .or_else(|| object.get("top_level_site"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|site| !site.is_empty());
            match site {
                None => None,
                Some(site) => {
                    let has_cross_site_ancestor = object
                        .get("hasCrossSiteAncestor")
                        .or_else(|| object.get("has_cross_site_ancestor"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    Some(normalize_partition_key(site, has_cross_site_ancestor)?)
                }
            }
        }
        Some(_) => return Err(RowError::Invalid),
    };
    if partitioned && parsed.is_none() {
        return Err(RowError::Unsupported);
    }
    Ok(parsed)
}

pub(crate) fn normalize_partition_key(
    top_level_site: &str,
    has_cross_site_ancestor: bool,
) -> Result<BrowserCookiePartitionKey, RowError> {
    let parsed = Url::parse(top_level_site.trim()).map_err(|_| RowError::Invalid)?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(RowError::Invalid);
    }
    let origin = parsed.origin().ascii_serialization();
    if origin == "null" {
        return Err(RowError::Invalid);
    }
    Ok(BrowserCookiePartitionKey {
        top_level_site: origin,
        has_cross_site_ancestor,
    })
}

fn validate_cookie_fields(
    name: &str,
    value: &str,
    host_only: bool,
    path: &str,
    secure: bool,
    partitioned: bool,
) -> Result<(), RowError> {
    if name.is_empty()
        || name.len().saturating_add(value.len()) > MAX_COOKIE_NAME_AND_VALUE_BYTES
        || !is_valid_cookie_name(name)
        || !is_valid_cookie_value(value)
    {
        return Err(RowError::Invalid);
    }
    if partitioned && !secure {
        return Err(RowError::Unsupported);
    }
    let lower_prefix = name.get(..9).unwrap_or(name);
    if lower_prefix.len() >= 9 && lower_prefix[..9].eq_ignore_ascii_case("__secure-") && !secure {
        return Err(RowError::Unsupported);
    }
    let host_prefix = name.get(..7).unwrap_or(name);
    if host_prefix.len() >= 7
        && host_prefix[..7].eq_ignore_ascii_case("__host-")
        && (!secure || !host_only || path != "/")
    {
        return Err(RowError::Unsupported);
    }
    Ok(())
}

fn is_valid_cookie_name(name: &str) -> bool {
    name.bytes()
        .all(|byte| !matches!(byte, 0x00..=0x20 | 0x7f | b';' | b'='))
}

fn is_valid_cookie_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte == b'\t' || !matches!(byte, 0x00..=0x1f | 0x7f))
}

fn normalize_cookie_path(path: Option<&str>) -> Result<String, RowError> {
    let path = path.unwrap_or("/").trim();
    if path.is_empty() {
        return Ok("/".to_owned());
    }
    if !path.starts_with('/')
        || path
            .bytes()
            .any(|byte| matches!(byte, 0x00..=0x1f | 0x7f | b';'))
    {
        return Err(RowError::Invalid);
    }
    Ok(path.to_owned())
}

fn parse_bool(value: &str) -> Result<bool, RowError> {
    if value.eq_ignore_ascii_case("true") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("false") {
        Ok(false)
    } else {
        Err(RowError::Invalid)
    }
}

fn source_scheme(
    explicit: Option<&str>,
    url_scheme: Option<&str>,
    secure: bool,
) -> Result<&'static str, RowError> {
    if secure {
        if let Some(value) = explicit
            && !value.eq_ignore_ascii_case("secure")
            && !value.eq_ignore_ascii_case("https")
            && !value.eq_ignore_ascii_case("nonsecure")
            && !value.eq_ignore_ascii_case("non_secure")
            && !value.eq_ignore_ascii_case("http")
        {
            return Err(RowError::Unsupported);
        }
        return Ok("https");
    }
    match explicit {
        None => Ok(if url_scheme == Some("https") {
            "https"
        } else {
            "http"
        }),
        Some(value)
            if value.eq_ignore_ascii_case("secure") || value.eq_ignore_ascii_case("https") =>
        {
            Ok("https")
        }
        Some(value)
            if value.eq_ignore_ascii_case("nonsecure")
                || value.eq_ignore_ascii_case("non_secure")
                || value.eq_ignore_ascii_case("http") =>
        {
            Ok("http")
        }
        Some(_) => Err(RowError::Unsupported),
    }
}

fn normalize_cookie_domain(raw: &str, host_only: bool) -> Result<(String, String), RowError> {
    let raw = raw.trim();
    if raw.is_empty() || raw.chars().any(char::is_whitespace) {
        return Err(RowError::Invalid);
    }
    let host = raw.trim_start_matches('.');
    let authority = authority_host(host);
    let parsed = Url::parse(&format!("http://{authority}")).map_err(|_| RowError::Invalid)?;
    if parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.port().is_some()
    {
        return Err(RowError::Invalid);
    }
    let host = parsed.host_str().ok_or(RowError::Invalid)?.to_owned();
    if !host_only && (is_ip_literal(&host) || !host.contains('.')) {
        return Err(RowError::Unsupported);
    }
    let domain = if host_only {
        String::new()
    } else {
        format!(".{host}")
    };
    Ok((host, domain))
}

fn authority_host(host: &str) -> String {
    if host.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
}

fn cookie_source_url(
    domain: &str,
    path: &str,
    scheme: &str,
    port: Option<i32>,
) -> Result<String, RowError> {
    let host = authority_host(domain.trim_start_matches('.'));
    let mut url = Url::parse(&format!("{scheme}://{host}")).map_err(|_| RowError::Invalid)?;
    if url.host_str().is_none() {
        return Err(RowError::Invalid);
    }
    if let Some(port) = port {
        let port = u16::try_from(port).map_err(|_| RowError::Invalid)?;
        url.set_port(Some(port)).map_err(|()| RowError::Invalid)?;
    }
    url.set_path(path);
    Ok(url.into())
}

fn expiration_micros(seconds: f64) -> Result<Option<i64>, RowError> {
    if !seconds.is_finite() {
        return Err(RowError::Invalid);
    }
    if seconds == 0.0 || seconds == -1.0 {
        return Ok(None);
    }
    if seconds < 0.0 {
        return Err(RowError::Invalid);
    }
    let seconds = if seconds > UNIX_MILLIS_THRESHOLD_SECS {
        seconds / 1_000.0
    } else {
        seconds
    };
    let duration = Duration::try_from_secs_f64(seconds).map_err(|_| RowError::Invalid)?;
    let micros = i64::try_from(duration.as_micros()).map_err(|_| RowError::Invalid)?;
    Ok(Some(micros))
}

pub(crate) fn current_unix_micros() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_micros()).ok())
        .unwrap_or_default()
}

fn same_site(value: Option<&str>) -> Result<BrowserCookieSameSite, RowError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(BrowserCookieSameSite::Unspecified),
        Some(value)
            if value.eq_ignore_ascii_case("unspecified")
                || value.eq_ignore_ascii_case("extended") =>
        {
            Ok(BrowserCookieSameSite::Unspecified)
        }
        Some(value)
            if value.eq_ignore_ascii_case("none")
                || value.eq_ignore_ascii_case("no_restriction")
                || value.eq_ignore_ascii_case("no-restriction") =>
        {
            Ok(BrowserCookieSameSite::NoRestriction)
        }
        Some(value) if value.eq_ignore_ascii_case("lax") => Ok(BrowserCookieSameSite::Lax),
        Some(value) if value.eq_ignore_ascii_case("strict") => Ok(BrowserCookieSameSite::Strict),
        Some(_) => Err(RowError::Unsupported),
    }
}

fn priority(value: Option<&str>) -> Result<BrowserCookiePriority, RowError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(BrowserCookiePriority::Medium),
        Some(value) if value.eq_ignore_ascii_case("low") => Ok(BrowserCookiePriority::Low),
        Some(value) if value.eq_ignore_ascii_case("medium") => Ok(BrowserCookiePriority::Medium),
        Some(value) if value.eq_ignore_ascii_case("high") => Ok(BrowserCookiePriority::High),
        Some(_) => Err(RowError::Unsupported),
    }
}

fn is_ip_literal(domain: &str) -> bool {
    domain
        .trim_matches(['[', ']'])
        .parse::<std::net::IpAddr>()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cookie_editor_json_including_chips_and_empty_partition_keys() {
        let input = r#"[
            {
                "domain": ".example.com",
                "expirationDate": 1893456000.25,
                "hostOnly": false,
                "httpOnly": true,
                "name": "session",
                "path": "/account",
                "partitionKey": {},
                "sameSite": "no_restriction",
                "secure": true,
                "value": "secret"
            },
            {
                "domain": "embed.example",
                "name": "chips",
                "partitionKey": { "topLevelSite": "https://example.com", "hasCrossSiteAncestor": true },
                "secure": true,
                "value": "token"
            },
            {
                "domain": "embed.example",
                "name": "insecure-chips",
                "partitionKey": { "topLevelSite": "https://example.com" },
                "secure": false,
                "value": "rejected"
            }
        ]"#;

        let batch = parse_cookie_import(input).expect("parse cookie export");
        assert_eq!(batch.skipped, 1);
        assert_eq!(batch.cookies.len(), 2);
        let cookie = &batch.cookies[0];
        assert_eq!(cookie.source_url, "https://example.com/account");
        assert_eq!(cookie.domain, ".example.com");
        assert_eq!(cookie.path, "/account");
        assert!(cookie.secure);
        assert!(cookie.http_only);
        assert_eq!(cookie.expires_unix_micros, Some(1_893_456_000_250_000));
        assert_eq!(cookie.same_site, BrowserCookieSameSite::NoRestriction);
        assert_eq!(cookie.priority, BrowserCookiePriority::Medium);
        assert_eq!(cookie.partition_key, None);

        let chips = &batch.cookies[1];
        assert_eq!(
            chips.partition_key,
            Some(BrowserCookiePartitionKey {
                top_level_site: "https://example.com".to_owned(),
                has_cross_site_ancestor: true,
            })
        );
    }

    #[test]
    fn preserves_non_secure_attribute_for_https_origin_and_handles_negative_one_session_expiry() {
        let input = r#"[
            {
                "domain": "app.example.com",
                "url": "https://app.example.com/dashboard",
                "sourceScheme": "secure",
                "name": "prefs",
                "path": "/",
                "secure": false,
                "expires": -1,
                "value": "compact"
            }
        ]"#;

        let batch = parse_cookie_import(input).expect("parse https non-secure cookie");
        assert_eq!(batch.cookies.len(), 1);
        let cookie = &batch.cookies[0];
        assert!(!cookie.secure);
        assert_eq!(cookie.source_url, "https://app.example.com/");
        assert_eq!(cookie.expires_unix_micros, None);
    }

    #[test]
    fn parses_json_object_and_host_only_cookie() {
        let input = r#"{
            "cookies": [{
                "domain": ".example.com",
                "hostOnly": true,
                "name": "theme",
                "path": "/",
                "sameSite": "lax",
                "value": "dark"
            }]
        }"#;

        let batch = parse_cookie_import(input).expect("parse object export");
        assert_eq!(batch.cookies[0].domain, "");
        assert_eq!(batch.cookies[0].source_url, "http://example.com/");
        assert_eq!(batch.cookies[0].same_site, BrowserCookieSameSite::Lax);
    }

    #[test]
    fn parses_netscape_http_only_and_session_cookies() {
        let input = concat!(
            "# Netscape HTTP Cookie File\n",
            "#HttpOnly_.example.com\tTRUE\t/\tTRUE\t1893456000\tsession\tsecret\n",
            "localhost\tFALSE\t/app\tFALSE\t0\ttheme\tdark\n",
        );

        let batch = parse_cookie_import(input).expect("parse Netscape export");
        assert_eq!(batch.skipped, 0);
        assert_eq!(batch.cookies.len(), 2);
        assert!(batch.cookies[0].http_only);
        assert_eq!(batch.cookies[0].domain, ".example.com");
        assert_eq!(
            batch.cookies[0].expires_unix_micros,
            Some(1_893_456_000_000_000)
        );
        assert_eq!(batch.cookies[1].source_url, "http://localhost/app");
        assert_eq!(batch.cookies[1].domain, "");
        assert_eq!(batch.cookies[1].expires_unix_micros, None);
    }

    #[test]
    fn skips_invalid_rfc_cookies_prefixes_and_expired_rows() {
        let json = r#"[
            { "domain": "example.com", "name": "insecure-none", "sameSite": "none", "secure": false, "value": "x" },
            { "domain": "example.com", "name": "__Secure-bad", "secure": false, "value": "x" },
            { "domain": ".example.com", "name": "__Host-domain", "hostOnly": false, "secure": true, "path": "/", "value": "x" },
            { "domain": "example.com", "name": "bad;name", "value": "x" },
            { "domain": "example.com", "name": "ctl", "value": "bad\nvalue" },
            { "domain": ".localhost", "hostOnly": false, "name": "dotlocal", "value": "x" },
            { "domain": "example.com", "name": "valid", "value": "secret" }
        ]"#;
        let batch = parse_cookie_import(json).expect("parse partial JSON export");
        assert_eq!(batch.cookies.len(), 1);
        assert_eq!(batch.cookies[0].name, "valid");
        assert_eq!(batch.skipped, 6);
    }

    #[test]
    fn accepts_bom_crlf_and_empty_netscape_values() {
        let input =
            "\u{feff}# Netscape HTTP Cookie File\r\nexample.com\tFALSE\t/\tFALSE\t0\tempty\t\r\n";
        let batch = parse_cookie_import(input).expect("parse Netscape export");
        assert_eq!(batch.cookies.len(), 1);
        assert!(batch.cookies[0].value.is_empty());
    }

    #[test]
    fn rejects_files_without_supported_cookies() {
        assert!(matches!(
            parse_cookie_import("not a cookie file"),
            Err(CookieImportError::NoUsableCookies { skipped: 1 })
        ));
        assert!(matches!(
            parse_cookie_import("  \n"),
            Err(CookieImportError::Empty)
        ));
    }
}
