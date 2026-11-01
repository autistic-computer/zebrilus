use std::{
    ffi::{OsStr, OsString},
    hash::{DefaultHasher, Hash as _, Hasher as _},
    io::{self, Write as _},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, SystemTime},
};

const DIRECTORY: &str = "claude-plugin";
const PLUGIN_DIRS: &str = "CLAUDE_CODE_PLUGIN_DIRS";
pub(crate) const SOCKET: &str = "ZZ_LENS_SOCKET";
const MAX_SOCKET_PATH: usize = 100;
const KEEP_UNUSED: Duration = Duration::from_hours(30 * 24);
const MAX_PRUNE_DEPTH: usize = 6;

const FILES: &[(&str, &str)] = &[
    (
        ".claude-plugin/plugin.json",
        include_str!("../../assets/claude-plugin/.claude-plugin/plugin.json"),
    ),
    (
        "hooks/hooks.json",
        include_str!("../../assets/claude-plugin/hooks/hooks.json"),
    ),
    (
        "hooks/register.ts",
        include_str!("../../assets/claude-plugin/hooks/register.ts"),
    ),
];

pub(crate) fn install(root: &Path) -> io::Result<PathBuf> {
    let directory = root.join(folder(FILES));
    for (name, text) in FILES {
        let path = directory.join(name);
        if std::fs::read(&path).is_ok_and(|old| old == text.as_bytes()) {
            continue;
        }
        let parent = path.parent().unwrap_or(&directory);
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(text.as_bytes())?;
        file.persist(&path).map_err(|error| error.error)?;
    }
    touch(&directory);
    prune(root, &directory, SystemTime::now());
    Ok(directory)
}

fn touch(directory: &Path) {
    drop(std::fs::File::open(directory).and_then(|folder| folder.set_modified(SystemTime::now())));
}

fn prune(root: &Path, ours: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let ours_or_other = path == ours
            || !entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix(DIRECTORY)?.strip_prefix('-'))
                .is_some_and(|hash| {
                    hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                });
        if ours_or_other || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let unused = newest_in(&path, MAX_PRUNE_DEPTH)
            .and_then(|newest| now.duration_since(newest).ok())
            .is_some_and(|age| age >= KEEP_UNUSED);
        if unused && let Err(error) = std::fs::remove_dir_all(&path) {
            log::debug!(target: "zz::agent", "could not remove {}: {error}", path.display());
        }
    }
}

fn newest_in(path: &Path, depth: usize) -> Option<SystemTime> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    let mut newest = metadata.modified().ok()?;
    if metadata.is_dir() {
        if depth == 0 {
            return None;
        }
        for entry in std::fs::read_dir(path).ok()? {
            newest = newest.max(newest_in(&entry.ok()?.path(), depth - 1)?);
        }
    }
    Some(newest)
}

fn folder(files: &[(&str, &str)]) -> String {
    let mut hasher = DefaultHasher::new();
    files.hash(&mut hasher);
    format!("{DIRECTORY}-{:016x}", hasher.finish())
}

fn installed() -> Option<&'static Path> {
    static PLUGIN: OnceLock<Option<PathBuf>> = OnceLock::new();
    PLUGIN
        .get_or_init(|| {
            if cfg!(test) {
                return None;
            }
            let root = super::paths::daemon_data_dir().ok()?;
            install(&root)
                .inspect_err(|error| {
                    log::warn!(target: "zz::agent", "could not write the Claude Code plugin: {error}");
                })
                .ok()
        })
        .as_deref()
}

pub(crate) fn lens_socket(daemon_socket: &Path) -> Option<PathBuf> {
    let stem = daemon_socket.file_stem()?.to_str()?;
    let path = daemon_socket.with_file_name(format!("{stem}-lens.sock"));
    (path.as_os_str().len() <= MAX_SOCKET_PATH).then_some(path)
}

pub(crate) fn plugin_dirs(inherited: Option<&OsStr>, ours: &Path) -> Option<OsString> {
    let mut dirs = inherited
        .map(|value| {
            std::env::split_paths(value)
                .filter(|path| !path.as_os_str().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !dirs.iter().any(|dir| dir == ours) {
        dirs.push(ours.to_path_buf());
    }
    std::env::join_paths(dirs).ok()
}

pub(crate) fn pane_environment(
    environment: &mut Vec<(OsString, Option<OsString>)>,
    hub: impl FnOnce() -> Option<PathBuf>,
) {
    let Some(plugin) = installed() else {
        return;
    };
    let Some(socket) = hub() else {
        return;
    };
    touch(plugin);
    merge(environment, plugin, socket, std::env::var_os(PLUGIN_DIRS));
}

fn merge(
    environment: &mut Vec<(OsString, Option<OsString>)>,
    plugin: &Path,
    socket: PathBuf,
    process: Option<OsString>,
) {
    let inherited = environment
        .iter()
        .rev()
        .find(|(name, _)| name == PLUGIN_DIRS)
        .map_or(process, |(_, value)| value.clone());
    let Some(dirs) = plugin_dirs(inherited.as_deref(), plugin) else {
        return;
    };
    environment.push((PLUGIN_DIRS.into(), Some(dirs)));
    environment.push((SOCKET.into(), Some(socket.into_os_string())));
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    use super::{
        FILES, KEEP_UNUSED, PLUGIN_DIRS, SOCKET, folder, install, lens_socket, merge, plugin_dirs,
        prune,
    };

    fn value(environment: &[(OsString, Option<OsString>)], name: &str) -> Option<OsString> {
        environment
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .and_then(|(_, value)| value.clone())
    }

    #[test]
    fn the_plugin_is_written_once_and_again_only_when_it_changed() {
        let root = tempfile::tempdir().unwrap();
        let directory = install(root.path()).unwrap();
        for (name, text) in FILES {
            assert_eq!(
                std::fs::read_to_string(directory.join(name)).unwrap(),
                *text
            );
        }
        let manifest = directory.join(".claude-plugin/plugin.json");
        let written = std::fs::metadata(&manifest).unwrap().modified().unwrap();
        std::fs::write(directory.join("hooks/register.ts"), "stale").unwrap();
        std::fs::create_dir_all(directory.join(".claude-plugin/types")).unwrap();
        std::fs::write(directory.join(".claude-plugin/types/index.d.ts"), "kept").unwrap();

        install(root.path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(directory.join("hooks/register.ts")).unwrap(),
            FILES[2].1
        );
        assert_eq!(
            std::fs::metadata(&manifest).unwrap().modified().unwrap(),
            written
        );
        assert_eq!(
            std::fs::read_to_string(directory.join(".claude-plugin/types/index.d.ts")).unwrap(),
            "kept"
        );
    }

    #[test]
    fn each_version_of_the_plugin_gets_its_own_folder() {
        let root = tempfile::tempdir().unwrap();
        let older = root.path().join(folder(&[("hooks/register.ts", "older")]));
        std::fs::create_dir_all(older.join("hooks")).unwrap();
        std::fs::write(older.join("hooks/register.ts"), "older").unwrap();

        let directory = install(root.path()).unwrap();

        assert_eq!(directory.file_name().unwrap(), folder(FILES).as_str());
        assert_ne!(directory, older);
        assert_eq!(folder(FILES), folder(FILES));
        assert_eq!(
            std::fs::read_to_string(older.join("hooks/register.ts")).unwrap(),
            "older"
        );
    }

    #[test]
    fn the_plugin_folder_joins_what_the_pane_already_had() {
        let ours = Path::new("/data/zz/claude-plugin");
        assert_eq!(
            plugin_dirs(None, ours),
            Some(OsString::from("/data/zz/claude-plugin"))
        );
        assert_eq!(
            plugin_dirs(Some("/mine:/theirs".as_ref()), ours),
            Some(OsString::from("/mine:/theirs:/data/zz/claude-plugin"))
        );
        assert_eq!(
            plugin_dirs(Some("/mine:/data/zz/claude-plugin".as_ref()), ours),
            Some(OsString::from("/mine:/data/zz/claude-plugin"))
        );
        assert_eq!(plugin_dirs(None, Path::new("/odd:path")), None);
    }

    #[test]
    fn the_pane_gets_the_plugin_and_its_socket_without_losing_the_users_folders() {
        let plugin = Path::new("/data/zz/claude-plugin");
        let socket = PathBuf::from("/tmp/zz-me/default-lens.sock");

        let mut environment = vec![(OsString::from(PLUGIN_DIRS), Some(OsString::from("/mine")))];
        merge(
            &mut environment,
            plugin,
            socket.clone(),
            Some("/process".into()),
        );
        assert_eq!(
            value(&environment, PLUGIN_DIRS),
            Some(OsString::from("/mine:/data/zz/claude-plugin"))
        );
        assert_eq!(
            value(&environment, SOCKET),
            Some(socket.clone().into_os_string())
        );

        let mut environment = Vec::new();
        merge(
            &mut environment,
            plugin,
            socket.clone(),
            Some("/process".into()),
        );
        assert_eq!(
            value(&environment, PLUGIN_DIRS),
            Some(OsString::from("/process:/data/zz/claude-plugin"))
        );

        let mut environment = vec![(OsString::from(PLUGIN_DIRS), None)];
        merge(&mut environment, plugin, socket, Some("/process".into()));
        assert_eq!(
            value(&environment, PLUGIN_DIRS),
            Some(OsString::from("/data/zz/claude-plugin"))
        );
    }

    #[test]
    fn the_lens_socket_sits_beside_the_daemons() {
        assert_eq!(
            lens_socket(Path::new("/tmp/zz-me/default.sock")),
            Some(PathBuf::from("/tmp/zz-me/default-lens.sock"))
        );
        assert_eq!(
            lens_socket(Path::new("/tmp/zz-me/default")),
            Some(PathBuf::from("/tmp/zz-me/default-lens.sock"))
        );
        let long = format!("/tmp/{}/default.sock", "x".repeat(90));
        assert_eq!(lens_socket(Path::new(&long)), None);
    }

    fn age(path: &Path, when: SystemTime) {
        for entry in walk(path) {
            std::fs::File::open(&entry)
                .unwrap()
                .set_modified(when)
                .unwrap();
        }
    }

    fn walk(path: &Path) -> Vec<PathBuf> {
        let mut paths = vec![path.to_path_buf()];
        if path.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                paths.extend(walk(&entry.unwrap().path()));
            }
        }
        paths
    }

    fn plugin(root: &Path, hash: &str) -> PathBuf {
        let directory = root.join(format!("claude-plugin-{hash}"));
        std::fs::create_dir_all(directory.join(".claude-plugin/types")).unwrap();
        std::fs::write(directory.join(".claude-plugin/types/index.d.ts"), "types").unwrap();
        directory
    }

    #[test]
    fn plugin_folders_unused_for_a_month_are_pruned_and_the_rest_stay() {
        let root = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        let old = now - KEEP_UNUSED - Duration::from_hours(1);
        let ours = plugin(root.path(), "0000000000000001");
        age(&ours, old);
        let stale = plugin(root.path(), "00000000000000a2");
        age(&stale, old);
        let in_use = plugin(root.path(), "00000000000000a3");
        age(&in_use, old);
        std::fs::File::open(in_use.join(".claude-plugin/types/index.d.ts"))
            .unwrap()
            .set_modified(now - Duration::from_hours(24 * 20))
            .unwrap();
        let young = plugin(root.path(), "00000000000000a4");
        let foreign = root.path().join("claude-plugin-notours");
        std::fs::create_dir_all(&foreign).unwrap();
        age(&foreign, old);
        let journal = root.path().join("agent-journal");
        std::fs::create_dir_all(&journal).unwrap();
        age(&journal, old);
        let linked = root.path().join("claude-plugin-00000000000000a5");
        std::os::unix::fs::symlink(&stale, &linked).unwrap();

        prune(root.path(), &ours, now);

        assert!(ours.exists());
        assert!(!stale.exists());
        assert!(in_use.exists());
        assert!(young.exists());
        assert!(foreign.exists());
        assert!(journal.exists());
        assert!(std::fs::symlink_metadata(&linked).is_ok());
    }

    #[test]
    fn installing_marks_the_folder_in_use_and_prunes_the_others() {
        let root = tempfile::tempdir().unwrap();
        let old = SystemTime::now() - KEEP_UNUSED - Duration::from_hours(1);
        let directory = install(root.path()).unwrap();
        age(&directory, old);
        let stale = plugin(root.path(), "00000000000000b1");
        age(&stale, old);

        assert_eq!(install(root.path()).unwrap(), directory);

        assert!(!stale.exists());
        let touched = std::fs::metadata(&directory).unwrap().modified().unwrap();
        assert!(touched > old + Duration::from_hours(1));
    }
}
