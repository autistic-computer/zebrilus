#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
icon_name=zz
output="$REPO_ROOT/packaging/mac"
if [[ "${1:-}" == dev && "$#" == 1 ]]; then
    icon_name=zz-dev
    output="$REPO_ROOT/packaging/mac-dev"
elif [[ "$#" != 0 ]]; then
    echo "usage: scripts/compile-macos-icon.sh [dev]" >&2
    exit 2
fi

die() { echo "error: $*" >&2; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || die "compiling the macOS icon requires macOS"

work_dir="$(mktemp -d "${TMPDIR:-/tmp}/zz-icon.XXXXXX")"
cleanup() { rm -rf "$work_dir"; }
trap cleanup EXIT

xcrun actool "$REPO_ROOT/assets/$icon_name.icon" \
    --compile "$work_dir" \
    --platform macosx \
    --minimum-deployment-target 15.0 \
    --app-icon "$icon_name" \
    --output-partial-info-plist "$work_dir/partial.plist" \
    >/dev/null

[[ -s "$work_dir/Assets.car" ]] || die "actool produced no Assets.car"
mkdir -p "$output"
install -m 644 "$work_dir/Assets.car" "$output/Assets.car"
if [[ "$icon_name" == zz-dev ]]; then
    install -m 644 "$work_dir/zz-dev.icns" "$output/zz.icns"
    ictool="$(xcode-select -p)/../Applications/Icon Composer.app/Contents/Executables/ictool"
    for appearance in Default Dark; do
        "$ictool" "$REPO_ROOT/assets/zz-dev.icon" --export-image \
            --output-file "$work_dir/$appearance.png" --platform macOS \
            --rendition "$appearance" --width 512 --height 512 --scale 1 >/dev/null
    done
    swift - "$work_dir" "$REPO_ROOT/assets" <<'SWIFT'
import AppKit

for (appearance, name) in [("Default", "light"), ("Dark", "dark")] {
    let input = URL(fileURLWithPath: CommandLine.arguments[1]).appendingPathComponent("\(appearance).png")
    let output = URL(fileURLWithPath: CommandLine.arguments[2]).appendingPathComponent("zz-dev-\(name)-512.png")
    guard let image = NSImage(contentsOf: input),
          let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 512, pixelsHigh: 512,
                                        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true,
                                        isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
          let context = NSGraphicsContext(bitmapImageRep: bitmap) else { fatalError("Cannot render dev icon") }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = context
    image.draw(in: NSRect(x: 50, y: 50, width: 412, height: 412))
    NSGraphicsContext.restoreGraphicsState()
    guard let data = bitmap.representation(using: .png, properties: [:]) else { fatalError("Cannot encode dev icon") }
    try data.write(to: output)
}
SWIFT
fi
echo "$output updated"
