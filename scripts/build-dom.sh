#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/clients/dom/dist"
PROFILE=debug
ARGS=()
THEME_IDENTITY=1
if [[ "${1:-}" == "--release" ]]; then
    PROFILE=release
    ARGS=(--release)
    THEME_IDENTITY=0
fi

cd "$ROOT"
cargo run -q -p zz-ui --example theme_css design/tokens >/dev/null
cargo build ${ARGS[@]+"${ARGS[@]}"} \
    --manifest-path clients/dom/Cargo.toml \
    --target wasm32-unknown-unknown \
    --target-dir target/dom
ZZ_DEV_BUILD="$THEME_IDENTITY" cargo build --release \
    --manifest-path clients/dom/theme/Cargo.toml \
    --target wasm32-unknown-unknown \
    --target-dir target/dom-theme
rm -rf "$OUT"
mkdir -p "$OUT/kit" "$OUT/fonts" "$OUT/img"
wasm-bindgen "target/dom/wasm32-unknown-unknown/$PROFILE/zz_dom.wasm" \
    --target web --no-typescript --out-dir "$OUT/pkg"
wasm-bindgen target/dom-theme/wasm32-unknown-unknown/release/zz_dom_theme.wasm \
    --target web --no-typescript --out-dir "$OUT/theme"
cp -R clients/dom/web/. "$OUT/"
cp libs/penna/web/penna.css libs/penna/web/kit.js design/zz.css "$OUT/kit/"
cp -R design/tokens "$OUT/kit/tokens"
cp clients/web/assets/fonts/lilex/*.ttf clients/web/assets/fonts/inter/*.ttf "$OUT/fonts/"
cp assets/zz-light-512.png assets/zz-dark-512.png assets/zz-dev-light-512.png assets/zz-dev-dark-512.png "$OUT/img/"
