# Ladybird in a zz browser pane

This Linux x86-64 spike embeds the first Ladybird alpha, 2026.47.7, in zz's existing desktop browser pane. The baseline is zz at `e6d337080` with CEF 156.0.15 and Chromium 156.0.8078.80. Ordinary desktop builds select CEF. [Ladybird release](https://github.com/LadybirdBrowser/ladybird/releases/tag/2026.47.7)

![Ladybird's website open inside a zz desktop browser pane](desktop.png)

## Run it

Run either command from the worktree on Linux x86-64. The launcher needs the usual zz build dependencies, a working graphical session, `curl`, `ar`, and `tar`.

```sh
just ladybird run
just ladybird run 'https://en.wikipedia.org/wiki/Terminal_emulator'
```

`just ladybird build` verifies the official Ubuntu 24.04 package against its pinned SHA-256, extracts the engine helpers, resources, libraries and notices, and builds the release desktop and CLI. It stages everything under `target/ladybird-app`, or under `CARGO_TARGET_DIR/ladybird-app` when set. The launcher uses that bundle's own profile, config, cache, and daemon socket. The Qt browser and WebDriver executables are omitted.

## Integration

The adapter spawns the released Hub through an inherited Unix socket. Each zz profile owns a Hub. Each pane in a profile owns a separate view. The Hub manages networking, content and compositor processes. The desktop forwards navigation, focus, viewport changes and input over Hub IPC. Ladybird renders into shared-memory BGRA buffers, passes their file descriptors over the socket, and receives acknowledgements when a backing store becomes available again. The adapter copies the active rectangle into zz's frame mailbox. Ala uploads and presents it using its existing browser-pane renderer. [Pinned Hub protocol](https://github.com/LadybirdBrowser/ladybird/tree/2026.47.7/Services/Hub)

The existing GPUI project Elaris provided a reference for embedding Ladybird. This spike uses the released engine binaries directly and carries its own small Rust implementation of the messages zz needs. [Elaris](https://github.com/elarisbrowser/elaris)

## What ran

The end-to-end harness starts a real desktop client on a private daemon, opens a browser through the zz CLI, waits for JavaScript in a loopback page, and verifies a captured background pixel on the readback path. It types into a form, activates the button with Tab and Enter, opens a split, checks the new viewport and verifies that the second page can read the first page's local storage. It then navigates the split to a four-second animated canvas and a JavaScript and DOM workload, zooms the split to the full pane for four seconds of scrolling, and checks that a separately named profile starts with empty storage. Live HTTPS smoke tests rendered the Ladybird home page, a Wikipedia article and a GitHub repository. Those three checks covered initial page rendering. The real-engine integration test also clicked with the mouse, scrolled with the wheel, loaded a data URL above the 64 KiB inline IPC limit, traversed history in both directions, and answered a JavaScript prompt.

## Scope

Linux x86-64 on Wayland was exercised. The wire IDs and field layouts follow one alpha tag. A subsequent Ladybird release needs a protocol check. macOS needs its own Hub transport and backing-store import. This adapter forces CPU painting and copies a full visible frame before GPU upload. Large panes, high-DPI displays, video, and several continuously animated tabs can change both CPU and memory costs.

The spike omits Chromium DevTools, element picking, Chrome-cookie import, zz's custom local URL scheme and remote-host proxy egress. Downloads, file pickers, popups, accessibility trees, clipboard integration, pre-edit IME composition and media controls need further implementation. Login flows, WebRTC, DRM, crash recovery, macOS and Windows were outside the measured workload.

## Reproduce the comparison

Build both desktops from the worktree with the dev identity so the harness can give each run a fresh profile. Install Python's Pillow and psutil. Run in a clean graphical session and use an empty output directory name for each invocation. `--readback` selects CEF's CPU-readable presentation path and enables its PNG assertion.

```sh
just ladybird build
ZZ_DEV_BUILD=1 cargo xtask bundle-cef --release --output target/cef-comparison
bench/ladybird/compare.py --binary target/cef-comparison/zz \
  --cli target/ladybird-app/cli --hub target/ladybird-app/runtime/libexec/Hub \
  --output target/cef-trial
bench/ladybird/compare.py --binary target/ladybird-app/zz \
  --cli target/ladybird-app/cli --hub target/ladybird-app/runtime/libexec/Hub \
  --output target/ladybird-trial
ZZ_LADYBIRD_HUB="$PWD/target/ladybird-app/runtime/libexec/Hub" \
  cargo test -p zz-browser-runtime --no-default-features --features ladybird -- --include-ignored
```
