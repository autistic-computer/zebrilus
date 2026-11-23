import "settings.just"

set working-directory := "../.."

[doc("Build the experimental Linux x86_64 desktop and stage its pinned engine.")]
build:
    @bench/ladybird/build.sh

[doc("Open a Ladybird pane in the spike's private development session.")]
run url="https://ladybird.org":
    @just ladybird build
    @bench/ladybird/run.sh {{ quote(url) }}
