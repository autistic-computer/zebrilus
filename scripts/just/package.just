import "settings.just"

set working-directory := "../.."

[doc("Build and validate the macOS release bundle, then emit dist/zz-macos.dmg.")]
mac:
    @if [[ "$(uname -s)" != "Darwin" ]]; then echo "just package mac requires macOS" >&2; exit 2; fi
    @just build mac
    @scripts/package-dmg.sh dist/zz/zz.app dist/zz-macos.dmg

[doc("Build and validate the Windows release bundle, then emit dist/zz-windows.zip.")]
windows:
    @if [[ "$(uname -s)" != MINGW* && "$(uname -s)" != MSYS* ]]; then echo "just package windows requires Windows" >&2; exit 2; fi
    @just build windows
    @powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/package-windows.ps1 dist/zz dist/zz-windows.zip

[doc("Build and validate the native Arch Linux package in packaging/arch.")]
arch *args:
    @if [[ "$(uname -s)" != "Linux" ]] || ! command -v makepkg >/dev/null 2>&1; then echo "just package arch requires an Arch Linux system with makepkg" >&2; exit 2; fi
    @just build linux -- {{ args }}
    @cd packaging/arch && makepkg --force --cleanbuild --clean

[doc("Build and validate the Debian/Ubuntu package, then emit dist/zz-linux.deb.")]
deb *args:
    @if [[ "$(uname -s)" != "Linux" ]] || ! command -v dpkg-deb >/dev/null 2>&1; then echo "just package deb requires a Debian/Ubuntu system with dpkg-dev" >&2; exit 2; fi
    @just build linux -- {{ args }}
    @scripts/package-deb.sh dist/zz dist/zz-linux.deb
