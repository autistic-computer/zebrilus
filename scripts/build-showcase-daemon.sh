#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="aarch64-unknown-linux-musl"
OUT="${1:-}"

cd "$ROOT"

if ! rustup target list --installed | grep -qx "$TARGET"; then
    echo "missing target $TARGET; run: rustup target add $TARGET" >&2
    exit 2
fi

SYSROOT="$(rustc --print sysroot)"
HOST="$(rustc -vV | awk '/^host:/ { print $2 }')"
LLD="$SYSROOT/lib/rustlib/$HOST/bin/rust-lld"

if [[ ! -x "$LLD" ]]; then
    echo "missing rust-lld at $LLD" >&2
    exit 2
fi

SHIM="$(mktemp "${TMPDIR:-/tmp}/zz-musl-lld.XXXXXX")"
trap 'rm -f "$SHIM"' EXIT

cat > "$SHIM" <<EOF
#!/usr/bin/env python3
import os, sys
os.environ["DYLD_LIBRARY_PATH"] = "$SYSROOT/lib"
out = ["$LLD", "-flavor", "gnu", "-m", "aarch64linux"]
for arg in sys.argv[1:]:
    if arg in ("-nostartfiles", "-nodefaultlibs", "-no-pie"):
        continue
    if arg == "-static":
        out.append("-Bstatic")
        continue
    if arg.startswith("-Wl,"):
        out.extend(arg[4:].split(","))
    else:
        out.append(arg)
os.execv("$LLD", out)
EOF
chmod +x "$SHIM"

cd "$ROOT"
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="$SHIM" \
    RUSTFLAGS="-C target-feature=+crt-static" \
    cargo build --profile showcase --target "$TARGET" -p zz-showcase-daemon

BIN="$ROOT/target/$TARGET/showcase/zz-showcase-daemon"
if [[ -n "$OUT" ]]; then
    mkdir -p "$(dirname "$OUT")"
    cp "$BIN" "$OUT"
    echo "showcase daemon ready: $OUT"
else
    echo "showcase daemon ready: $BIN"
fi
