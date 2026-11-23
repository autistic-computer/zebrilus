#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -sm)" != "Linux x86_64" ]]; then
    echo "The Ladybird spike uses the Linux x86_64 alpha package." >&2
    exit 1
fi
destination="$1"
flavor="${2:-embedded}"
case "$flavor" in
    embedded) exclude=(--exclude='./opt/ladybird/Qt6' --exclude='./opt/ladybird/bin' --exclude='./opt/ladybird/lib/libQt6*') ;;
    standalone) exclude=() ;;
    *) echo "Expected embedded or standalone" >&2; exit 2 ;;
esac
if [[ -x "$destination/libexec/Hub" ]]; then
    if [[ "$flavor" == standalone && ! -x "$destination/bin/Ladybird" ]]; then
        echo "A full standalone package is required at $destination" >&2
        exit 1
    fi
    [[ "$("$destination/libexec/Hub" --version)" == "Version 2026.47.7" ]]
    exit
fi
mkdir -p "$(dirname "$destination")"
stage="$(mktemp -d "$(dirname "$destination")/.ladybird-XXXXXXXX")"
trap 'rm -rf "$stage"' EXIT
package="ladybird_2026.47.7-1~ubuntu24.04_amd64.deb"
curl -fLsS --retry 3 "https://apt.ladybird.org/pool/main/l/ladybird/$package" -o "$stage/$package"
(cd "$stage" && echo "67e33f0a234b672734b8965ea7dd5c63c8f0b616313b91d43522c53ad686b1d4  $package" | sha256sum -c -)
ar p "$stage/$package" data.tar.xz | tar -xJ -C "$stage" "${exclude[@]}" ./opt/ladybird
"$stage/opt/ladybird/libexec/Hub" --version
mv -T "$stage/opt/ladybird" "$destination"
