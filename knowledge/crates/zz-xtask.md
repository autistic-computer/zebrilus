---
type: Rust Crate
title: zz-xtask crate
description: Build and validation commands for desktop CEF bundles on Linux, macOS and Windows.
resource: crates/zz-xtask/src/main.rs
tags: [xtask, cef, build-tooling, cli, bundling, profiling, dsym]
timestamp: 2026-11-24T00:00:00-03:00
---

# Schema

`cargo xtask` runs `crates/zz-xtask/src/main.rs`. It has two subcommands. The default bundle
output is `dist/zz`. The Cargo profile defaults to development. Named profiles are supported
on macOS.

| Command | Result |
| --- | --- |
| `bundle-cef [--release \| --profile NAME] [--output DIR] [--features LIST]` | Builds the current platform's GUI bundle, installs CEF notices, applies platform settings, then validates the result |
| `verify-cef-bundle PATH` | Validates a built bundle. Supply the app directory on macOS or the executable on Linux/Windows. |

| Target | What `platform_bundle_files` and the platform checks expect |
| --- | --- |
| Linux | GUI executable and `cli`, CEF library, ICU data, resources, en-US locale, sandbox, credits and license. A release bundle strips `libcef.so`. |
| macOS | Main app and CLI, helper apps, Chromium framework, resources, icons and notices. The build configures app metadata, signs nested code inside out, then verifies the bundle. A profiling build copies and verifies dSYM UUIDs. |
| Windows | Bootstrap `zz.exe` with the app icon and PerMonitorV2 manifest, `zz.dll`, CEF runtime DLLs, snapshot/resources, locale and notices. |

## Build and verify

```sh
cargo xtask bundle-cef --release --output dist/zz
```

On macOS, validate the bundle or build the symbolized variant for Instruments.

```sh
cargo xtask verify-cef-bundle dist/zz/zz.app
open dist/zz/zz.app
cargo xtask bundle-cef --profile profiling --output dist/zz-profile
```

On Linux, use `cargo xtask verify-cef-bundle dist/zz/zz`. On Windows, use
`cargo xtask verify-cef-bundle dist/zz/zz.exe`. `verify_bundle` rejects missing, empty or
non-file resources. `MACOS_LOCAL_SIGN_IDENTITY` overrides local signing identity selection.
Release signing and notarization use the separate release recipes.

See [bundle playbook](/playbooks/build-cef-bundle.md), [CEF artifact pins](/references/cef-artifacts.md),
[updating CEF](/playbooks/updating-cef.md), [prerequisites](/playbooks/prerequisites.md) and
[running and profiling](/playbooks/running-zz.md).
