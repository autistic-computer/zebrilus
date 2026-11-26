use super::*;

pub(super) fn expand_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        return home_directory().map_or_else(|| PathBuf::from(path), |home| home.join(rest));
    }
    PathBuf::from(path)
}

#[cfg(unix)]
pub(super) fn source_working_directory_glob_prefix(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};

    let bytes = path.as_os_str().as_bytes();
    let mut quoted = Vec::with_capacity(bytes.len().saturating_mul(2));
    for &byte in bytes {
        if byte.is_ascii() && !byte.is_ascii_alphanumeric() && byte != b'/' {
            quoted.push(b'\\');
        }
        quoted.push(byte);
    }
    PathBuf::from(std::ffi::OsString::from_vec(quoted))
}

#[cfg(not(unix))]
pub(super) fn source_working_directory_glob_prefix(path: &Path) -> PathBuf {
    PathBuf::from(glob::Pattern::escape(path.to_string_lossy().as_ref()))
}

#[cfg(test)]
pub(super) fn literal_directory_name(prefix: &str) -> String {
    let specials = if cfg!(windows) {
        "[literal]"
    } else {
        "[literal]*?"
    };
    format!("{prefix} {specials} with spaces")
}

pub(super) fn resolve_source_path(path: &RawText, working_directory: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(path.to_os_string());
    if path.is_absolute() {
        path
    } else {
        working_directory.map_or(path.clone(), |working_directory| {
            source_working_directory_glob_prefix(working_directory).join(path)
        })
    }
}

#[derive(Default)]
pub(super) struct SourceGlobMatches {
    pub(super) paths: Vec<PathBuf>,
    pub(super) errors: Vec<String>,
}

pub(super) const STANDARD_INPUT_SOURCE_WARNING: &str =
    "source-file from standard input is not supported";

pub(super) fn spent_source_stream_error() -> String {
    source_glob_error_warning(Path::new("-"), "Bad file descriptor")
}

pub(super) fn caller_stdin_read_failure(message: &str) -> bool {
    message == spent_source_stream_error()
        || message == source_glob_error_warning(Path::new("-"), "Input/output error")
}

pub(super) fn source_glob_error_warning(path: &Path, error: &str) -> String {
    format!("{error}: {}", path.display())
}

pub(super) fn filesystem_error_message(error: &std::io::Error) -> String {
    let message = error.to_string();
    error.raw_os_error().map_or(message.clone(), |code| {
        #[cfg(not(unix))]
        if let Some(text) = posix_error_text(error.kind()) {
            return text.to_owned();
        }
        message
            .strip_suffix(&format!(" (os error {code})"))
            .unwrap_or(&message)
            .to_owned()
    })
}

#[cfg(not(unix))]
fn posix_error_text(kind: ErrorKind) -> Option<&'static str> {
    Some(match kind {
        ErrorKind::NotFound => "No such file or directory",
        ErrorKind::PermissionDenied => "Permission denied",
        ErrorKind::IsADirectory => "Is a directory",
        ErrorKind::NotADirectory => "Not a directory",
        ErrorKind::InvalidInput | ErrorKind::InvalidFilename => "Invalid argument",
        ErrorKind::OutOfMemory => "Cannot allocate memory",
        _ => return None,
    })
}

pub(super) fn source_read_error_warning(path: &Path, error: &std::io::Error) -> String {
    source_glob_error_warning(path, &filesystem_error_message(error))
}

pub(super) fn client_source_read_error_warning(
    command_client: bool,
    path: &Path,
    error: &std::io::Error,
) -> String {
    if command_client && error.kind() == ErrorKind::IsADirectory {
        return source_glob_error_warning(path, "Input/output error");
    }
    source_read_error_warning(path, error)
}

pub(super) fn missing_source_error(path: &Path) -> String {
    format!("No such file or directory: {}", path.display())
}

#[cfg(unix)]
#[allow(unsafe_code, clippy::undocumented_unsafe_blocks)]
pub(super) fn source_glob_matches(path: &Path) -> SourceGlobMatches {
    use std::{
        ffi::{CStr, CString},
        mem::MaybeUninit,
        os::unix::ffi::{OsStrExt as _, OsStringExt as _},
    };

    let error_message = |errno| {
        unsafe { CStr::from_ptr(libc::strerror(errno)) }
            .to_string_lossy()
            .into_owned()
    };
    let Ok(pattern) = CString::new(path.as_os_str().as_bytes()) else {
        return SourceGlobMatches {
            paths: Vec::new(),
            errors: vec![error_message(libc::EINVAL)],
        };
    };
    let mut state = MaybeUninit::<libc::glob_t>::zeroed();
    let result = unsafe { libc::glob(pattern.as_ptr(), 0, None, state.as_mut_ptr()) };
    let mut state = unsafe { state.assume_init() };
    let mut matches = SourceGlobMatches::default();
    if result == 0 {
        for index in 0..state.gl_pathc {
            let path = unsafe { *state.gl_pathv.add(index) };
            if !path.is_null() {
                let bytes = unsafe { CStr::from_ptr(path) }.to_bytes().to_vec();
                matches
                    .paths
                    .push(PathBuf::from(std::ffi::OsString::from_vec(bytes)));
            }
        }
    } else if result == libc::GLOB_NOSPACE {
        matches.errors.push(error_message(libc::ENOMEM));
    } else if result != libc::GLOB_NOMATCH {
        matches.errors.push(error_message(libc::EINVAL));
    }
    unsafe { libc::globfree(&raw mut state) };
    matches
}

#[cfg(not(unix))]
pub(super) fn source_glob_matches(path: &Path) -> SourceGlobMatches {
    match glob::glob(path.to_string_lossy().as_ref()) {
        Ok(paths) => {
            let mut matches = SourceGlobMatches::default();
            for path in paths {
                match path {
                    Ok(path) => matches.paths.push(path),
                    Err(error) => matches.errors.push(error.to_string()),
                }
            }
            matches.paths.sort();
            matches
        }
        Err(_) => SourceGlobMatches {
            paths: path
                .exists()
                .then(|| path.to_path_buf())
                .into_iter()
                .collect(),
            errors: Vec::new(),
        },
    }
}

#[cfg(test)]
pub(super) fn tmux_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "\\\\")
}
