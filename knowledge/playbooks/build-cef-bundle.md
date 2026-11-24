---
type: Playbook
title: Building and verifying a platform CEF bundle
description: Assemble, verify and package the native desktop bundle on macOS, Linux or Windows.
resource: crates/zz-xtask/src/main.rs
tags: [cef, xtask, bundle, playbook, sha1, codesign, pacman, deb, apparmor, profiling, dsym]
timestamp: 2026-11-24T00:00:00Z
---

## Build and verify

Run the row for the build host from the checkout. Each build assembles a release bundle under
`dist/zz` and calls `verify_bundle`. The verification commands also work on an existing bundle.

| Host | Build | Verify | Launch GUI |
| --- | --- | --- | --- |
| macOS | `just build mac` | `cargo xtask verify-cef-bundle dist/zz/zz.app` | `open dist/zz/zz.app` |
| Linux | `just build linux` | `cargo xtask verify-cef-bundle dist/zz/zz` | `dist/zz/zz app` |
| Windows (Git Bash) | `just build windows` | `cargo xtask verify-cef-bundle dist/zz/zz.exe` | `./dist/zz/zz.exe` |

For a custom destination use `cargo xtask bundle-cef --release --output DIR`. Omit
`--release` for debug. The bundler accepts `--features LIST` and named profiles on macOS.
`just profile build mac` writes the optimized app to `dist/zz-profile` and verifies that
the copied GUI and helper dSYMs match their executable UUIDs. [Profiling](/playbooks/running-zz.md#profiling-the-macos-bundle)

The first native build resolves the matching CEF distribution, verifies its archive against
the published index and builds the C++ wrapper. Set `CEF_PATH` to a stable cache directory to
reuse the download across worktrees. [Prerequisites](/playbooks/prerequisites.md) · [CEF pin](/references/cef-artifacts.md)

## Platform checks

`crates/zz-xtask/src/main.rs` defines the required files in `platform_bundle_files`.
Every required file must exist and be nonempty. Every bundle includes CEF's license and credits.

- **macOS.** The app contains the CEF framework, helper apps, `Contents/MacOS/zz` and
  `Contents/MacOS/cli`. Point PATH links at `cli` so a shell launch can find the bundle.
  Local builds sign inside out with the sole valid Apple Development identity, falling back
  to ad hoc signing when there is no unique match. `MACOS_LOCAL_SIGN_IDENTITY` selects a name
  or fingerprint, and `-` forces ad hoc. Verification checks the outer app's quarantine policy,
  main and helper bundle versions, resources and strict recursive signature.
- **Linux.** Keep `zz`, `cli`, libraries and resources together. The bundler unlinks the old
  executable before installing its replacement so a running daemon can keep its mapped inode.
  Release bundles strip `libcef.so`; debug builds retain symbols. The packaged PATH launcher
  resolves to `/usr/lib/zz/cli`.
- **Windows.** The flat bundle starts at CEF's `bootstrap.exe` copied as `zz.exe`, with
  `zz.dll`, libraries and resources beside it. `verify_windows_executable` checks the icon
  and per-monitor DPI manifest.

After editing `assets/zz.icon`, run `scripts/compile-macos-icon.sh` with Xcode 26+.
Pass `dev` for `assets/zz-dev.icon`. The build consumes the checked-in catalogs and fallback icons.

## Package

| Output | Command | What it checks or installs |
| --- | --- | --- |
| `dist/zz-macos.dmg` | `just package mac` | Creates and mounts the disk image, verifies the contained app. For public distribution use the Developer ID and notarization recipes. |
| `dist/zz-windows.zip` | `just package windows` | Packages the flat bundle. The Release workflow also builds an installer. |
| Arch package | `just package arch` or `just install arch` | Uses `makepkg` in `packaging/arch`, installs via pacman with the install recipe. |
| `dist/zz-linux.deb` | `just package deb` or `just install deb` | Computes ELF dependencies with `dpkg-shlibdeps`, adds the AppArmor profile, installs via apt with the install recipe. |
| Linux AppImage | Run `scripts/fetch-appimage-tools.sh`, apply its two printed exports, then run `scripts/package-appimage.sh dist/zz dist/zz-linux.AppImage`. | Uses verified tooling and validates the extracted AppDir. |
| Headless tarball | `just headless mac`, `linux` or `windows` on that host | Packages the standalone CLI for supported release architectures. |

Use `compat/packaged-cli.sh dist/zz/zz.app` to verify shell launches from a temporary path
containing spaces. The checks use isolated daemons. CI's `scripts/ci.sh bundle` builds into
`$CARGO_TARGET_DIR/cef-bundle`. Pushes and pull requests exercise Linux and macOS. Scheduled
and manual CI runs add Windows. Release signing, channel selection and publication are in
[releasing zz](/playbooks/releasing.md).
