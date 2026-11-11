use std::time::Duration;

use semver::Version;
use serde::Deserialize;

mod install;

pub use install::{
    Assets, Facts, Progress, Staged, Target, UpdateError, apply, assets_for, clear_staged, detect,
    homebrew_cask, prepare, staged, staged_for, tarball_prefix, target_of,
};

const RELEASES_API: &str =
    "https://api.github.com/repos/autistic-computer/zebrilus/releases?per_page=10";
const WIRE_LINE_PREFIX: &str = "<!-- zz-wire:";
const WIRE_LINE_SUFFIX: &str = "-->";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Beta,
}

impl Channel {
    pub fn of(version: &Version) -> Self {
        if version.pre.is_empty() {
            Self::Stable
        } else {
            Self::Beta
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub url: String,
    pub assets: Vec<ReleaseAsset>,
    pub wire: Option<ReleaseWire>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseWire {
    pub epoch: u16,
    pub min_daemon_revision: u32,
}

impl ReleaseWire {
    fn parse(body: &str) -> Option<Self> {
        body.lines().find_map(|line| {
            let fields = line
                .trim()
                .strip_prefix(WIRE_LINE_PREFIX)?
                .strip_suffix(WIRE_LINE_SUFFIX)?;
            let mut epoch = None;
            let mut min_daemon_revision = None;
            for field in fields.split_whitespace() {
                match field.split_once('=') {
                    Some(("epoch", value)) => epoch = value.parse().ok(),
                    Some(("min-daemon-revision", value)) => {
                        min_daemon_revision = value.parse().ok();
                    }
                    _ => {}
                }
            }
            Some(Self {
                epoch: epoch?,
                min_daemon_revision: min_daemon_revision?,
            })
        })
    }

    #[must_use]
    pub const fn keeps_daemon(self, epoch: u16, revision: u32) -> bool {
        self.epoch == epoch && revision >= self.min_daemon_revision
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
}

pub fn build_version() -> Result<Version, String> {
    Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())
}

pub fn checks_enabled() -> bool {
    if zz_protocol::app_identity::DEVELOPMENT {
        return false;
    }
    match std::env::var("ZZ_UPDATE_CHECK") {
        Ok(value) => !matches!(value.as_str(), "" | "0" | "false" | "off"),
        Err(_) => !cfg!(debug_assertions),
    }
}

pub fn fetch_latest(channel: Channel) -> Result<Option<Release>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .into();
    let mut response = agent
        .get(RELEASES_API)
        .header("User-Agent", concat!("zz/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| error.to_string())?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    parse_releases(&body, channel)
}

#[derive(Deserialize)]
struct ReleaseEntry {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
    #[serde(default)]
    body: Option<String>,
}

pub fn parse_releases(json: &str, channel: Channel) -> Result<Option<Release>, String> {
    let entries: Vec<ReleaseEntry> =
        serde_json::from_str(json).map_err(|error| format!("unexpected release list: {error}"))?;
    Ok(entries
        .into_iter()
        .filter(|entry| !entry.draft)
        .filter_map(|entry| {
            let version = Version::parse(entry.tag_name.strip_prefix('v')?).ok()?;
            let stable = !entry.prerelease && version.pre.is_empty();
            (channel == Channel::Beta || stable).then(|| Release {
                version,
                url: entry.html_url,
                assets: entry.assets,
                wire: entry.body.as_deref().and_then(ReleaseWire::parse),
            })
        })
        .max_by(|a, b| a.version.cmp(&b.version)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).expect("valid version")
    }

    fn release(tag: &str, prerelease: bool, draft: bool) -> String {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://github.com/autistic-computer/zebrilus/releases/tag/{tag}","draft":{draft},"prerelease":{prerelease}}}"#
        )
    }

    fn releases(entries: &[String]) -> String {
        format!("[{}]", entries.join(","))
    }

    #[test]
    fn channel_follows_the_running_build() {
        assert_eq!(Channel::of(&version("0.3.1")), Channel::Stable);
        assert_eq!(Channel::of(&version("0.3.2-beta.1")), Channel::Beta);
    }

    #[test]
    fn stable_channel_skips_prereleases_and_drafts() {
        let json = releases(&[
            release("v0.4.0", false, true),
            release("v0.3.2-beta.1", true, false),
            release("v0.3.1", false, false),
            release("v0.3.0", false, false),
        ]);
        let newest = parse_releases(&json, Channel::Stable)
            .expect("parses")
            .expect("finds a release");
        assert_eq!(newest.version, version("0.3.1"));
        assert_eq!(
            newest.url,
            "https://github.com/autistic-computer/zebrilus/releases/tag/v0.3.1"
        );
    }

    #[test]
    fn stable_channel_distrusts_an_unflagged_prerelease_tag() {
        let json = releases(&[
            release("v0.3.2-beta.1", false, false),
            release("v0.3.1", false, false),
        ]);
        let newest = parse_releases(&json, Channel::Stable).unwrap().unwrap();
        assert_eq!(newest.version, version("0.3.1"));
    }

    #[test]
    fn beta_channel_takes_the_newest_of_everything() {
        let json = releases(&[
            release("v0.3.1", false, false),
            release("v0.3.2-beta.1", true, false),
            release("v0.3.2-beta.2", true, false),
        ]);
        let newest = parse_releases(&json, Channel::Beta).unwrap().unwrap();
        assert_eq!(newest.version, version("0.3.2-beta.2"));

        let promoted = releases(&[
            release("v0.3.2", false, false),
            release("v0.3.2-beta.2", true, false),
        ]);
        let newest = parse_releases(&promoted, Channel::Beta).unwrap().unwrap();
        assert_eq!(newest.version, version("0.3.2"));
    }

    #[test]
    fn foreign_tags_and_bad_json_are_handled() {
        let json = releases(&[
            release("nightly", false, false),
            release("v0.3.1", false, false),
        ]);
        let newest = parse_releases(&json, Channel::Stable).unwrap().unwrap();
        assert_eq!(newest.version, version("0.3.1"));
        assert_eq!(parse_releases("[]", Channel::Stable), Ok(None));
        assert!(parse_releases(r#"{"message":"rate limited"}"#, Channel::Stable).is_err());
    }

    #[test]
    fn a_release_names_its_wire_epoch_and_daemon_floor_in_its_notes() {
        let entry = |tag: &str, body: &str| {
            format!(
                r#"{{"tag_name":"{tag}","html_url":"https://github.com/autistic-computer/zebrilus/releases/tag/{tag}","body":"{body}"}}"#
            )
        };
        let notes = r"## What's Changed\r\n* Fix a crash";
        let stated = entry(
            "v0.3.2",
            &format!(r"<!-- zz-wire: epoch=123 min-daemon-revision=4 -->\r\n{notes}"),
        );
        let newest = parse_releases(
            &releases(&[stated, release("v0.3.1", false, false)]),
            Channel::Stable,
        )
        .unwrap()
        .unwrap();
        let wire = ReleaseWire {
            epoch: 123,
            min_daemon_revision: 4,
        };
        assert_eq!(newest.wire, Some(wire));
        assert!(wire.keeps_daemon(123, 4));
        assert!(!wire.keeps_daemon(123, 3));
        assert!(!wire.keeps_daemon(122, 9));

        let newest = parse_releases(&releases(&[entry("v0.3.1", notes)]), Channel::Stable)
            .unwrap()
            .unwrap();
        assert_eq!(newest.version, version("0.3.1"));
        assert_eq!(newest.wire, None);
    }
}
