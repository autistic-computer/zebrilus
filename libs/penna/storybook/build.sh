#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$HERE/Cargo.toml"
TARGET="${CARGO_TARGET_DIR:-$HERE/target}"
DIST="$HERE/dist"
TOOLCHAIN="${WEB_TOOLCHAIN:-nightly}"
PROFILE=debug
BUILD_ARGS=(--profile dev)
CARGO_ARGS=()

while [[ "$#" -gt 0 ]]; do
    case "$1" in
        --release)
            PROFILE=release
            BUILD_ARGS=(--release)
            shift
            ;;
        --out)
            DIST="$(mkdir -p "$2" && cd "$2" && pwd)"
            shift 2
            ;;
        --)
            shift
            CARGO_ARGS=("$@")
            break
            ;;
        *)
            echo "usage: storybook/build.sh [--release] [--out <dir>] [-- <cargo args>]" >&2
            exit 2
            ;;
    esac
done

if ! rustup run "$TOOLCHAIN" rustc --version >/dev/null 2>&1; then
    echo "missing Rust $TOOLCHAIN with the wasm32-unknown-unknown target" >&2
    exit 2
fi

if ! command -v wasm-bindgen >/dev/null 2>&1 || [[ "$(wasm-bindgen --version)" != "wasm-bindgen 0.2.128" ]]; then
    echo "wasm-bindgen 0.2.128 is required: cargo install wasm-bindgen-cli --version 0.2.128 --locked" >&2
    exit 2
fi

SYNTAX_ARGS=()
if [[ "${WEB_SYNTAX_HIGHLIGHTING:-1}" == 1 ]]; then
    AR="$(command -v llvm-ar || true)"
    if [[ -z "$AR" ]] && command -v brew >/dev/null 2>&1; then
        AR="$(brew --prefix llvm 2>/dev/null)/bin/llvm-ar"
    fi
    if [[ ! -x "$AR" ]]; then
        echo "syntax highlighting needs llvm-ar (brew install llvm); set WEB_SYNTAX_HIGHLIGHTING=0 to skip it" >&2
        exit 2
    fi
    TS_HEADERS="$(cargo metadata --format-version 1 --manifest-path "$MANIFEST" --features syntax-highlighting \
        ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"} \
        | python3 -c 'import json, os, sys; print(next(os.path.join(os.path.dirname(p["manifest_path"]), "wasm", "include") for p in json.load(sys.stdin)["packages"] if p["name"] == "tree-sitter-language"))')"
    TS_STUBS="$TARGET/tree-sitter-wasm-stubs"
    mkdir -p "$TS_STUBS"
    : > "$TS_STUBS/stdio.c"
    : > "$TS_STUBS/stdlib.c"
    : > "$TS_STUBS/string.c"
    export AR_wasm32_unknown_unknown="$AR"
    export CFLAGS_wasm32_unknown_unknown="-isystem $TS_HEADERS -Disdigit(c)=((unsigned)(c)-48u<10u)"
    SYNTAX_ARGS=(
        --features syntax-highlighting
        --config "target.wasm32-unknown-unknown.tree-sitter-language.wasm-headers=\"$TS_HEADERS\""
        --config "target.wasm32-unknown-unknown.tree-sitter-language.wasm-src=\"$TS_STUBS\""
    )
fi

rustup run "$TOOLCHAIN" cargo build --lib --features page "${BUILD_ARGS[@]}" \
    ${SYNTAX_ARGS[@]+"${SYNTAX_ARGS[@]}"} ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"} \
    --manifest-path "$MANIFEST" \
    --target-dir "$TARGET" \
    --target wasm32-unknown-unknown

mkdir -p "$DIST/wasm"
wasm-bindgen "$TARGET/wasm32-unknown-unknown/$PROFILE/penna_storybook.wasm" \
    --out-dir "$DIST/wasm" \
    --out-name storybook \
    --target web \
    --no-typescript
cp "$HERE/web/index.html" "$HERE/web/main.js" "$HERE/web/style.css" "$DIST/"
echo "built $DIST"
