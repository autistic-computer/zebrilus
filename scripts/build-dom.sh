#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/clients/dom/dist"
PROFILE=debug
ARGS=()
THEME_IDENTITY=1
BINDGEN_ARGS=()
if [[ "${1:-}" == "--release" ]]; then
    PROFILE=release
    ARGS=(--release)
    THEME_IDENTITY=0
elif [[ "${1:-}" == "--showcase" ]]; then
    PROFILE=showcase
    ARGS=(--profile showcase --features showcase)
    THEME_IDENTITY=0
    BINDGEN_ARGS=(--remove-name-section --remove-producers-section)
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
mkdir -p "$OUT/kit" "$OUT/fonts" "$OUT/img/icons" "$OUT/img/programs" "$OUT/sounds"
wasm-bindgen "target/dom/wasm32-unknown-unknown/$PROFILE/zz_dom.wasm" \
    --target web --no-typescript ${BINDGEN_ARGS[@]+"${BINDGEN_ARGS[@]}"} --out-dir "$OUT/pkg"
wasm-bindgen target/dom-theme/wasm32-unknown-unknown/release/zz_dom_theme.wasm \
    --target web --no-typescript ${BINDGEN_ARGS[@]+"${BINDGEN_ARGS[@]}"} --out-dir "$OUT/theme"
cp -R clients/dom/web/. "$OUT/"
cp libs/penna/web/penna.css libs/penna/web/kit.js libs/penna/src/components/slider/slider.css libs/penna/src/components/segmented/segmented.css libs/penna/src/components/segmented/segmented.js design/zz.css "$OUT/kit/"
cp -R design/tokens "$OUT/kit/tokens"
cp clients/web/assets/fonts/lilex/*.ttf clients/web/assets/fonts/inter/*.ttf "$OUT/fonts/"
cp assets/zz-light-512.png assets/zz-dark-512.png assets/zz-dev-light-512.png assets/zz-dev-dark-512.png "$OUT/img/"
cp crates/zz-ui/assets/icons/*.svg "$OUT/img/icons/"
for svgz in crates/zz-ui/assets/programs/*.svgz; do
    base="$(basename "$svgz" .svgz)"
    gzip -dc "$svgz" > "$OUT/img/programs/$base.svg"
done
cp crates/zz-client/assets/sounds/*.wav "$OUT/sounds/"
