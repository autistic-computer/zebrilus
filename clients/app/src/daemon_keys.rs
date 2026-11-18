use std::{collections::BTreeMap, fs, io, path::PathBuf};

fn path() -> PathBuf {
    crate::preferences::support_directory().join("daemon-keys.json")
}

fn load() -> BTreeMap<String, String> {
    fs::read(path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(keys: &BTreeMap<String, String>) -> io::Result<()> {
    let path = path();
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    let staged = path.with_extension("json.tmp");
    fs::write(
        &staged,
        serde_json::to_vec_pretty(keys).map_err(io::Error::other)?,
    )?;
    fs::rename(staged, path)
}

pub(crate) fn pinned(endpoint: &str) -> Option<String> {
    load().remove(endpoint)
}

pub(crate) fn pin(endpoint: &str, key: &str) -> io::Result<()> {
    let mut keys = load();
    keys.insert(endpoint.to_owned(), key.to_owned());
    save(&keys)
}

pub(crate) fn forget(endpoint: &str) {
    let mut keys = load();
    if keys.remove(endpoint).is_some()
        && let Err(error) = save(&keys)
    {
        log::warn!("could not forget the saved zz identity of {endpoint}: {error}");
    }
}
