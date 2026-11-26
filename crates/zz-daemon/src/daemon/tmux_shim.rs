use super::*;

#[cfg(unix)]
static TMUX_SHIM_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[cfg(unix)]
pub(super) struct TmuxShimGuard {
    pub(super) directory: PathBuf,
    pub(super) executable: PathBuf,
}

#[cfg(unix)]
fn pinned_executable(directory: &Path, executable: &Path) -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        let _ = directory;
        let own = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
        if own.exists() {
            return own;
        }
        executable.to_path_buf()
    }
    #[cfg(target_os = "macos")]
    {
        shared_pinned_executable(
            &std::env::temp_dir().join(PINNED_ROOT),
            directory,
            executable,
        )
        .unwrap_or_else(|| executable.to_path_buf())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = directory;
        executable.to_path_buf()
    }
}

#[cfg(target_os = "macos")]
pub(super) const PINNED_ROOT: &str = "zz-pinned";

#[cfg(target_os = "macos")]
const PINNED_LINK: &str = "pin";

#[cfg(target_os = "macos")]
pub(super) fn shared_pinned_executable(
    root: &Path,
    directory: &Path,
    executable: &Path,
) -> Option<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};

    let name = executable.file_name()?;
    let metadata = fs::metadata(executable).ok()?;
    let key = format!(
        "{:x}-{:x}-{:x}-{:x}.{:x}",
        metadata.dev(),
        metadata.ino(),
        metadata.size(),
        metadata.mtime(),
        metadata.mtime_nsec()
    );
    let build = root.join(&key);
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&build)
        .ok()?;
    let pinned = build.join(name);
    if !pinned.exists() {
        let staged = build.join(format!(".{}", std::process::id()));
        let _ = fs::remove_file(&staged);
        clone_file(executable, &staged).ok()?;
        let linked = fs::hard_link(&staged, &pinned);
        let _ = fs::remove_file(&staged);
        if let Err(error) = linked
            && error.kind() != ErrorKind::AlreadyExists
        {
            return None;
        }
    }
    fs::hard_link(&pinned, directory.join(PINNED_LINK)).ok()?;
    remove_unused_pinned_builds(root, &key);
    Some(pinned)
}

#[cfg(target_os = "macos")]
fn remove_unused_pinned_builds(root: &Path, current: &str) {
    use std::os::unix::fs::MetadataExt as _;

    let Ok(builds) = fs::read_dir(root) else {
        return;
    };
    for build in builds.flatten() {
        if build.file_name() == current {
            continue;
        }
        let path = build.path();
        let settled = fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| {
                modified
                    .elapsed()
                    .is_ok_and(|age| age > Duration::from_secs(60))
            });
        let referenced = fs::read_dir(&path).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.metadata().is_ok_and(|metadata| metadata.nlink() > 1))
        });
        if settled && !referenced {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "clonefile copies one file the daemon can read into its own directory"
)]
fn clone_file(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;

    unsafe extern "C" {
        fn clonefile(
            src: *const std::ffi::c_char,
            dst: *const std::ffi::c_char,
            flags: u32,
        ) -> std::ffi::c_int;
    }
    let source = std::ffi::CString::new(source.as_os_str().as_bytes())?;
    let target = std::ffi::CString::new(target.as_os_str().as_bytes())?;
    if unsafe { clonefile(source.as_ptr(), target.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
pub(super) fn remove_dead_tmux_shims(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let pid = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_prefix("zz-tmux-shim-"))
            .and_then(|rest| rest.split_once('-'))
            .and_then(|(pid, _)| pid.parse::<u32>().ok());
        if pid.is_some_and(|pid| pid != std::process::id() && !crate::pid_alive(pid)) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(unix)]
impl TmuxShimGuard {
    pub(super) fn install(executable: PathBuf, pin: bool) -> std::io::Result<Self> {
        use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};

        remove_dead_tmux_shims(&std::env::temp_dir());
        let directory = loop {
            let sequence = TMUX_SHIM_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir()
                .join(format!("zz-tmux-shim-{}-{sequence}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            match builder.create(&directory) {
                Ok(()) => break directory,
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        };
        let write = |name: &str, script: &[u8]| -> std::io::Result<()> {
            let mut options = fs::OpenOptions::new();
            options.create_new(true).write(true).mode(0o700);
            let mut file = options.open(directory.join(name))?;
            file.write_all(script)?;
            file.flush()
        };
        if let Err(error) = write("tmux", b"#!/bin/sh\nif [ \"$#\" -eq 0 ] && [ -n \"${TMUX-}\" ]; then\n    printf '%s\\n' 'sessions should be nested with care, unset $TMUX to force' >&2\n    exit 1\nfi\nexec \"$ZZ_TMUX_EXECUTABLE\" \"$@\"\n") {
            let _ = fs::remove_file(directory.join("tmux"));
            let _ = fs::remove_dir(&directory);
            return Err(error);
        }
        let executable = if pin {
            pinned_executable(&directory, &executable)
        } else {
            executable
        };
        if !directory.join("zz").exists()
            && let Err(error) = write("zz", b"#!/bin/sh\nexec \"$ZZ_TMUX_EXECUTABLE\" \"$@\"\n")
        {
            drop(Self {
                directory,
                executable,
            });
            return Err(error);
        }
        Ok(Self {
            directory,
            executable,
        })
    }
}

#[cfg(unix)]
impl Drop for TmuxShimGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        let _ = fs::remove_file(self.directory.join(PINNED_LINK));
        let _ = fs::remove_file(self.directory.join("tmux"));
        let _ = fs::remove_file(self.directory.join("zz"));
        let _ = fs::remove_dir(&self.directory);
    }
}
