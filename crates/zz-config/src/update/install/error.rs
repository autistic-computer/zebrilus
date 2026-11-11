use std::{error::Error, fmt, io, path::PathBuf, process::ExitStatus};

#[derive(Debug)]
pub enum UpdateError {
    Unavailable,
    NoCacheDirectory,
    Network(String),
    UnreadableChecksum,
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
    Signature(String),
    NotWritable(PathBuf),
    Tool {
        program: String,
        status: ExitStatus,
        stderr: String,
    },
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Cancelled,
}

impl UpdateError {
    pub(super) fn io(action: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => {
                f.write_str("this release has no download for this kind of install")
            }
            Self::NoCacheDirectory => f.write_str("could not find the user's cache directory"),
            Self::Network(message) => write!(f, "could not download the update: {message}"),
            Self::UnreadableChecksum => f.write_str("the release's checksum file is unreadable"),
            Self::ChecksumMismatch { expected, actual } => write!(
                f,
                "the download does not match its checksum (expected {expected}, got {actual})"
            ),
            Self::Signature(message) => {
                write!(f, "the update failed its signature check: {message}")
            }
            Self::NotWritable(path) => write!(f, "cannot write to {}", path.display()),
            Self::Tool {
                program,
                status,
                stderr,
            } if stderr.is_empty() => write!(f, "{program} failed ({status})"),
            Self::Tool {
                program,
                status,
                stderr,
            } => write!(f, "{program} failed ({status}): {stderr}"),
            Self::Io {
                action,
                path,
                source,
            } => write!(f, "could not {action} {}: {source}", path.display()),
            Self::Cancelled => f.write_str("the update was cancelled"),
        }
    }
}

impl Error for UpdateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
