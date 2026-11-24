---
type: Playbook
title: Toolchain and system prerequisites
description: Toolchain pins, native build dependencies and platform packaging requirements.
resource: .github/workflows/ci.yml
tags: [prerequisites, toolchain, setup, rust, zig, cmake, linux, macos, windows, pacman]
timestamp: 2026-11-24T00:00:00Z
---

Use just 1.52 or newer. From the checkout, `rustup toolchain install` reads
`rust-toolchain.toml` (Rust 1.99.0, minimal, Clippy and rustfmt). With mise active,
`mise install` installs Zig 0.16.0 for the vendored terminal engine. The same Zig pin is
in `mise.toml`, `.zigversion` and `scripts/just/settings.just`. Native desktop builds also
need a C++ compiler, CMake 3.21 or newer (the distribution's `CMakeLists.txt`) and Ninja.

## Linux

For the full Ubuntu 24.04 CI job, including the tmux reference, packaging and software Vulkan tests,
install the dependencies from the **Install Linux dependencies** step in `.github/workflows/ci.yml`:

```sh
sudo apt-get update
sudo apt-get install --yes autoconf automake bison cmake curl desktop-file-utils dpkg-dev \
  libevent-dev libfontconfig-dev libncurses-dev libwayland-dev libx11-xcb-dev libxcb1-dev \
  libxkbcommon-dev libxkbcommon-x11-dev mesa-vulkan-drivers ninja-build pkg-config
```

CEF uses unprivileged user namespaces on Linux. The Debian package installs
`/etc/apparmor.d/zz` for the packaged executable. When Ubuntu restricts unprivileged user
namespaces, an AppImage or dev build needs permission for its executable path. See
`packaging/deb/zz.apparmor` and the `disable-setuid-sandbox` switch in
`libs/ala-cef/src/cef_runtime.rs`.

For Arch packaging install `base-devel` and use `makepkg`. For AppImage packaging,
`scripts/fetch-appimage-tools.sh` fetches the pinned `appimagetool` and type-2 runtime and
checks the SHA-256 for the build host's architecture.

## macOS and Windows

On macOS install Xcode. If `xcrun metal` or `metallib` is missing, install the Metal
Toolchain with `xcodebuild -downloadComponent MetalToolchain`. The shader build invokes
both tools in `libs/ala/crates/ala-platform/build.rs`. Local packaging uses `ditto`,
`codesign` and `hdiutil`. Public signing additionally needs a Developer ID identity and
a notary profile. [Release setup](/playbooks/releasing.md)

On Windows use the native MSVC Rust toolchain and Visual Studio C++ build tools.
The just recipes use Bash under MINGW/MSYS. The release installer also uses Inno Setup.
For the native launch and verification commands, see [building a CEF bundle](/playbooks/build-cef-bundle.md).
