#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
root="$(pwd -W 2>/dev/null || pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"
export CEF_PATH="${CEF_PATH:-$root/.cef-cache}"

case "$(uname -s)" in
    Linux) os=linux ;;
    Darwin) os=macos ;;
    MINGW* | MSYS*) os=windows ;;
    *) echo "unsupported OS: $(uname -s)" >&2; exit 2 ;;
esac

ala=(--config "patch.\"https://github.com/autistic-computer/ala\".ala.path=\"$root/libs/ala/crates/ala\"")
ala_platform=(--config "patch.\"https://github.com/autistic-computer/ala\".ala-platform.path=\"$root/libs/ala/crates/ala-platform\"")

stage_compat() {
    git ls-files --error-unmatch compat/results/summary.md >/dev/null
    compat/run.sh --check-summary
    compat/check.sh
}

stage_fmt() {
    cargo fmt --all -- --check
    cargo fmt --manifest-path libs/penna/storybook/Cargo.toml -- --check
}

stage_lint() {
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo clippy -p zz-daemon -p zz-daemon-client --no-default-features --all-targets -- -D warnings
}

stage_lint-libs() {
    cargo clippy --manifest-path libs/ala/Cargo.toml --workspace --all-targets --all-features -- -D warnings
    cargo metadata --locked --format-version 1 --manifest-path libs/penna/Cargo.toml >/dev/null
    cargo metadata --locked --format-version 1 --manifest-path libs/penna/storybook/Cargo.toml >/dev/null
    cargo metadata --locked --format-version 1 --manifest-path libs/ala-cef/Cargo.toml >/dev/null
    cargo metadata --locked --format-version 1 --manifest-path libs/pele/Cargo.toml >/dev/null
    cargo clippy --manifest-path libs/penna/Cargo.toml --all-targets --all-features "${ala[@]}" -- -D warnings
    cargo clippy --manifest-path libs/penna/Cargo.toml --lib --no-default-features "${ala[@]}" -- -D warnings
    cargo clippy --manifest-path libs/penna/storybook/Cargo.toml --all-targets --all-features "${ala[@]}" "${ala_platform[@]}" -- -D warnings
    rustup target add wasm32-unknown-unknown
    cargo clippy --manifest-path libs/penna/storybook/Cargo.toml --target wasm32-unknown-unknown --lib --features page "${ala[@]}" "${ala_platform[@]}" -- -D warnings
    cargo clippy --manifest-path libs/ala-cef/Cargo.toml --all-targets --all-features "${ala[@]}" -- -D warnings
    cargo clippy --manifest-path libs/pele/Cargo.toml --locked --all-targets --all-features -- -D warnings
    cargo clippy --manifest-path libs/pele/Cargo.toml --locked --lib --target wasm32-unknown-unknown -- -D warnings
}

stage_web() {
    cargo fmt --manifest-path clients/web/Cargo.toml -- --check
    rustup toolchain install nightly --profile minimal --target wasm32-unknown-unknown
    rustup run nightly cargo check --manifest-path clients/web/Cargo.toml --target wasm32-unknown-unknown --all-targets --locked
}

stage_test() {
    if [[ "$os" == windows ]]; then
        local cef
        for cef in "$CEF_PATH"/*/cef_windows_x86_64; do PATH="$cef:$PATH"; done
        cargo ci-test --no-fail-fast
    else
        cargo ci-test
    fi
}

stage_test-libs() {
    cargo test --manifest-path libs/ala/Cargo.toml --workspace --all-features --exclude ala-platform
    cargo test --manifest-path libs/penna/Cargo.toml --all-features "${ala[@]}"
    cargo test --manifest-path libs/penna/Cargo.toml --test highlighter_api "${ala[@]}"
    cargo test --manifest-path libs/penna/storybook/Cargo.toml --all-features "${ala[@]}" "${ala_platform[@]}"
    cargo test --manifest-path libs/ala-cef/Cargo.toml --all-features "${ala[@]}"
    cargo test --manifest-path libs/pele/Cargo.toml --locked --all-features
}

stage_ios() {
    cargo test --manifest-path libs/ala/Cargo.toml -p ala-platform --all-features --test ios
}

stage_lavapipe() {
    local icd
    icd="$(echo /usr/share/vulkan/icd.d/lvp_icd*.json)"
    export VK_ICD_FILENAMES="$icd"
    if [[ -f /usr/share/glvnd/egl_vendor.d/50_mesa.json ]]; then
        export __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json
    fi
    cargo test --manifest-path libs/ala/Cargo.toml -p ala-platform --all-features
}

stage_mux-release() {
    cargo test --release -p zz-mux
}

stage_attached-client() {
    cargo build -p zz-cli
    compat/attached-client.sh "$CARGO_TARGET_DIR/debug/zz_cli" "$(compat/fetch-tmux.sh)"
}

stage_bundle() {
    cargo xtask bundle-cef --release --output "$CARGO_TARGET_DIR/cef-bundle"
}

stage_headless() {
    local tree
    tree="$(cargo tree -p zz-cli -e normal --prefix none)"
    if grep -Eq '^(ala|ala-platform|ala-cef|wgpu|cef|penna|zz-ui) ' <<<"$tree"; then
        echo "zz-cli links GUI crates" >&2
        return 1
    fi
    if grep -Eq '^ureq ' <<<"$tree"; then
        echo "zz-cli links the update checker's TLS stack" >&2
        return 1
    fi
}

stage_packaged-cli() {
    compat/packaged-cli.sh "$CARGO_TARGET_DIR/cef-bundle/zz.app"
}

linux=(compat fmt lint lint-libs web test test-libs lavapipe mux-release attached-client bundle headless)
macos=(test test-libs ios bundle headless packaged-cli)
windows=(ios bundle headless)

if [[ $# -eq 0 ]]; then
    set -- "$os"
fi

stages=()
for arg in "$@"; do
    case "$arg" in
        linux) stages+=("${linux[@]}") ;;
        macos) stages+=("${macos[@]}") ;;
        windows) stages+=("${windows[@]}") ;;
        *)
            declare -F "stage_$arg" >/dev/null || { echo "unknown stage: $arg" >&2; exit 2; }
            stages+=("$arg")
            ;;
    esac
done

if [[ ${#stages[@]} -eq 1 ]]; then
    "stage_${stages[0]}"
    exit
fi

failed=()
for stage in "${stages[@]}"; do
    echo "== $stage"
    start=$SECONDS
    if ("stage_$stage"); then
        echo "== $stage ok $((SECONDS - start))s"
    else
        echo "== $stage FAILED $((SECONDS - start))s"
        failed+=("$stage")
    fi
done

if [[ ${#failed[@]} -gt 0 ]]; then
    echo "== failed: ${failed[*]}"
    exit 1
fi
echo "== all passed"
