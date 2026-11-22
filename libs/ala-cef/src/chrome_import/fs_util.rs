use std::{
    fs::{self, File, OpenOptions},
    io::{self, ErrorKind, Write as _},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

use rusqlite::{Connection, ErrorCode, OpenFlags};
use tempfile::TempDir;

const CONFIG_FILE_NAME: &str = "config";
const SNAPSHOT_RETRY_ATTEMPTS: u32 = 3;
static CONFIG_TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "windows")]
const ERROR_SHARING_VIOLATION: i32 = 32;
#[cfg(target_os = "windows")]
const ERROR_LOCK_VIOLATION: i32 = 33;

pub(crate) enum SqliteSnapshotError {
    DatabaseLocked,
    DatabaseTooLarge(u64),
    Io(io::Error),
    Database(rusqlite::Error),
}

pub(crate) fn with_sqlite_snapshot<T, E>(
    database: &Path,
    temp_prefix: &str,
    snapshot_name: &str,
    max_bytes: u64,
    mut read: impl FnMut(&Connection) -> Result<T, E>,
) -> Result<T, E>
where
    E: From<SqliteSnapshotError> + AsSqliteError,
{
    for attempt in 0..SNAPSHOT_RETRY_ATTEMPTS {
        let snapshot = match snapshot_database(database, temp_prefix, snapshot_name, max_bytes) {
            Ok(snapshot) => snapshot,
            Err(SqliteSnapshotError::Io(error)) if is_sharing_violation(&error) => {
                return Err(SqliteSnapshotError::DatabaseLocked.into());
            }
            Err(error) => return Err(error.into()),
        };
        let (_dir, snapshot_path) = snapshot;
        let connection = match Connection::open_with_flags(
            &snapshot_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(connection) => connection,
            Err(error)
                if is_retryable_sqlite_error(&error) && attempt + 1 < SNAPSHOT_RETRY_ATTEMPTS =>
            {
                thread::sleep(Duration::from_millis(40 * u64::from(attempt + 1)));
                continue;
            }
            Err(error) => return Err(SqliteSnapshotError::Database(error).into()),
        };
        match read(&connection) {
            Ok(value) => return Ok(value),
            Err(error)
                if error
                    .as_sqlite_error()
                    .is_some_and(is_retryable_sqlite_error)
                    && attempt + 1 < SNAPSHOT_RETRY_ATTEMPTS =>
            {
                drop(connection);
                thread::sleep(Duration::from_millis(40 * u64::from(attempt + 1)));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("snapshot loop always returns on final attempt")
}

pub(crate) trait AsSqliteError {
    fn as_sqlite_error(&self) -> Option<&rusqlite::Error>;
}

fn snapshot_database(
    database: &Path,
    temp_prefix: &str,
    snapshot_name: &str,
    max_bytes: u64,
) -> Result<(TempDir, PathBuf), SqliteSnapshotError> {
    let database_bytes = fs::metadata(database)
        .map_err(SqliteSnapshotError::Io)?
        .len();
    if database_bytes > max_bytes {
        return Err(SqliteSnapshotError::DatabaseTooLarge(database_bytes));
    }

    let directory = tempfile::Builder::new()
        .prefix(temp_prefix)
        .tempdir()
        .map_err(SqliteSnapshotError::Io)?;
    let snapshot = directory.path().join(snapshot_name);
    let mut snapshot_bytes = fs::copy(database, &snapshot).map_err(SqliteSnapshotError::Io)?;
    ensure_writable(&snapshot);
    if snapshot_bytes > max_bytes {
        return Err(SqliteSnapshotError::DatabaseTooLarge(snapshot_bytes));
    }
    for suffix in ["-wal", "-journal"] {
        let source = sidecar_path(database, suffix);
        let target = sidecar_path(&snapshot, suffix);
        match fs::metadata(&source) {
            Ok(metadata) => {
                let expected_bytes = snapshot_bytes.saturating_add(metadata.len());
                if expected_bytes > max_bytes {
                    return Err(SqliteSnapshotError::DatabaseTooLarge(expected_bytes));
                }
                match fs::copy(source, &target) {
                    Ok(copied) => {
                        ensure_writable(&target);
                        snapshot_bytes = snapshot_bytes.saturating_add(copied);
                        if snapshot_bytes > max_bytes {
                            return Err(SqliteSnapshotError::DatabaseTooLarge(snapshot_bytes));
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => return Err(SqliteSnapshotError::Io(error)),
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(SqliteSnapshotError::Io(error)),
        }
    }
    Ok((directory, snapshot))
}

fn ensure_writable(path: &Path) {
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            #[allow(
                clippy::permissions_set_readonly_false,
                reason = "the copied snapshot lives in an isolated temporary directory and must allow SQLite WAL recovery"
            )]
            permissions.set_readonly(false);
            let _ = fs::set_permissions(path, permissions);
        }
    }
}

fn sidecar_path(database: &Path, suffix: &str) -> PathBuf {
    let mut path = database.as_os_str().to_owned();
    path.push(suffix);
    PathBuf::from(path)
}

#[cfg(target_os = "windows")]
fn is_sharing_violation(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
    )
}

#[cfg(not(target_os = "windows"))]
const fn is_sharing_violation(_error: &io::Error) -> bool {
    false
}

fn is_retryable_sqlite_error(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(
            ErrorCode::DatabaseBusy
                | ErrorCode::DatabaseLocked
                | ErrorCode::DatabaseCorrupt
                | ErrorCode::NotADatabase
                | ErrorCode::SystemIoFailure
                | ErrorCode::CannotOpen
        )
    )
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "configuration path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let (temporary_path, mut temporary_file) = create_config_temp_file(path, parent)?;
    let write_result = (|| {
        if let Ok(metadata) = fs::metadata(path) {
            temporary_file.set_permissions(metadata.permissions())?;
        }
        temporary_file.write_all(contents)?;
        temporary_file.sync_all()
    })();
    drop(temporary_file);

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(())
}

fn create_config_temp_file(path: &Path, parent: &Path) -> io::Result<(PathBuf, File)> {
    let file_name = path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new(CONFIG_FILE_NAME))
        .to_string_lossy();
    for _ in 0..128 {
        let nonce = CONFIG_TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary_path =
            parent.join(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        "could not allocate a unique temporary configuration file",
    ))
}

#[cfg(unix)]
pub(crate) fn restrict_to_current_user(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "both implementations share the fallible unix signature"
)]
pub(crate) fn restrict_to_current_user(_: &Path) -> io::Result<()> {
    Ok(())
}
