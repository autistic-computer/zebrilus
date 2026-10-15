use std::{
    fs, io,
    io::{ErrorKind, Read as _},
    path::{Path, PathBuf},
    time::SystemTime,
};

use zz_client::appearance::is_builtin_theme;

use crate::{atomic_write, config_path_for_write};

pub const THEMES_DIRECTORY_NAME: &str = "themes";
pub const THEME_EXTENSION: &str = "json";
pub const MAX_THEME_BYTES: usize = 64 * 1024;
pub const MAX_THEME_NAME_BYTES: usize = 64;

pub fn is_theme_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_THEME_NAME_BYTES
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub fn themes_directory_for(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(THEMES_DIRECTORY_NAME)
}

pub fn themes_directory() -> io::Result<PathBuf> {
    config_path_for_write().map(|path| themes_directory_for(&path))
}

pub fn theme_path(directory: &Path, name: &str) -> PathBuf {
    directory.join(format!("{name}.{THEME_EXTENSION}"))
}

pub fn saved_theme_names(directory: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension()? == THEME_EXTENSION && path.is_file())
                .then(|| path.file_stem()?.to_str().map(str::to_owned))
                .flatten()
        })
        .filter(|name| is_theme_name(name) && !is_builtin_theme(name))
        .collect::<Vec<_>>();
    names.sort();
    names
}

pub fn read_theme_source(directory: &Path, name: &str) -> io::Result<String> {
    if !is_theme_name(name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("`{name}` is not a theme name"),
        ));
    }
    let file = fs::File::open(theme_path(directory, name))?;
    let byte_limit = u64::try_from(MAX_THEME_BYTES).unwrap_or(u64::MAX - 1);
    let mut source = String::new();
    file.take(byte_limit + 1).read_to_string(&mut source)?;
    if source.len() > MAX_THEME_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("theme exceeds the {MAX_THEME_BYTES}-byte limit"),
        ));
    }
    Ok(source)
}

pub fn save_theme(directory: &Path, name: &str, source: &str) -> io::Result<PathBuf> {
    if !is_theme_name(name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("`{name}` is not a theme name"),
        ));
    }
    if source.len() > MAX_THEME_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("theme exceeds the {MAX_THEME_BYTES}-byte limit"),
        ));
    }
    let path = theme_path(directory, name);
    atomic_write(&path, source.as_bytes())?;
    Ok(path)
}

pub fn next_theme_name(directory: &Path) -> String {
    let taken = saved_theme_names(directory);
    (1..=taken.len() + 1)
        .map(|index| format!("theme-{index}"))
        .find(|name| !taken.contains(name))
        .expect("one more name than themes holds a free one")
}

pub type ThemesStamp = Vec<(String, Option<SystemTime>, u64)>;

pub fn themes_stamp(directory: &Path) -> ThemesStamp {
    saved_theme_names(directory)
        .into_iter()
        .map(|name| {
            let metadata = fs::metadata(theme_path(directory, &name)).ok();
            let modified = metadata
                .as_ref()
                .and_then(|metadata| metadata.modified().ok());
            let len = metadata.as_ref().map_or(0, fs::Metadata::len);
            (name, modified, len)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_are_plain_named_json_files() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let themes = directory.path().join(THEMES_DIRECTORY_NAME);
        assert!(saved_theme_names(&themes).is_empty());
        assert_eq!(next_theme_name(&themes), "theme-1");

        save_theme(&themes, "theme-1", "{}").expect("save theme");
        save_theme(&themes, "Neon_2", r#"{"radius":4}"#).expect("save theme");
        fs::write(themes.join("notes.txt"), "not a theme").expect("write stray file");
        fs::write(themes.join("bad name.json"), "{}").expect("write stray theme");
        fs::write(themes.join("default.json"), "{}").expect("write a built-in id");

        assert_eq!(saved_theme_names(&themes), ["Neon_2", "theme-1"]);
        assert_eq!(next_theme_name(&themes), "theme-2");
        assert_eq!(
            read_theme_source(&themes, "Neon_2").expect("read theme"),
            r#"{"radius":4}"#
        );
        assert!(read_theme_source(&themes, "../config").is_err());
        assert!(save_theme(&themes, "a/b", "{}").is_err());
        assert_eq!(themes_stamp(&themes).len(), 2);
    }
}
