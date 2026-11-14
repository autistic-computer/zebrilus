import "settings.just"

set working-directory := "../.."

mod mac "release-mac.just"

[doc("Install the pinned dry-run-first release driver.")]
setup:
    cargo install cargo-release --version 1.1.5 --locked

[doc("Preview a SemVer bump, release commit, tag, and push; --execute applies and publishes it (the pushed tag starts release CI).")]
bump target *flags:
    @scripts/release.sh "{{ target }}" {{ flags }}

[doc("Install the release runners on this Mac and win-desktop and build the Ubuntu runner images here and on alienware.")]
runners:
    @scripts/release-runners.sh setup

[doc("Build and package every release leg on the release runners, with no tag, upload or publish.")]
dry-run:
    @scripts/release-runners.sh dry-run

[doc("Preflight, start single-use release runners, bump and tag, and watch the release until it is published.")]
ship target:
    @scripts/release-runners.sh ship "{{ target }}"

[doc("Stop the release runners and remove any still registered.")]
stop:
    @scripts/release-runners.sh stop
