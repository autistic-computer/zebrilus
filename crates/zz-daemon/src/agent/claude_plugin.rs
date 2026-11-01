use std::{
    ffi::{OsStr, OsString},
    hash::{DefaultHasher, Hash as _, Hasher as _},
    io::{self, Write as _},
    path::{Path, PathBuf},
    sync::OnceLock,
};

const DIRECTORY: &str = "claude-plugin";
const PLUGIN_DIRS: &str = "CLAUDE_CODE_PLUGIN_DIRS";
pub(crate) const SOCKET: &str = "ZZ_LENS_SOCKET";
const MAX_SOCKET_PATH: usize = 100;

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
    Ok(directory)
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

    use super::{FILES, PLUGIN_DIRS, SOCKET, folder, install, lens_socket, merge, plugin_dirs};

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
}
