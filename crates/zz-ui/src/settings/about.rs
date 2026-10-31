use penna::settings::about::{About, AboutLink};

pub use penna::settings::about::ABOUT_LOGO_SIZE;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY_URL: &str = "https://github.com/autistic-computer/zebrilus";
pub const RELEASES_URL: &str = "https://github.com/autistic-computer/zebrilus/releases";
pub const ISSUES_URL: &str = "https://github.com/autistic-computer/zebrilus/issues/new";

pub const ZZ_ABOUT: About = About {
    name: "zz",
    tagline: "tmux, ghostty and ala walked into a mux.",
    version: VERSION,
    license: "MIT or Apache-2.0",
    links: &[
        AboutLink {
            id: "settings-about-repository",
            title: "Source code",
            label: "GitHub",
            url: REPOSITORY_URL,
        },
        AboutLink {
            id: "settings-about-releases",
            title: "Releases",
            label: "Releases",
            url: RELEASES_URL,
        },
        AboutLink {
            id: "settings-about-issues",
            title: "Report an issue",
            label: "New issue",
            url: ISSUES_URL,
        },
    ],
};

#[must_use]
pub fn build_info(platform: &str) -> String {
    penna::settings::about::build_info(&ZZ_ABOUT, platform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_names_zz_and_its_version() {
        assert_eq!(
            build_info("macos · aarch64"),
            format!("zz {VERSION} (macos · aarch64)")
        );
    }
}
