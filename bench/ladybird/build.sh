#!/usr/bin/env bash
set -euo pipefail

target_root="${CARGO_TARGET_DIR:-$PWD/target}"
bundle="$target_root/ladybird-app"
export CARGO_TARGET_DIR="$target_root/ladybird-build"
bench/ladybird/runtime.sh "$bundle/runtime"
export ZZ_DEV_BUILD=1
cargo build --locked --release -p zz -p zz-cli --bin zz --bin zz_cli \
    --no-default-features --features zz/ladybird,zz/agent-pane
for name in zz cli; do
    source="$CARGO_TARGET_DIR/release/zz"
    [[ "$name" == zz ]] || source="$CARGO_TARGET_DIR/release/zz_cli"
    cp --reflink=auto "$source" "$bundle/$name.$$"
    mv -f "$bundle/$name.$$" "$bundle/$name"
done
