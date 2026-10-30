import "settings.just"

set working-directory := "../.."

[doc("Build the storybook, serve it on 127.0.0.1:8097 and rebuild on save.")]
run:
    @scripts/storybook-dev.sh

[doc("Build the storybook into clients/storybook/dist; pass --release for an optimized build.")]
build *args:
    @scripts/build-storybook.sh {{ args }}

[doc("Lint the storybook under the root clippy rules, for the host and for wasm.")]
lint:
    cargo clippy --manifest-path clients/storybook/Cargo.toml --all-targets -- -D warnings
    cargo clippy --manifest-path clients/storybook/Cargo.toml --target wasm32-unknown-unknown --lib -- -D warnings

[doc("Serve the last build without rebuilding.")]
serve:
    @scripts/storybook-serve.py clients/storybook/dist 8097

[doc("Build penna's own storybook (the theme creator with penna's five plates, then foundation and kit, penna roles only) against libs/ala into libs/penna/storybook/dist; pass --release for an optimized build.")]
penna-build *args:
    #!/usr/bin/env bash
    set -euo pipefail
    CARGO_TARGET_DIR="$PWD/target/storybook" libs/penna/storybook/build.sh {{ args }} -- \
        --config "patch.\"https://github.com/autistic-computer/ala\".ala.path=\"$PWD/libs/ala/crates/ala\"" \
        --config "patch.\"https://github.com/autistic-computer/ala\".ala-platform.path=\"$PWD/libs/ala/crates/ala-platform\""

[doc("Build penna's own storybook and serve it on 127.0.0.1:8098 (PORT overrides).")]
penna *args: (penna-build args)
    @echo "penna storybook at http://127.0.0.1:${PORT:-8098}/"
    @scripts/storybook-serve.py libs/penna/storybook/dist "${PORT:-8098}"

[doc("Screenshot one story or section of a running storybook: just storybook shot agent/composer?theme=dark out.png")]
shot route out *args:
    @scripts/storybook-shot.mjs "{{ route }}" "{{ out }}" {{ args }}

[doc("Compare the macOS theme (penna's, with zz's role patches) with native AppKit glass on the same backdrop (macOS 27 SDK).")]
macos:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target/macos-glass-reference
    xcrun swiftc -parse-as-library \
      "$PWD/clients/storybook/examples/macos_reference.swift" \
      -framework AppKit -o target/macos-glass-reference/native
    target/macos-glass-reference/native &
    reference_pid=$!
    trap 'kill "$reference_pid" 2>/dev/null || true' EXIT
    cargo run --manifest-path clients/storybook/Cargo.toml --example macos
