use std::{
    fmt::Write as _,
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::Command,
};

use super::{LOG_TARGET, UpdateError, move_entry, remove_path, run, swap};

const DESKTOP_EXEC: &str = "Exec=zz app";

pub(super) fn stage(prefix: &Path, tarball: &Path, staged: &Path) -> Result<(), UpdateError> {
    remove_path(staged)?;
    let result = unpack(prefix, tarball, staged);
    if result.is_err() {
        let _ = remove_path(staged);
    }
    result
}

fn unpack(prefix: &Path, tarball: &Path, staged: &Path) -> Result<(), UpdateError> {
    fs::create_dir_all(staged).map_err(|source| UpdateError::io("create", staged, source))?;
    run(Command::new("tar")
        .arg("-xzf")
        .arg(tarball)
        .arg("--strip-components=2")
        .arg("-C")
        .arg(staged))?;
    if !staged.join("lib/zz/zz").is_file() {
        return Err(UpdateError::io(
            "find lib/zz/zz in",
            tarball,
            io::Error::from(ErrorKind::NotFound),
        ));
    }
    let desktop = staged.join("share/applications/zz.desktop");
    let entry =
        fs::read_to_string(&desktop).map_err(|source| UpdateError::io("read", &desktop, source))?;
    fs::write(&desktop, desktop_entry(&entry, &prefix.join("bin/zz")))
        .map_err(|source| UpdateError::io("write", &desktop, source))
}

fn desktop_entry(entry: &str, launcher: &Path) -> String {
    let mut patched = String::with_capacity(entry.len() + launcher.as_os_str().len());
    for line in entry.split_inclusive('\n') {
        let (body, ending) = line
            .strip_suffix('\n')
            .map_or((line, ""), |body| (body, "\n"));
        if body == DESKTOP_EXEC {
            let _ = write!(patched, "Exec={} app{ending}", launcher.display());
        } else {
            patched.push_str(line);
        }
    }
    patched
}

pub(super) fn install(prefix: &Path, staged: &Path) -> Result<PathBuf, UpdateError> {
    for part in ["bin", "share"] {
        merge(&staged.join(part), &prefix.join(part))?;
    }
    let lib = prefix.join("lib");
    swap(
        &lib.join("zz"),
        &staged.join("lib/zz"),
        &lib.join(".zz-old"),
    )?;
    if let Err(error) = remove_path(staged) {
        log::warn!(target: LOG_TARGET, "{error}");
    }
    Ok(lib.join("zz/zz"))
}

fn merge(from: &Path, to: &Path) -> Result<(), UpdateError> {
    let entries = match fs::read_dir(from) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(UpdateError::io("read", from, source)),
    };
    fs::create_dir_all(to).map_err(|source| UpdateError::io("create", to, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| UpdateError::io("read", from, source))?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(|error| UpdateError::io("read", &source, error))?;
        if kind.is_dir() {
            merge(&source, &destination)?;
        } else {
            move_entry(&source, &destination)?;
        }
    }
    Ok(())
}

pub(super) fn refresh_desktop_database(prefix: &Path) {
    let applications = prefix.join("share/applications");
    if let Err(error) = run(Command::new("update-desktop-database")
        .arg("-q")
        .arg(&applications))
    {
        log::debug!(target: LOG_TARGET, "skipped the desktop database refresh: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().expect("a parent")).expect("create parents");
        fs::write(path, contents).expect("write file");
    }

    fn names(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .expect("read directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_swap_installs_the_new_files_and_leaves_nothing_behind() {
        let root = tempfile::tempdir().expect("temporary prefix");
        let prefix = root.path();
        write(&prefix.join("lib/zz/zz"), "old");
        write(&prefix.join("lib/zz/stale.so"), "old");
        write(&prefix.join("share/applications/zz.desktop"), "old");
        let staged = prefix.join("lib/.zz-update");
        write(&staged.join("lib/zz/zz"), "new");
        write(
            &staged.join("share/applications/zz.desktop"),
            &desktop_entry("[Desktop Entry]\nExec=zz app\n", &prefix.join("bin/zz")),
        );

        let relaunch = install(prefix, &staged).expect("the swap succeeds");

        assert_eq!(relaunch, prefix.join("lib/zz/zz"));
        assert_eq!(fs::read_to_string(&relaunch).unwrap(), "new");
        assert_eq!(names(&prefix.join("lib")), ["zz"]);
        assert_eq!(names(&prefix.join("lib/zz")), ["zz"]);
        assert_eq!(
            fs::read_to_string(prefix.join("share/applications/zz.desktop")).unwrap(),
            format!(
                "[Desktop Entry]\nExec={} app\n",
                prefix.join("bin/zz").display()
            )
        );
    }

    #[test]
    fn a_failed_swap_keeps_the_old_install() {
        let root = tempfile::tempdir().expect("temporary prefix");
        let prefix = root.path();
        write(&prefix.join("lib/zz/zz"), "old");
        let staged = prefix.join("lib/.zz-update");
        write(&staged.join("bin/unrelated"), "new");

        let error = install(prefix, &staged).expect_err("nothing to swap in");

        assert!(
            matches!(&error, UpdateError::Io { path, .. } if *path == prefix.join("lib/zz")),
            "{error:?}"
        );
        assert_eq!(fs::read_to_string(prefix.join("lib/zz/zz")).unwrap(), "old");
        assert_eq!(names(&prefix.join("lib")), [".zz-update", "zz"]);
    }
}
