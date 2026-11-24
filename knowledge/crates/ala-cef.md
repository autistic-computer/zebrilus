---
type: Rust Crate
title: ala-cef crate
description: CEF off-screen browser hosting, cross-platform input and frame types, profiles, and optional Chrome data import.
resource: libs/ala-cef/src/lib.rs
tags: [browser, cef, crate, osr]
timestamp: 2026-11-24T00:00:00-03:00
---

# Features and ownership

`libs/ala-cef` is a standalone Cargo workspace. The desktop [zz client](/crates/zz.md)
uses its Chromium runtime and consumes browser events and frames. Thin clients can reuse the
browser-neutral types with default features disabled.

| Build in `libs/ala-cef/Cargo.toml` | What it provides |
| --- | --- |
| Default features disabled | Browser events, input, lifecycle, profile names and paths, cookie parsing and page-zoom steps. Element-picker state is available on non-Wasm targets. |
| `host` | Host frame types, GPU context, artifacts and the latest-frame mailbox |
| `cef-runtime` (default) | Enables `host`, links CEF and exposes `bootstrap`, `BrowserRuntime`, `BrowserSession` and the message pump |
| `chrome-import` | Independent opt-in for Chrome profile discovery, cookie import and history reading |

`libs/ala-cef/src/lib.rs` gates public exports. macOS host builds expose IOSurface frames and a
presenter. Windows host builds expose shared GPU texture types, and Windows runtime builds expose
`bootstrap_windows`. Linux runtime builds include DMA-BUF import. See
[off-screen rendering](/browser/osr-rendering.md) and [CEF runtime](/browser/cef-runtime.md).

## Source map

- `src/cef_runtime.rs` owns runtime and session operations. `src/frame.rs` defines
  `OsrFrame` and `FrameMailbox` for accelerated or CPU-backed frames.
  [Runtime lifecycle](/browser/lifecycle.md).
- `src/input.rs`, `src/event.rs` and `src/ime.rs` define the input and event contract.
  [Input translation](/browser/input-translation.md).
- `src/profile.rs` owns profile normalization and `BrowserProfilePaths`. The caller selects
  the data directory. `src/cookie.rs` parses imported cookie formats, and
  `src/chrome_import/` implements the optional Chrome import. [Profiles](/browser/profile.md).
- `src/element_picker.rs` and `picker/` hold picker state and the injected script's source.
  [Element picker](/browser/element-picker.md).

Run `just libs check ala-cef` from the zz repository for the library's own checks. The
[CEF artifact reference](/references/cef-artifacts.md) and [bundle playbook](/playbooks/build-cef-bundle.md)
cover dependency pins and packaging.
