#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TARGET_DIR="$ROOT/target/storybook" exec "$ROOT/libs/penna/storybook/build.sh" \
    --manifest "$ROOT/clients/storybook/Cargo.toml" --out "$ROOT/clients/storybook/dist" "$@" -- --locked
