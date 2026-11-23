import "settings.just"

set working-directory := "../.."

[doc("Build the experimental Linux x86_64 desktop and stage its pinned engine.")]
build:
    @bench/ladybird/build.sh

[doc("Open a Ladybird pane in the spike's private development session.")]
run url="https://ladybird.org":
    @just ladybird build
    @bench/ladybird/run.sh {{ quote(url) }}

[doc("Compare the standalone engine, stock browser and zz on a private Wayland display.")]
baseline output: build
    #!/usr/bin/env bash
    set -euo pipefail
    target_root="${CARGO_TARGET_DIR:-$PWD/target}"
    bench/ladybird/runtime.sh "$target_root/ladybird-stock" standalone
    python3 bench/ladybird/baseline.py --bundle "$target_root/ladybird-app" \
        --stock "$target_root/ladybird-stock" --output {{ quote(output) }}
    python3 bench/ladybird/baseline_report.py {{ quote(output) }} \
        --archive {{ quote(output) }}/samples.json.gz --output {{ quote(output) }}/report.md
