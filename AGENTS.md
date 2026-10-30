# AGENTS.md

Zebrilus (zz, repo autistic-computer/zebrilus) is a tmux-superset terminal multiplexer that ships as a native GPU desktop app: a Rust workspace built on ala (our fork of the GPUI UI framework, in `libs/ala`), a persistent daemon that owns sessions and PTYs, Chromium browser panes (CEF off-screen rendering), agent panes (ACP), and remote hosts over plain ssh. Targets macOS and Linux (Wayland), with experimental Windows/WSL, iOS and Android clients, and a raw-terminal attach client.

Rust edition 2024, MSRV 1.99. Release builds on mac/windows require Zig 0.16.0 (see `mise.toml`).

## Project map

- `libs/ala` - ala, our GPUI, a git subtree of [autistic-computer/ala](https://github.com/autistic-computer/ala) with its own Cargo workspace: `crates/ala` (the gpui core split out of upstream, with the upstream utility code it needs as modules: `ala::collections`, `sum_tree`, `scheduler`, `util`, `http_client`, `refineable`), `crates/ala-platform` (the backends, one module per target, each compiled only there: `macos` with the Metal renderer in `apple`, `linux` (Wayland, X11), `windows`, `web`, `ios` (UIKit), `android` (JNI and the NDK), and the `wgpu` renderer Linux, web, iOS and Android share), `crates/ala-macros` (proc macros, including the `Refineable` derive), and `third_party/cosmic-text`
- `libs/penna` - `penna`, the widget kit on ala (theme, primitives, widgets, icons), a git subtree of [autistic-computer/penna](https://github.com/autistic-computer/penna): a maintained fork of gpui-component with no zz dependencies, usable by other apps
- `libs/ala-cef` - `ala-cef`, Chromium off-screen rendering for wgpu apps (CEF frames as GPU textures, input, profiles, cookies, the element picker and its TypeScript source in `picker/`), with its CEF sys adapter in `third_party/cef-dll-sys`; without features it is the pure half the thin clients' browser panes and `zz-chrome-import` use, and `zz-cli` must not link it. It is set up like the other libs, but its repo, autistic-computer/ala-cef, does not exist yet
- `crates/zz` - desktop client: ala shell, terminal/browser/agent panes, settings, daemon client
- `crates/zz-daemon` — the daemon: session state, PTY workers, client connections
- `crates/zz-daemon-client` - the client half of the daemon: local and ssh endpoints, askpass, `CommandClient`/`InteractiveClient`, transport, paths, and process facts; client-only crates depend on it instead of `zz-daemon`
- `crates/zz-mux` — tmux-compatible model: sessions, windows, panes, key tables
- `crates/zz-protocol` — wire protocol between daemon and clients, plus the shared key contract (tables, engine, fold, command catalog)
- `crates/zz-client` - sans-IO client core: protocol reduction, chrome keymap, daemon-backed convergence simulator
- `crates/zz-config` - renderer-free application config, settings actions, preference persistence; update checks behind its `update` feature
- `crates/zz-machine` - the shape of zz's pure state machines: a transition returns the next state, effects and rejects, and the state says when to wake; the agent turn machine (`zz-daemon/src/agent/turn.rs`) is built on it
- `crates/zz-cli` — headless `zz_cli` binary: the CLI, raw-terminal attach, and the ssh-side entry point
- `crates/zz-terminal` — terminal engine: PTY sessions, libghostty-vt state, frame snapshots
- `crates/zz-chrome-import` — Chrome profile, cookie, and history import
- `crates/zz-ui` - zz's application UI on penna (panes, terminal painting, agent, palette, settings, navigation, phone shell); kit names come from `penna` directly, not through it
- `crates/zz-tui` — raw-terminal attach client as a library (the binary lives in `zz-cli`)
- `crates/zz-web` - local HTTP/WebSocket gateway for browser clients
- `crates/zz-hotreload` - Subsecond hot-reload launcher for `just hot linux`
- `clients/app` - `zz-app`: the thin-client app web, iOS and Android share (shell, sidebar, status bar, settings, palette, overlays, panes, connection reducer, image cache, the native transport and the iOS and Android browser panes)
- `clients/web` - `zz-web-client`: the WASM entry point around zz-app, with its own Cargo workspace
- `clients/dom` - `zz-dom`: an HTML thin client, `zz-client` in wasm drawn with `design/kit`, served by `just design client`
- `clients/ios` - `zz-ios`: the iOS entry point around zz-app and ala-platform's iOS backend, the bundle files, and a terminal example (`ZZ_ALA_DEMO=terminal`); `just ios`
- `clients/android` - `zz-android`: the Android entry point around zz-app and ala-platform's Android backend, the Java glue (activity, surface view, input connection, web view), the manifest and resources; `just android`
- `crates/zz-xtask` — build tooling: CEF bundling, packaging (`cargo xtask`)
- `compat/` — tmux compat campaign: differential harness (`run.sh`), gap registry (`tmux-gaps.json`), dispatch-board client (`board.py`), progress meter, orchestration handoff (`orchestration/`)
- `compat/tui/` — TUI parity campaign: proof ledger (`campaign.json`), validator and report generator (`tracker.py`), cycle runners (`run-N.js`); closed 2026-09-20 at 18/18
- `compat/catchup/` — tmux catch-up campaign (stale refusals, pin to 3.8, floating panes): rules and resume steps in `README.md`, work items in `ledger.json`
- `knowledge/` — OKF knowledge bundle for the whole system (start at `index.md`)
- `scripts/` — build, packaging, and profiling scripts
- `bench/` — terminal throughput benchmark harness
- `design/` - HTML mockups painted with the real theme: `crates/zz-ui/src/theme_css.rs` writes every role's look as CSS (`just design tokens`), `just design serve` shows them; see `design/README.md`
- `site/` — zzmux.sh, which serves only `install.sh`; the landing page and docs were wiped 2026-10-30 for a rework
- `third_party/` — vendored code (`ghostty/`, a trimmed Ghostty synced with `just vendor ghostty <rev>`; the libghostty-vt crates under `rust/`) and pinned reference material
- `packaging/` — Arch package, AUR/cask templates

## Knowledge bundle

`knowledge/` documents architecture, the wire protocol, tmux compat, the terminal engine, the CEF browser, designs, and operational playbooks. Read `knowledge/index.md` before digging into an unfamiliar subsystem — it beats cold grepping. The bundle is a map, not ground truth: load-bearing facts cite `resource:` source files; verify those before acting on them.

For TUI parity campaign work, start with `compat/tui/README.md` and
`knowledge/playbooks/tui-parity-campaign.md`. `python3 compat/tui/tracker.py check` validates the
ledger and its generated report; `ready` lists the dependency-ready obligations. Only verified proof
counts as TUI progress; accepted tmux gaps do not.

<important if="you need to build, run, test, lint, package, profile, or release">

Use just 1.52 or newer. Run `just` for the command groups, `just <group>` for its actions,
and `just --list --list-submodules` for the full tree. Keep related workflows under
`just <group> <action> [arguments]`; common desktop commands stay at the root.
Recipes live in `Justfile` and `scripts/just/*.just` and run from the repo root.

| Command | What it does |
|---|---|
| `cargo ci-test` | Tests (what CI runs): `cargo test --workspace --all-features` (alias in `.cargo/config.toml`). `libs/` sits outside the workspace; `just libs check <ala\|penna\|ala-cef>` lints and tests each lib the way CI does. CI's Linux leg runs ala-platform's whole suite, wgpu pixel tests included, on Mesa lavapipe (software Vulkan): `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json cargo test --manifest-path libs/ala/Cargo.toml -p ala-platform --all-features` (Ubuntu `mesa-vulkan-drivers`, Arch `vulkan-swrast`; on an NVIDIA box also set `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json`, or the atlas tests take NVIDIA's GL backend and fail). macOS and Windows run only its iOS input tests: `cargo test --manifest-path libs/ala/Cargo.toml -p ala-platform --all-features --test ios` |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Lint (what CI runs) |
| `cargo clippy -p zz-daemon -p zz-daemon-client --no-default-features --all-targets -- -D warnings` | Lint the daemon without agent support (CI runs this too) |
| `cargo fmt --all` | Format |
| `just run <mac\|linux> [--verbose] [--features <list>]` | Launch isolated zz Dev (own daemon, config, browser, and data). Extra args are those two flags, not Cargo passthrough. No `windows` |
| `just watch <platform>` | Rebuild and relaunch on source change |
| `just build <platform>` | Release bundle into `dist/zz` (wraps `cargo xtask bundle-cef`) |
| `just install mac` | Build and swap `/Applications/zz.app`; the daemon survives the swap |
| `just android <setup\|emulator\|run\|build\|device\|logs>` / `android rig [stop]` | ala Android app: install the SDK, NDK, emulator and `zz-phone` AVD, boot it, build and launch on it attached to the rig through `adb reverse`, build the APK, install a release arm64 build on a USB or wireless device, or follow its logcat |
| `just ios <run\|build\|device\|bench> [iPhone\|iPad]` / `ios rig [stop]` / `ios testflight` | ala iOS app (iPhone by default): simulator, a paired device (release, signed, installed), a hands-free frame benchmark on the device, a throwaway daemon that `ios run` attaches to, or a signed `dev.zz.ios` TestFlight upload; `ZZ_ALA_DEMO=terminal` selects the terminal example |
| `just libs <check\|push\|pull> <ala\|penna\|ala-cef>` | Lint and test a lib alone, push zz's `libs/<lib>` commits to its autistic-computer repo, or merge that repo's commits back (git subtree) |
| `just site` | Deploy zzmux.sh (copies `install.sh` and runs wrangler) |
| `just web run` / `web setup` / `web build [--release]` / `web serve` | Browser client dev loop / toolchain / assets / local gateway |
| `just profile <cpu\|memory\|startup\|system\|metal\|terminal> mac …` | Capture profiling data; read it back with `just profile summary <cpu\|metal\|terminal> <run>` |
| `just profile build mac` | Release-optimized bundle with dSYMs for profiling |
| `just package mac` / `package windows` / `package arch` / `install arch` / `package deb` / `install deb` | Platform packages |
| `just release bump <level-or-version> [--execute]` | Preview or publish a version bump (setup: `just release setup`) |
| `just release mac build <version>` | Full signed+notarized DMG (setup: `release mac setup`; pieces: `release mac sign`, `release mac notarize`, `release mac verify`, `release mac check`) |
| `bench/run.sh` | Terminal throughput benchmarks |
</important>

<important if="you are about to stash, reset, revert, or clean the working tree">
Multiple agent sessions often share this checkout in parallel. Never `git stash`, hard-reset, or discard uncommitted changes you did not author — you may be destroying another session's in-flight work.
</important>

<important if="you are changing anything under libs/">

- `libs/ala` and `libs/penna` are git subtrees of [autistic-computer/ala](https://github.com/autistic-computer/ala) and [autistic-computer/penna](https://github.com/autistic-computer/penna), libraries other apps build on; `libs/ala-cef` follows the same rules and becomes a subtree of autistic-computer/ala-cef once that repo exists. Edit them in place and commit them with the zz code that needs the change, like any other file here.
- No comments in their code, doc comments included. Third-party copyright and license notices live in each lib's `NOTICE`, and every README is the same centered `<samp>` block: the three-row bird drawn in quadrant block characters over the frontmatter, padded with literal non-breaking spaces so the lines stay aligned, then a `<kbd>` link to autistic.computer.
- They must build on their own: no zz crate dependencies and no paths outside their own directory. Each is its own Cargo workspace, excluded from zz's; zz uses them by path, and the root `[patch."https://github.com/autistic-computer/ala"]` (repeated in `clients/web`, `clients/storybook` and `clients/dom/theme`) points the kit's git dependency on ala at `libs/ala`. The root `[patch.crates-io]` points `cef-dll-sys` at ala-cef's adapter, as ala-cef's own manifest does.
- The root clippy and `cargo ci-test` skip them. Run `just libs check <ala|penna|ala-cef>` when you touch one; CI runs the same.
- Commits touching `libs/<lib>` reach its repo with `just libs push <lib>` once they are on main, carrying their commit messages along, so write those for a reader outside zz. `just libs pull <lib>` merges commits made in the lib repo; do that before pushing if the lib repo moved.
</important>

<important if="you are changing ala (our gpui)">

- ala is `libs/ala`: the crates `ala`, `ala-macros` and `ala-platform`, our copy of the gpui crates split out of upstream, with upstream's utility crates folded into `ala` as modules and every backend folded into `ala-platform`. It is not a patch branch: there is nothing to rebase, and upstream fixes come in by hand. Change ala in the same commit as the zz code that needs it.
- `just libs check ala` lints them with all features, but only for the host OS: each backend in ala-platform compiles for its own target only. Check the others from macOS with `cargo clippy --manifest-path libs/ala/Cargo.toml --target <triple> -p ala-platform --all-features` (`libs/ala/UPSTREAM.md` has the zig and NDK setup that cross-compiles their C dependencies). CI's Linux leg runs ala-platform's tests on lavapipe. The Wayland swapchain test (ignored) and the Metal and DirectX renderers stay unchecked in CI. Run `cargo test --manifest-path libs/ala/Cargo.toml -p ala-platform --all-features` when you touch it, on a GPU or with lavapipe selected as above. The three carry upstream's relaxed `[lints]` table instead of zz's pedantic set, and `[profile.dev.package]` in the root `Cargo.toml` optimizes them in debug builds so those stay fast enough to use. `libs/penna` is ours and carries zz's pedantic lints in its own manifest.
- `clients/web` consumes ala's WASM renderer in an excluded workspace; check `just web build` after an ala change.
- `knowledge/references/ala.md` has the full recipe; `libs/ala/UPSTREAM.md` maps upstream's paths to ala's for pulling an upstream fix. The `ghostty-sync` skill covers the vendored Ghostty.
</important>

<important if="a test fails under cargo test --workspace">
A few `zz-daemon` tests are timing-sensitive and only fail under full-workspace parallel load. Before diagnosing, re-run the failing test alone (`cargo test -p zz-daemon <test_name>`); a solo pass points to load-induced flake, not your change. On headless machines `concurrent_default_interactive_attaches_atomically_share_session_zero` fails deterministically with `open terminal failed: not a terminal` — environmental, not a regression. Its panic is raised on a spawned thread, so libtest can attribute the failure to an innocent neighboring daemon test; a one-off daemon failure with that error text is this test misattributed.
</important>

<important if="you are debugging a running daemon or checking the CLI">

- The daemon outlives the app: after installing a new build, existing sessions keep running the old daemon binary until it restarts. Don't chase "missing" behavior in a stale daemon.
- `just run` / `just watch` build zz Dev with `ZZ_DEV_BUILD=1`, clear inherited daemon/pane context, and use `zz-dev` config/data/socket paths. The macOS bundle is `dist/zz-dev/zz Dev.app` (`dev.zz.app.dev`); Linux launches `target/debug/zz-dev`. Installed and beta packages keep their existing behavior.
- `just web run` / `web build` / `web serve` use dev assets and port 8081. Dev SSH selects `zz-dev`, linked by desktop dev runs into `~/.local/bin`. `web build --release` retains production identity. `just ios run iPad` opens the session sidebar and daemon-backed terminal/agent panes; `just ios run` (iPhone) opens the phone shell, one pane per screen, paged by the bar's pill, with a drill-in overview as its navigator (Overview in the pill's menu or dragging the pill up zooms out to the host's windows as a bento grid; All hosts in the host pill's menu or dragging it up again zooms out to every host); the gear on the Hosts page, or Settings in a host pill's menu, opens the thin-client settings (iPhone hides Panes and Status bar); set `ZZ_ALA_DEMO=terminal` for the terminal example. See `clients/ios/README.md` for endpoint selection. `just android run` installs the same app on the `zz-phone` emulator and attaches it to `just android rig` (the iOS rig) through `adb reverse`; see `clients/android/README.md`.
- `ZZ_SOCKET` overrides the socket the app dials. Unix socket paths have a low length cap (`sun_path`); put test sockets directly under `/tmp`.
- Recipes live in `knowledge/playbooks/running-zz.md`.
</important>

<important if="you are building or styling UI chrome or widgets">

- UI conventions: `knowledge/configuration/ui-conventions.md` (chrome colors come from the theme; clippy rejects raw `rgb`/`hsla`).
- `libs/penna` is a full fork of gpui-component, not a dependency - read `libs/penna/UPSTREAM.md` before touching widget internals or trying to "update" it. App-specific UI belongs in `crates/zz-ui`; the kit must not depend on zz crates.
</important>

<important if="you are adding or editing documents under knowledge/">

- The bundle follows OKF v0.1: YAML frontmatter per document, and each directory's `index.md` carries a managed listing fence that mirrors frontmatter descriptions — keep both in sync when adding or renaming documents.
- Design documents under `knowledge/designs/` carry a `status:` field; update it when a design ships or dies.
- If a page contradicts the source it cites, fix the page — source is ground truth.
</important>

- No AI attribution in commits, PR descriptions or branch names: no co-author trailer with an email (never `@anthropic.com` or `@openai.com`), no model or product name as author or co-author, no "Generated with ..." line, no `claude/` or `codex/` branch prefix. A co-author trailer is a funny name with no email. `.claude/settings.json` turns Claude Code's own attribution off.
- No research about other products in the repo, commit messages, PR descriptions or code: no surveys, teardowns, comparisons or benchmarks against competitors, and no "like X does" to justify a choice. Say what zz does with its own numbers. Upstreams, dependencies, standards and protocols zz implements can be named.
- No comments in code, doc comments and Python docstrings included. What stays is what a tool reads: shebangs, `# shellcheck` and `# noqa` directives, `@ts-check`, the help headers `bench/run.sh`, `bench/inner.sh`, `bench/gen-fixtures.sh` and `compat/run.sh` print with `sed`, and module docstrings that argparse shows through `__doc__`. Just recipe docs are `[doc("...")]` attributes
