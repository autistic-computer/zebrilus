---
type: Rust Crate
title: zz-browser crate
description: Browser-neutral abstraction over CEF Alloy off-screen rendering. Owns CEF init, named private request contexts, page zoom, input, lifecycle, and frame mailboxes.
resource: crates/zz-browser/src/lib.rs
tags: [browser, cef, crate, osr]
timestamp: 2026-08-14T00:00:00Z
---

# Overview

`crates/zz-browser` (package name `zz-browser`) is the single crate that touches
CEF. It wraps CEF Alloy off-screen rendering (OSR) behind a small, CEF-free public
API so the rest of zz never sees a raw Chromium handle. It owns global CEF
initialization, single-binary multi-process subprocess dispatch, named private
persistent request contexts ("zz profiles"), GPUI-to-CEF input translation,
runtime/session lifecycle state machines, and one **latest-frame mailbox** per
session. The ala client in [`crates/zz`](/crates/zz.md) consumes only the
browser-neutral types re-exported from `lib.rs`, never CEF binding types.

Frame transport is a replaceable boundary, and it has already been replaced twice:
`OsrFrame` is an enum over `OwnedBgra` (the universal readback tier), `Gpu` (a
zz-owned wgpu texture on Linux/FreeBSD), and `MacGpu` (a retained `IOSurface` on
macOS). Adding or losing a tier changes nothing about navigation, profile,
lifecycle, or input. See [OSR rendering](/browser/osr-rendering.md) for the three
pipelines and how a session falls back to readback.

# Architecture

The crate depends on no zz crate, so it can leave the workspace as `ala-cef`.
It splits into three layers by Cargo feature:

- **No features**: `cookie`, `element_picker`, `event`, `input`, `lifecycle`,
  `page_zoom`, `profile`. Pure types, validation and state
  machines on `url`, `serde` and `parking_lot`; this is what the thin
  clients' browser panes (`clients/app`) and `zz-chrome-import` use, and it
  builds for WASM, iOS and Android. The root workspace dependency turns
  default features off. CI keeps it out of `zz-cli`'s tree, so `zz-config`
  takes `SearchProvider` from `zz-client`'s `url_input`, not from here.
- **`host`**: adds `frame`, `readback_texture` and `artifact`, the wgpu,
  Metal and Direct3D frame types and the local artifact pages.
- **`cef-runtime`** (the crate's default, enabled by `crates/zz`): adds
  `cef_runtime`, the only module that links `cef`. It hosts
  `BrowserRuntime`, `BrowserSession`, `bootstrap`, `run_subprocess`, and every
  CEF handler (`cef::wrap_*!`) callback. Its module attribute locally allows
  `unsafe_code` solely to validate and copy CEF's callback-scoped BGRA paint
  buffer into owned memory; no borrowed CEF pointer survives that callback.

`cargo test -p zz-browser --no-default-features --features host` runs the
CEF-free tests without downloading Chromium; `cargo check -p zz-browser
--all-targets --features cef/dox` type-checks the runtime without it.
The one-slot mailbox model, whatever the tier: CEF hands over a callback-scoped
resource, `cef_runtime` copies it exactly once into something zz owns (a `Vec<u8>`
from `on_paint`, or a zz-owned destination texture/`IOSurface` from
`on_accelerated_paint`), and publishes that into a
[`FrameMailbox`](/browser/osr-rendering.md) holding at most one pending frame. A
new frame replaces a stale unread one, `take()` hands the consumer the owned
frame, and the app moves it onward without another pixel copy. A slow ala
consumer therefore never builds an unbounded queue; it always paints the newest
frame.

## Remote proxy plumbing

A local pane uses its plain named request context and the client's network. A
pane attached through ssh uses a local `<profile>@egress-<hash8>` context whose
proxy preference targets the managed `ssh -D` SOCKS listener.
`BrowserProfilePaths::egress_profile_name` derives the bounded composite name,
and `BrowserRuntime::set_profile_proxy` installs `fixed_servers` with
`socks5://127.0.0.1:<port>` plus Chromium's `<-loopback>` bypass override. The
logical profile name remains the daemon-owned descriptor; the composite context
and its per-host cookie jar never cross the wire.

# Public API surface

`lib.rs` re-exports the browser-neutral types (always available) and the CEF
runtime types (only under `cef-runtime`):

| Re-export | Module | Role |
| --- | --- | --- |
| `SessionId`, `BrowserEvent`, `BrowserCursor`, `ContextMenuRequest`, `EditFlags` | `event` | Owned browser-domain events, cursor shapes, and context-menu state |
| `OsrFrame`, `FrameMailbox`, `FrameError`, `FrameMailboxDiagnostics`, `FrameTier`, `OwnedBgraFrame`, `GpuFrame`, `BrowserGpuContext` (+ `MacGpuFrame`/`MacIoSurface` on macOS) | `frame` | Latest-frame mailbox and the three frame payloads |
| `Viewport`, `Modifiers`, `PointerEvent`, `WheelEvent`, `KeyInput`, `BrowserKey`, `KeyAction`, `EditCommand`, `PointerButton`, `PointerPhase` | `input` | Browser-neutral input + viewport types |
| `RuntimePhase`, `SessionPhase` | `lifecycle` | Runtime + session state machines |
| `BrowserProfilePaths` (`under`, `normalize`), `BrowserProfileError`, profile-name constants/normalization | `profile` | Safe named-profile validation, path resolution, creation; the default directory's name also names the default profile |
| `parse_cookie_import`, `BrowserCookie`, `CookieImportBatch`, `CookieImportResult`, `SiteDataClearResult` | `cookie` | Bounded Cookie-Editor/Netscape parsing plus browser-data operation results |
| `BrowserBootstrap`, `BrowserRuntime`, `BrowserSession`, `BrowserError`, `RuntimeSignal`, `bootstrap`, `bootstrap_with_profile_paths`, `run_subprocess` (+ `bootstrap_windows`) | `cef_runtime` | CEF init, subprocess dispatch, session control. `bootstrap` takes a closure that resolves the profile paths, called only in the browser process; zz passes `browser::profile_paths` |

# Key files

| File | Role |
| --- | --- |
| `src/lib.rs` | Crate root; module declarations and the public re-export surface. Gates `frame`/`artifact` on `host` and `cef_runtime` on `cef-runtime`. |
| `src/cef_runtime.rs` | The only CEF-linked module: `BrowserRuntime`, `BrowserSession`, `bootstrap`/`run_subprocess`, all `cef::wrap_*!` handlers, `on_paint`, input dispatch. |
| `src/frame.rs` | The `OsrFrame` tier enum + one-slot `FrameMailbox` (latest-wins publish, wake coalescing, dimension/byte-length validation, GPU-import failure tracking). |
| `src/event.rs` | `SessionId`, `BrowserEvent` variants, `BrowserCursor`. |
| `src/input.rs` | Browser-neutral `Viewport`, packed `Modifiers`, pointer/wheel/key input types + `windows_key_code` mapping. |
| `src/lifecycle.rs` | `RuntimePhase` and `SessionPhase` strict transition tables. |
| `src/page_zoom.rs` | Chrome percentage ladder and logarithmic CEF zoom-level conversion. |
| `src/profile.rs` | `BrowserProfilePaths`, profile-name rules; encoded named paths, user-only permissions. zz picks the data dir in `crates/zz/src/browser/mod.rs`. |
| `src/cookie.rs` | Cookie-Editor JSON/Netscape parsing, normalization, limits, and secret-free result types (CEF-free, so `zz-chrome-import` builds without Chromium). |
| `src/element_picker.rs` | Token-guarded in-page element-picker protocol state (`ElementPickState`). |
| `src/element_picker.js` | Injected renderer-side script backing the element picker (~45 KB). |
| `Cargo.toml` | Takes the workspace `cef` dependency (pinned in the root `Cargo.toml`, `accelerated_osr` feature) as an optional dep behind `cef-runtime`. |

# Module map (concept links)

- [CEF runtime & subprocess dispatch](/browser/cef-runtime.md) . `cef_runtime.rs` + `lib.rs`
- [Off-screen rendering pipeline](/browser/osr-rendering.md) . `frame.rs` + `on_paint`/DPI handling
- [Runtime & session lifecycle](/browser/lifecycle.md) . `lifecycle.rs` + `event.rs`
- [Named private profiles & request contexts](/browser/profile.md) . `profile.rs`
- [Input translation](/browser/input-translation.md) . `input.rs` + `zz-client`'s `url_input.rs`
- [Element picker](/browser/element-picker.md) . `element_picker.rs`

# Related

- Consumed by [`crates/zz`](/crates/zz.md), the ala mux client, which keys one
  CEF `BrowserSession` per browser pane ID and paints its frames via a custom ala
  element.
- CEF is pinned through `Cargo.lock`; the resolved versions and per-target archive
  hashes are recorded in `third_party/cef/ARTIFACTS.md`, described by
  [CEF artifacts](/references/cef-artifacts.md).
- Fits into the overall [process model](/architecture/process-model.md) and
  [data flow](/architecture/data-flow.md).
- Session restore behavior is described under
  [session persistence](/concepts/session-persistence.md).
