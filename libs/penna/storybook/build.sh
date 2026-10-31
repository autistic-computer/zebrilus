#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$HERE/Cargo.toml"
DIST=""
PROFILE=debug
BUILD_ARGS=(--profile dev)
CARGO_ARGS=()

while [[ "$#" -gt 0 ]]; do
    case "$1" in
        --manifest)
            MANIFEST="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
            shift 2
            ;;
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
            echo "usage: storybook/build.sh [--manifest <Cargo.toml>] [--release] [--out <dir>] [-- <cargo args>]" >&2
            exit 2
            ;;
    esac
done
DIST="${DIST:-$(dirname "$MANIFEST")/dist}"
cd "$(dirname "$MANIFEST")"

CARGO=(cargo)
if [[ -n "${WEB_TOOLCHAIN:-}" ]]; then
    CARGO=(rustup run "$WEB_TOOLCHAIN" cargo)
fi

METADATA="$("${CARGO[@]}" metadata --format-version 1 --all-features --manifest-path "$MANIFEST" \
    ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"})"
PACKAGE="$(printf '%s' "$METADATA" | python3 -c '
import json, os, shlex, sys
meta = json.load(sys.stdin)
manifest = os.path.realpath(sys.argv[1])
package = next(p for p in meta["packages"] if os.path.realpath(p["manifest_path"]) == manifest)
lib = next(t["name"] for t in package["targets"] if {"lib", "rlib", "cdylib"} & set(t["kind"]))
def version(name):
    return next((p["version"] for p in meta["packages"] if p["name"] == name), "")
def folder(name):
    return next((os.path.dirname(p["manifest_path"]) for p in meta["packages"] if p["name"] == name), "")
for key, value in {
    "TARGET": meta["target_directory"],
    "LIB": lib.replace("-", "_"),
    "WASM_BINDGEN": version("wasm-bindgen"),
    "PAGE_FEATURE": "page" if "page" in package["features"] else "",
    "SYNTAX_FEATURE": "syntax-highlighting" if "syntax-highlighting" in package["features"] else "",
    "TREE_SITTER_LANGUAGE": folder("tree-sitter-language"),
}.items():
    print(f"{key}={shlex.quote(value)}")
' "$MANIFEST")"
eval "$PACKAGE"

if [[ -z "$WASM_BINDGEN" ]]; then
    echo "$MANIFEST does not depend on wasm-bindgen" >&2
    exit 2
fi
if ! command -v wasm-bindgen >/dev/null 2>&1 || [[ "$(wasm-bindgen --version)" != "wasm-bindgen $WASM_BINDGEN" ]]; then
    echo "wasm-bindgen $WASM_BINDGEN is required: cargo install wasm-bindgen-cli --version $WASM_BINDGEN --locked" >&2
    exit 2
fi

FEATURE_ARGS=()
if [[ -n "$PAGE_FEATURE" ]]; then
    FEATURE_ARGS+=(--features "$PAGE_FEATURE")
fi
if [[ "${WEB_SYNTAX_HIGHLIGHTING:-1}" == 1 && -n "$SYNTAX_FEATURE" ]]; then
    AR="$(command -v llvm-ar || true)"
    if [[ -z "$AR" ]] && command -v brew >/dev/null 2>&1; then
        AR="$(brew --prefix llvm 2>/dev/null)/bin/llvm-ar"
    fi
    if [[ ! -x "$AR" ]]; then
        echo "syntax highlighting needs llvm-ar (brew install llvm); set WEB_SYNTAX_HIGHLIGHTING=0 to skip it" >&2
        exit 2
    fi
    TS_HEADERS="$TREE_SITTER_LANGUAGE/wasm/include"
    TS_STUBS="$TARGET/tree-sitter-wasm-stubs"
    mkdir -p "$TS_STUBS"
    : > "$TS_STUBS/stdio.c"
    : > "$TS_STUBS/stdlib.c"
    : > "$TS_STUBS/string.c"
    export AR_wasm32_unknown_unknown="$AR"
    export CFLAGS_wasm32_unknown_unknown="-isystem $TS_HEADERS -Disdigit(c)=((unsigned)(c)-48u<10u)"
    FEATURE_ARGS+=(
        --features "$SYNTAX_FEATURE"
        --config "target.wasm32-unknown-unknown.tree-sitter-language.wasm-headers=\"$TS_HEADERS\""
        --config "target.wasm32-unknown-unknown.tree-sitter-language.wasm-src=\"$TS_STUBS\""
    )
fi

"${CARGO[@]}" rustc --lib --crate-type cdylib "${BUILD_ARGS[@]}" \
    ${FEATURE_ARGS[@]+"${FEATURE_ARGS[@]}"} ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"} \
    --manifest-path "$MANIFEST" \
    --target wasm32-unknown-unknown

mkdir -p "$DIST/wasm"
wasm-bindgen "$TARGET/wasm32-unknown-unknown/$PROFILE/$LIB.wasm" \
    --out-dir "$DIST/wasm" \
    --out-name storybook \
    --target web \
    --no-typescript
cp "$HERE/web/index.html" "$HERE/web/main.js" "$HERE/web/style.css" "$DIST/"
echo "built $DIST"
