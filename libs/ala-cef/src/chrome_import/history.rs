use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
};

use thiserror::Error;
use url::Url;

use super::{
    fs_util::{AsSqliteError, SqliteSnapshotError, with_sqlite_snapshot},
    profiles::{ResolvedBrowserProfile, resolve_profile_source},
};

const CHROMIUM_EPOCH_OFFSET_MICROS: i64 = 11_644_473_600_000_000;
const MAX_HISTORY_DATABASE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_HISTORY_IMPORT_COUNT: usize = 5_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedPage {
    pub url: String,
    pub title: String,
    pub visited_at: u64,
    pub visit_count: u32,
    pub typed_count: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct ImportLimits {
    pub max_count: usize,
    pub max_url_bytes: usize,
    pub max_title_bytes: usize,
}

#[derive(Debug, Error)]
pub enum ChromeHistoryImportError {
    #[error("the selected browser profile is invalid")]
    InvalidProfile,
    #[error("the browser history database was not found")]
    DatabaseNotFound,
    #[error("the browser is holding its history database open — close it first and try again")]
    DatabaseLocked,
    #[error("the browser history database is unexpectedly large ({0} bytes)")]
    DatabaseTooLarge(u64),
    #[error("the selected browser profile did not have importable history")]
    NoHistory,
    #[error("could not read the browser history database: {0}")]
    Io(#[from] io::Error),
    #[error("could not query the browser history database: {0}")]
    Database(#[from] rusqlite::Error),
}

impl ChromeHistoryImportError {
    pub fn is_permission_denied(&self) -> bool {
        matches!(self, Self::Io(error) if error.kind() == io::ErrorKind::PermissionDenied)
    }
}

impl From<SqliteSnapshotError> for ChromeHistoryImportError {
    fn from(error: SqliteSnapshotError) -> Self {
        match error {
            SqliteSnapshotError::DatabaseLocked => Self::DatabaseLocked,
            SqliteSnapshotError::DatabaseTooLarge(bytes) => Self::DatabaseTooLarge(bytes),
            SqliteSnapshotError::Io(error) => Self::Io(error),
            SqliteSnapshotError::Database(error) => Self::Database(error),
        }
    }
}

impl AsSqliteError for ChromeHistoryImportError {
    fn as_sqlite_error(&self) -> Option<&rusqlite::Error> {
        match self {
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}

pub struct ChromeHistoryImport {
    pub pages: Vec<ImportedPage>,
    pub skipped: usize,
}

pub fn import_history(
    source_profile: &str,
    limits: ImportLimits,
) -> Result<ChromeHistoryImport, ChromeHistoryImportError> {
    let resolved =
        resolve_profile_source(source_profile).ok_or(ChromeHistoryImportError::InvalidProfile)?;
    let database = browser_history_database(&resolved)?;
    import_history_database(&database, limits)
}

fn browser_history_database(
    resolved: &ResolvedBrowserProfile,
) -> Result<PathBuf, ChromeHistoryImportError> {
    let root = resolved
        .browser
        .user_data_dir()
        .ok_or(ChromeHistoryImportError::DatabaseNotFound)?;
    let database = root.join(&resolved.storage_key).join("History");
    match database.try_exists() {
        Ok(true) => Ok(database),
        Ok(false) => Err(ChromeHistoryImportError::DatabaseNotFound),
        Err(error) => Err(error.into()),
    }
}

fn import_history_database(
    database: &Path,
    limits: ImportLimits,
) -> Result<ChromeHistoryImport, ChromeHistoryImportError> {
    let max_count = limits.max_count;
    let sql = format!(
        "SELECT url, title, visit_count, typed_count, last_visit_time \
         FROM urls WHERE url LIKE 'http%' \
         ORDER BY last_visit_time DESC LIMIT {max_count}"
    );
    let rows = with_sqlite_snapshot(
        database,
        "chrome-history-",
        "History",
        MAX_HISTORY_DATABASE_BYTES,
        |connection| {
            let mut statement = connection.prepare(&sql)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, ChromeHistoryImportError>(rows)
        },
    )?;

    let mut pages = Vec::with_capacity(rows.len());
    let mut seen = HashSet::new();
    let mut skipped = 0usize;
    for (url, title, visit_count, typed_count, last_visit_time) in rows {
        let Some(visited_at) = chromium_time_to_unix_seconds(last_visit_time) else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        if visit_count <= 0
            || url.len() > limits.max_url_bytes
            || !is_web_url(&url)
            || !seen.insert(url.clone())
        {
            skipped = skipped.saturating_add(1);
            continue;
        }
        pages.push(ImportedPage {
            url,
            title: truncate_utf8(&title, limits.max_title_bytes),
            visited_at,
            visit_count: u32::try_from(visit_count).unwrap_or(u32::MAX),
            typed_count: u32::try_from(typed_count.max(0)).unwrap_or(u32::MAX),
        });
    }
    if pages.is_empty() {
        return Err(ChromeHistoryImportError::NoHistory);
    }
    Ok(ChromeHistoryImport { pages, skipped })
}

fn is_web_url(value: &str) -> bool {
    Url::parse(value)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

fn chromium_time_to_unix_seconds(value: i64) -> Option<u64> {
    let unix_micros = value.checked_sub(CHROMIUM_EPOCH_OFFSET_MICROS)?;
    u64::try_from(unix_micros.div_euclid(1_000_000)).ok()
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};
    use tempfile::tempdir;

    use super::*;

    const TEST_LIMITS: ImportLimits = ImportLimits {
        max_count: MAX_HISTORY_IMPORT_COUNT,
        max_url_bytes: 4 * 1024,
        max_title_bytes: 2 * 1024,
    };

    #[test]
    fn imports_recent_web_history_and_skips_invalid_rows() {
        let directory = tempdir().expect("temporary Chrome profile");
        let database = directory.path().join("History");
        let connection = Connection::open(&database).expect("create history database");
        connection
            .execute_batch(
                "CREATE TABLE urls ( \
                    url LONGVARCHAR, title LONGVARCHAR, visit_count INTEGER, \
                    typed_count INTEGER, last_visit_time INTEGER \
                 );",
            )
            .expect("create history schema");
        let earlier = CHROMIUM_EPOCH_OFFSET_MICROS + 10_000_000;
        let later = CHROMIUM_EPOCH_OFFSET_MICROS + 20_000_000;
        connection
            .execute(
                "INSERT INTO urls VALUES ('https://one.example', 'One', 1, 0, ?1)",
                params![earlier],
            )
            .expect("insert earlier page");
        connection
            .execute(
                "INSERT INTO urls VALUES ('https://two.example', 'Two', 2, 1, ?1)",
                params![later],
            )
            .expect("insert later page");
        connection
            .execute(
                "INSERT INTO urls VALUES ('https://two.example', 'Duplicate', 1, 0, ?1)",
                params![earlier],
            )
            .expect("insert duplicate page");
        connection
            .execute(
                "INSERT INTO urls VALUES ('http-not-a-url', 'Invalid', 1, 0, ?1)",
                params![later],
            )
            .expect("insert malformed page");
        connection
            .execute(
                "INSERT INTO urls VALUES ('file:///tmp/private', 'File', 1, 0, ?1)",
                params![later],
            )
            .expect("insert non-web page");
        drop(connection);

        let imported = import_history_database(&database, TEST_LIMITS).expect("import history");
        assert_eq!(imported.pages.len(), 2);
        assert_eq!(imported.skipped, 2);
        assert_eq!(imported.pages[0].url, "https://two.example");
        assert_eq!(imported.pages[0].visited_at, 20);
        assert_eq!(imported.pages[0].visit_count, 2);
        assert_eq!(imported.pages[0].typed_count, 1);
        assert_eq!(imported.pages[1].url, "https://one.example");
        assert_eq!(imported.pages[1].visited_at, 10);
    }

    #[test]
    fn snapshots_history_from_a_live_write_ahead_log() {
        let directory = tempdir().expect("temporary Chrome profile");
        let database = directory.path().join("History");
        let connection = Connection::open(&database).expect("create history database");
        let journal_mode = connection
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0))
            .expect("enable WAL mode");
        assert_eq!(journal_mode, "wal");
        connection
            .execute_batch(
                "PRAGMA wal_autocheckpoint=0; \
                 CREATE TABLE urls ( \
                    url LONGVARCHAR, title LONGVARCHAR, visit_count INTEGER, \
                    typed_count INTEGER, last_visit_time INTEGER \
                 );",
            )
            .expect("create history schema");
        connection
            .execute(
                "INSERT INTO urls VALUES ('https://live.example', 'Live', 1, 0, ?1)",
                params![CHROMIUM_EPOCH_OFFSET_MICROS + 30_000_000],
            )
            .expect("insert live page");

        let imported =
            import_history_database(&database, TEST_LIMITS).expect("snapshot active WAL");
        assert_eq!(imported.pages.len(), 1);
        assert_eq!(imported.pages[0].url, "https://live.example");
    }

    #[test]
    fn truncates_titles_on_utf8_boundaries() {
        let title = format!("{}é", "a".repeat(TEST_LIMITS.max_title_bytes - 1));
        let truncated = truncate_utf8(&title, TEST_LIMITS.max_title_bytes);
        assert_eq!(truncated.len(), TEST_LIMITS.max_title_bytes - 1);
        assert!(truncated.is_char_boundary(truncated.len()));
    }
}
