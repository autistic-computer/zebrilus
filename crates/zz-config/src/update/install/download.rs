use std::{
    fmt::Write as _,
    fs::{self, File},
    io::{ErrorKind, Read as _, Write as _},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use sha2::{Digest as _, Sha256};

use super::{Progress, UpdateError};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const CHUNK: usize = 64 * 1024;
const USER_AGENT: &str = concat!("zz/", env!("CARGO_PKG_VERSION"));

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(RESPONSE_TIMEOUT))
        .timeout_per_read(Some(READ_TIMEOUT))
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>, UpdateError> {
    agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| UpdateError::Network(error.to_string()))
}

pub(super) fn expected_checksum(url: &str) -> Result<String, UpdateError> {
    let text = get(&agent(), url)?
        .body_mut()
        .read_to_string()
        .map_err(|error| UpdateError::Network(error.to_string()))?;
    parse_checksum(&text).ok_or(UpdateError::UnreadableChecksum)
}

pub(super) fn parse_checksum(text: &str) -> Option<String> {
    let token = text.split_whitespace().next()?;
    (token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| token.to_ascii_lowercase())
}

pub(super) fn fetch(
    url: &str,
    destination: &Path,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(Progress),
) -> Result<String, UpdateError> {
    let result = stream(url, destination, cancel, on_progress);
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn stream(
    url: &str,
    destination: &Path,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(Progress),
) -> Result<String, UpdateError> {
    let mut response = get(&agent(), url)?;
    let total = response.body().content_length();
    let mut reader = response.body_mut().as_reader();
    let mut file = File::create(destination)
        .map_err(|source| UpdateError::io("create", destination, source))?;
    let mut hasher = Hasher::default();
    let mut buffer = vec![0; CHUNK];
    let mut received = 0;
    let mut reported = Instant::now();
    on_progress(Progress::Downloading { received, total });
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(UpdateError::Cancelled);
        }
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(UpdateError::Network(error.to_string())),
        };
        let chunk = &buffer[..read];
        hasher.update(chunk);
        file.write_all(chunk)
            .map_err(|source| UpdateError::io("write", destination, source))?;
        received += read as u64;
        if reported.elapsed() >= PROGRESS_INTERVAL {
            on_progress(Progress::Downloading { received, total });
            reported = Instant::now();
        }
    }
    file.sync_all()
        .map_err(|source| UpdateError::io("write", destination, source))?;
    on_progress(Progress::Downloading { received, total });
    Ok(hasher.finish())
}

pub(super) fn verify(file: &Path, expected: &str, actual: &str) -> Result<(), UpdateError> {
    if expected == actual {
        return Ok(());
    }
    let _ = fs::remove_file(file);
    Err(UpdateError::ChecksumMismatch {
        expected: expected.to_owned(),
        actual: actual.to_owned(),
    })
}

#[derive(Default)]
pub(super) struct Hasher(Sha256);

impl Hasher {
    pub(super) fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub(super) fn finish(self) -> String {
        let mut hex = String::with_capacity(64);
        for byte in self.0.finalize() {
            let _ = write!(hex, "{byte:02x}");
        }
        hex
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_that_does_not_match_its_checksum_is_refused_and_deleted() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let file = directory.path().join("zz-0.23.0-macos-arm64.dmg");
        fs::write(&file, b"abc").expect("write the download");
        let mut hasher = Hasher::default();
        hasher.update(b"abc");
        let actual = hasher.finish();
        let expected = parse_checksum(
            "0000000000000000000000000000000000000000000000000000000000000000  zz-0.23.0-macos-arm64.dmg\n",
        )
        .expect("a checksum line");

        let error = verify(&file, &expected, &actual).expect_err("a mismatch");

        assert!(
            matches!(
                &error,
                UpdateError::ChecksumMismatch { expected, actual }
                    if expected == "0000000000000000000000000000000000000000000000000000000000000000"
                        && actual == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            ),
            "{error:?}"
        );
        assert!(!file.exists());
        assert_eq!(parse_checksum("ABC  zz.dmg"), None);
        assert_eq!(
            parse_checksum(
                "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD  zz.dmg"
            )
            .as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert!(verify(&file, &actual, &actual).is_ok());
    }
}
