use std::{
    fs, io,
    io::{ErrorKind, Read as _},
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{atomic_write, config_path_for_write};

pub const LOOKS_DIRECTORY_NAME: &str = "looks";
pub const LOOK_EXTENSION: &str = "json";
pub const MAX_LOOK_BYTES: usize = 64 * 1024;
pub const MAX_LOOK_NAME_BYTES: usize = 64;

pub fn is_look_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_LOOK_NAME_BYTES
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub fn looks_directory_for(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(LOOKS_DIRECTORY_NAME)
}

pub fn looks_directory() -> io::Result<PathBuf> {
    config_path_for_write().map(|path| looks_directory_for(&path))
}

pub fn look_path(directory: &Path, name: &str) -> PathBuf {
    directory.join(format!("{name}.{LOOK_EXTENSION}"))
}

pub fn saved_look_names(directory: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension()? == LOOK_EXTENSION && path.is_file())
                .then(|| path.file_stem()?.to_str().map(str::to_owned))
                .flatten()
        })
        .filter(|name| is_look_name(name))
        .collect::<Vec<_>>();
    names.sort();
    names
}

pub fn read_look_source(directory: &Path, name: &str) -> io::Result<String> {
    if !is_look_name(name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("`{name}` is not a look name"),
        ));
    }
    let file = fs::File::open(look_path(directory, name))?;
    let byte_limit = u64::try_from(MAX_LOOK_BYTES).unwrap_or(u64::MAX - 1);
    let mut source = String::new();
    file.take(byte_limit + 1).read_to_string(&mut source)?;
    if source.len() > MAX_LOOK_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("look exceeds the {MAX_LOOK_BYTES}-byte limit"),
        ));
    }
    Ok(source)
}

pub fn save_look(directory: &Path, name: &str, source: &str) -> io::Result<PathBuf> {
    if !is_look_name(name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("`{name}` is not a look name"),
        ));
    }
    if source.len() > MAX_LOOK_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("look exceeds the {MAX_LOOK_BYTES}-byte limit"),
        ));
    }
    let path = look_path(directory, name);
    atomic_write(&path, source.as_bytes())?;
    Ok(path)
}

pub fn next_look_name(directory: &Path) -> String {
    let taken = saved_look_names(directory);
    (1..=taken.len() + 1)
        .map(|index| format!("look-{index}"))
        .find(|name| !taken.contains(name))
        .expect("one more name than looks holds a free one")
}

pub type LooksStamp = Vec<(String, Option<SystemTime>, u64)>;

pub fn looks_stamp(directory: &Path) -> LooksStamp {
    saved_look_names(directory)
        .into_iter()
        .map(|name| {
            let metadata = fs::metadata(look_path(directory, &name)).ok();
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
    fn looks_are_plain_named_json_files() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let looks = directory.path().join(LOOKS_DIRECTORY_NAME);
        assert!(saved_look_names(&looks).is_empty());
        assert_eq!(next_look_name(&looks), "look-1");

        save_look(&looks, "look-1", "{}").expect("save look");
        save_look(&looks, "Neon_2", r#"{"radius":4}"#).expect("save look");
        fs::write(looks.join("notes.txt"), "not a look").expect("write stray file");
        fs::write(looks.join("bad name.json"), "{}").expect("write stray look");

        assert_eq!(saved_look_names(&looks), ["Neon_2", "look-1"]);
        assert_eq!(next_look_name(&looks), "look-2");
        assert_eq!(
            read_look_source(&looks, "Neon_2").expect("read look"),
            r#"{"radius":4}"#
        );
        assert!(read_look_source(&looks, "../config").is_err());
        assert!(save_look(&looks, "a/b", "{}").is_err());
        assert_eq!(looks_stamp(&looks).len(), 2);
    }
}
