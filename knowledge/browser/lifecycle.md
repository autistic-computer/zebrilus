---
type: Subsystem
title: Browser runtime & session lifecycle
description: Runtime/profile-context/session state machines and the browser-neutral events that CEF callbacks translate into.
resource: libs/ala-cef/src/lifecycle.rs
tags: [browser, lifecycle, events, state-machine]
timestamp: 2026-11-24T00:00:00Z
---

# Overview

`libs/ala-cef/src/lifecycle.rs` defines the runtime and session transitions.
`BrowserRuntime` in `cef_runtime.rs` manages persistent request contexts and creates a
`BrowserSession` for each windowless browser. The desktop drains session events on
its foreground thread.

# State machines

`RuntimePhase` (`lifecycle.rs`), the global CEF runtime:

```text
Uninitialized -> Initializing -> Running -> Closing -> Closed
                             \-> Failed
```

`SessionPhase`, one browser session:

```text
Creating -> Ready -> Crashed
   |          |         |
   +----------+---------+-> Closing -> Closed
```

The runtime also accepts `Uninitialized -> Closed` and `Initializing -> Closing`.
`may_transition_to` in each enum defines the allowed edges. A
crashed session cannot go backward to `Creating`; recovery creates a **new** session
generation against the same [profile](/browser/profile.md). `Failed` is terminal
only when CEF never initialized; a failure after successful init still follows
`Closing -> Closed` so partial startup cannot bypass CEF cleanup.

# Runtime control (`BrowserRuntime`)

| Method | Role |
| --- | --- |
| `handle_context_initialized()` | On `ContextInitialized`, create the `default` persistent `RequestContext` (still `Initializing`). |
| `handle_request_context_initialized(profile)` | Mark the tagged context ready; the default callback advances the runtime to `Running`. |
| `ensure_profile_context(profile)` | Validate/create a named zz profile and lazily start its request context; returns whether it is ready. |
| `create_session(profile, initial_url, viewport, page_zoom_factor, ...)` | Create a windowless browser against a ready named context; requires `Running`. |
| `do_message_loop_work()` | Step the external CEF pump (see [CEF runtime](/browser/cef-runtime.md)). |
| `shutdown()` | Refuses while sessions/data operations remain, releases every profile context, then calls `cef::shutdown()` exactly once. |

`Drop for BrowserRuntime` logs an error if dropped while still initialized:
`cef::shutdown` ordering must be explicit. `active_sessions` is an
`Arc<AtomicU64>` incremented when preparing ordinary sessions or popups. An
`ActiveCountGuard` releases each count once through `on_before_close`,
`mark_closed`, or destruction of an aborted creation's state.

On Unix, zz handles SIGTERM, SIGINT and SIGHUP. `BrowserRuntime::start` sets
`disable_signal_handlers = 1`. The desktop installs its handlers at startup and
reclaims them after CEF starts. On the first signal, `crates/zz/src/quit_signal.rs`
asks ala to quit and starts a two-second deadline for `_exit(128 + signal)`.
A repeat of the same signal takes its default action. This lets zz quit even if
the external CEF pump has stalled, as observed in four macOS profiling runs on 2026-09-24.

For remote panes, the controller applies the current SOCKS port before creating the session
and refreshes it on reconnect. [Logical profiles and remote-egress jars](/browser/profile.md#logical-profiles-and-remote-egress-jars)

# Session control (`BrowserSession`)

- **State:** `mark_ready` (`Creating -> Ready`), `mark_crashed`
  (`Ready -> Crashed`, clears the frame mailbox and cancels any element pick),
  `mark_closed` (`-> Closed`, decrements the live count once), `close(force)`
  (`-> Closing`, calls `host.close_browser`). `Drop` force-closes an un-closed
  browser.
- **Navigation:** `navigate(url)`, `go_back`/`go_forward` (guarded by
  `can_go_back`/`can_go_forward`), `reload`.
- **View:** `set_viewport` (see [DPI handling](/browser/osr-rendering.md)),
  `set_focus(bool)` → `host.set_focus`, and `zoom_in`/`zoom_out`/`reset_zoom`
  → the Chrome percentage ladder while retaining the raster-scale factor.
- **Input / IME:** `send_pointer`, `send_wheel`, `send_key`, `send_text`, and the
  composition calls, covered under [input translation](/browser/input-translation.md).

# Schema . `BrowserEvent`

`libs/ala-cef/src/event.rs` defines the full enum and its payload types. Each event
has a `SessionId` for one browser generation. `BrowserEvent::session()` lets the desktop
discard late events after replacement.

| What changed | What the desktop receives |
| --- | --- |
| Page or load | Address, title, favicon, loading and progress, load failure, status text or fullscreen state. Favicon events carry the captured page URL and PNG. |
| Rendering or input | Frame generation, shared-texture failure, cursor, IME character bounds or find results. |
| User interaction | Context-menu, file-dialog, JavaScript-dialog and download requests, plus download progress. A completed [element pick](/browser/element-picker.md) carries text and an optional PNG. |
| Tab lifetime | Creation, renderer termination, close, and the two popup events described below. |

# Focus, resize, popups

- **Focus** is one-directional: `set_focus(true/false)` forwards to the CEF host;
  focus is never shared with the terminal surface.
- **Resize** flows through `set_viewport` → `apply_viewport`, which drives
  `was_hidden`, `notify_screen_info_changed`, and `was_resized` in CEF's reference
  order (see [OSR rendering](/browser/osr-rendering.md)).
- **Popups / new windows:** `on_before_popup` allows Chromium to create a
  windowless child with its own handlers, mailbox, and session ID. The app adopts
  that child into a tab through `PopupCreated` and `BrowserSession::take_popup`,
  preserving Chromium's opener relationship, named-window reuse, and delayed
  navigation from an initially blank document. Chromium retains control over
  JavaScript access between the windows. `LifeSpanHandlerBuilder::on_before_popup` copies the opener's shared-texture
  and external BeginFrame settings into the child's windowless `WindowInfo`.
  The app displays popup content even at `about:blank`, and removes a popup tab
  when its browser closes itself. Closing an opener also closes any children still
  waiting for adoption. Adopted children have independent tab lifetimes.
  `on_open_urlfrom_tab` still emits `PopupRequested` for ordinary URL tab requests.
  Only `NEW_BACKGROUND_TAB` dispositions open in the background. `DeniedPermissionHandler` handles privileged permission requests.

# Popup regression

Run `zz_browser_fixture --popup-regression` from a CEF-capable fixture bundle. On macOS, copy a
built `zz Dev.app`, put `target/debug/zz_browser_fixture` in place of its `Contents/MacOS/zz`, and
re-sign the copy ad hoc (`codesign -s - -f` on the binary, then on the app); run beside `zz` instead,
CEF's GPU helper fails sandbox setup and the fixture exits. `--ime-regression` runs the same way.
The fixture uses a temporary `zz-default` profile and a loopback server to check
blank-document pixels, delayed navigation, named-window reuse, opener messages,
and JavaScript closure. On macOS the profile directory must differ from Chromium's
`Default` directory, including case. The fixture waits for the opener's rendered
page before clicking its controls. For manual checks, serve the fixture and visit
`/popup-fixture` in a browser pane.

# Key files

| File | Role |
| --- | --- |
| `src/lifecycle.rs` | `RuntimePhase`/`SessionPhase` + `may_transition_to` tables and tests. |
| `src/event.rs` | `SessionId`, `BrowserEvent`, `BrowserCursor`, `BrowserEvent::session()`. |
| `src/cef_runtime.rs` | `BrowserRuntime`/`BrowserSession` methods and the handlers that emit events. |

# Related

- Driven by the [CEF runtime](/browser/cef-runtime.md); events consumed by the ala
  client in [`crates/zz`](/crates/zz.md), which discards events from a stale
  session generation.
- Sessions restore against the persistent [profile](/browser/profile.md); see
  [session persistence](/concepts/session-persistence.md).
- Part of the [ala-cef crate](/crates/ala-cef.md).
