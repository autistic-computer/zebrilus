# AGENTS.md

Read `~/dev/autistic-computer/.agents/AGENTS.md` first: the rules every autistic-computer repo
shares. This file adds what is specific to zz.

@~/dev/autistic-computer/.agents/AGENTS.md

Zebrilus (zz) is a terminal multiplexer. A persistent daemon owns the sessions and PTYs.
The desktop app renders terminal, browser and agent panes. Other clients attach through the
CLI, TUI, web gateway or mobile app. Host connections include SSH, tailnet and, on Windows, WSL.

`Cargo.toml` and `rust-toolchain.toml` specify the Rust edition and toolchain.
`mise.toml` pins Zig for macOS and Windows release builds.

## Project map

| Path | Owns |
| --- | --- |
| `crates/zz`, `crates/zz-ui` | Desktop client and zz-specific UI shared with thin clients. Reusable widgets belong in penna. |
| `crates/zz-daemon`, `crates/zz-mux` | Session and PTY ownership, command execution and the tmux model. |
| `crates/zz-agent` | Agent runtime and drivers (Claude Code, Codex, ACP), journal and logins. zz-daemon builds it under its `agent` feature. |
| `crates/zz-protocol`, `crates/zz-client` | Wire messages, key tables and command catalog. Renderer-independent client state. |
| `crates/zz-daemon-client`, `crates/zz-tailnet` | Endpoints, transport and daemon lifecycle. The latter runs zz's own tailnet node on Linux and Apple Silicon macOS. |
| `crates/zz-cli`, `crates/zz-tui`, `crates/zz-terminal` | Headless entry point, raw-terminal attach client, PTYs and viewport snapshots. |
| `crates/zz-config`, `crates/zz-machine` | Preferences and updates, including the owned headless copy. Pure state machines used by the agent runtime. |
| `crates/zz-web`, `clients/app`, `clients/{web,ios,android}` | HTTP/WebSocket gateway, shared thin client and platform entry points. |
| `clients/dom`, `crates/zz-showcase-daemon` | HTML client and the in-browser showcase's daemon, CLI and socket proxy. |
| `crates/zz-simulator`, `crates/zz-hotreload`, `crates/zz-xtask` | iOS Simulator access, Linux hot reload and build tooling. |
| `libs/ala`, `libs/penna` | UI framework, platform backends and widget kit. See [ala](knowledge/references/ala.md) and [penna](knowledge/references/penna.md) for embedding and setup. |
| `libs/ala-cef`, `libs/pele` | Chromium off-screen rendering and Mermaid-to-SVG rendering. The headless CLI must stay free of GUI and browser dependencies. |
| `clients/storybook`, `design/` | zz's catalog in penna's storybook and HTML mockups. See the [storybook playbook](knowledge/playbooks/storybook.md). |
| `compat/`, `bench/` | Differential scenarios, gap registry, TUI evidence, and benchmarks. |
| `third_party/`, `packaging/`, `site/` | Vendored sources and reference pins, package definitions, and the zzmux.sh installer site. |

## Knowledge bundle

Start at [knowledge/index.md](knowledge/index.md). Check a concept's cited source before changing
behavior. For compatibility work, `compat/tmux-gaps.json` owns the command and behavior registry.
The TUI campaign has its own [proof ledger](compat/tui/README.md). Run
`python3 compat/tui/tracker.py check` to validate it and its generated report.

<important if="you need to build, run, test, lint, package, profile, or release">

Use just 1.52 or newer. Run `just` for the command groups, `just <group>` for its actions,
and `just --list --list-submodules` for the full tree. Keep related workflows under
`just <group> <action> [arguments]`; common desktop commands stay at the root.
Recipes live in `Justfile` and `scripts/just/*.just` and run from the repo root.

| Command | What it does |
|---|---|
| `cargo ci-test` | Tests (what CI runs): `cargo test --workspace --all-features` (alias in `.cargo/config.toml`). `libs/` sits outside the workspace; `just libs check <ala\|penna\|ala-cef\|pele>` lints and tests each lib the way CI does. CI's Linux leg runs ala-platform's whole suite, wgpu pixel tests included, on Mesa lavapipe (software Vulkan): `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json cargo test --manifest-path libs/ala/Cargo.toml -p ala-platform --all-features` (Ubuntu `mesa-vulkan-drivers`, Arch `vulkan-swrast`; on an NVIDIA box also set `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json`, or the atlas tests take NVIDIA's GL backend and fail). macOS and Windows run only its iOS input tests: `cargo test --manifest-path libs/ala/Cargo.toml -p ala-platform --all-features --test ios` |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Lint (what CI runs) |
| `cargo clippy -p zz-daemon -p zz-daemon-client --no-default-features --all-targets -- -D warnings` | Lint the daemon without agent support (CI runs this too) |
| `cargo fmt --all` | Format |
| `just run <mac\|linux> [--verbose] [--features <list>]` | Launch isolated zz Dev (own daemon, config, browser, and data). Extra args are those two flags, not Cargo passthrough. No `windows` |
| `just watch <platform>` | Rebuild and relaunch on source change |
| `just build <platform>` | Release bundle into `dist/zz` (wraps `cargo xtask bundle-cef`) |
| `just install mac` | Build and swap `/Applications/zz.app`; the daemon survives the swap |
| `just android <setup\|emulator\|run\|build\|device\|logs>` / `android rig [stop]` | ala Android app: install the SDK, NDK, emulator and `zz-phone` AVD, boot it, build and launch on it attached to the rig through `adb reverse`, build the APK, install a release arm64 build on a USB or wireless device, or follow its logcat |
| `just ios <run\|build\|device\|bench> [iPhone\|iPad]` / `ios rig [stop]` / `ios testflight` | ala iOS app (iPhone by default): simulator, a paired device (release, signed, installed), a hands-free frame benchmark on the device, a throwaway daemon that `ios run` attaches to, or a signed `dev.zz.ios` TestFlight upload; `ZZ_ALA_DEMO=terminal` selects the terminal example |
| `just libs <check\|push\|pull> <ala\|penna\|ala-cef\|pele>` | Lint and test a lib alone, push zz's `libs/<lib>` commits to its autistic-computer repo (pele's is `pele-rs`), or merge that repo's commits back (git subtree) |
| `just site` | Deploy zzmux.sh (copies `install.sh` and runs wrangler) |
| `just web run` / `web setup` / `web build [--release]` / `web serve` | Browser client dev loop / toolchain / assets / local gateway |
| `just profile <cpu\|memory\|startup\|system\|metal\|terminal> mac …` | Capture profiling data; read it back with `just profile summary <cpu\|metal\|terminal> <run>` |
| `just profile build mac` | Release-optimized bundle with dSYMs for profiling |
| `just package mac` / `package windows` / `package arch` / `install arch` / `package deb` / `install deb` | Platform packages |
| `just release bump <level-or-version> [--execute]` | Preview or publish a version bump (setup: `just release setup`) |
| `just release ship <level-or-version>` | Preflight, then bump, tag and build the release on our own boxes: single-use runners on this Mac (macOS, plus Linux aarch64 in Docker), alienware (Linux x86_64 in Docker) and win-desktop, registered only for the run. `just release runners` installs them, `just release dry-run` builds every leg with no tag or publish, `just release stop` cleans up |
| `just preflight [linux] [macos] [windows]` | CI's checks (`scripts/ci.sh`) on HEAD on alienware, this Mac and win-desktop at once, before a push or tag |
| `just release mac build <version>` | Full signed+notarized DMG (setup: `release mac setup`; pieces: `release mac sign`, `release mac notarize`, `release mac verify`, `release mac check`) |
| `bench/run.sh` | Terminal throughput benchmarks |
</important>

<important if="you are changing anything under libs/">

- `libs/ala`, `libs/penna`, `libs/ala-cef` and `libs/pele` are git subtrees of [autistic-computer/ala](https://github.com/autistic-computer/ala), [autistic-computer/penna](https://github.com/autistic-computer/penna), [autistic-computer/ala-cef](https://github.com/autistic-computer/ala-cef) and [autistic-computer/pele-rs](https://github.com/autistic-computer/pele-rs), libraries other apps build on. Edit them in place and commit them with the zz code that needs the change, like any other file here.
- They must build on their own: no zz crate dependencies and no paths outside their own directory. Each is its own Cargo workspace, excluded from zz's; zz uses them by path, and the root `[patch."https://github.com/autistic-computer/ala"]` (repeated in `clients/web`, `clients/storybook` and `clients/dom/theme`) points the kit's git dependency on ala at `libs/ala`. The root `[patch.crates-io]` points `cef-dll-sys` at ala-cef's adapter, as ala-cef's own manifest does.
- The root clippy and `cargo ci-test` skip them. Run `just libs check <ala|penna|ala-cef|pele>` when you touch one; CI runs the same.
- Commits touching `libs/<lib>` reach its repo with `just libs push <lib>` once they are on main, carrying their commit messages along, so write those for a reader outside zz. pele came in by `git subtree add` on a branch, and its add commit names a main commit from before `libs/pele` existed, so a plain `git subtree split` of it pulls zz's own history along; `just libs push pele` splits with `--ignore-joins` onto pele-rs's main instead. `just libs pull <lib>` merges commits made in the lib repo; do that before pushing if the lib repo moved.
- `libs/penna` commits `Cargo.lock` and `storybook/Cargo.lock`, and `libs/ala-cef` its `Cargo.lock`, for their own repos' CI, resolved against ala on GitHub, not `libs/ala`. `libs/pele` commits its `Cargo.lock` too; it has no ala dependency, so its checks just run `--locked`. The recipes that patch in `libs/ala` (`just libs check penna|ala-cef`, `just storybook penna-build`, `just design penna-tokens`) check all three against their manifests first and put them back as they were afterwards; a new recipe like them sources `scripts/lib-locks.sh`. After changing a lib's dependencies, refresh its lockfile without the patch: `cargo update --workspace --manifest-path libs/<lib>/Cargo.toml` (penna's storybook too). When penna or ala-cef needs ala commits newer than its lockfile pins, push ala first, then run `cargo update -p ala` on its manifests and commit that before `just libs push <lib>`.
</important>

<important if="you are changing ala (our gpui)">

- ala is `libs/ala`: the crates `ala`, `ala-macros` and `ala-platform`, our copy of the gpui crates split out of upstream, with upstream's utility crates folded into `ala` as modules and every backend folded into `ala-platform`. It is not a patch branch: there is nothing to rebase, and upstream fixes come in by hand. Change ala in the same commit as the zz code that needs it.
- `just libs check ala` lints them with all features, but only for the host OS: each backend in ala-platform compiles for its own target only. Check the others from macOS with `cargo clippy --manifest-path libs/ala/Cargo.toml --target <triple> -p ala-platform --all-features` (`libs/ala/UPSTREAM.md` has the zig and NDK setup that cross-compiles their C dependencies). CI's Linux leg runs ala-platform's tests on lavapipe. The Wayland swapchain test (ignored) and the Metal and DirectX renderers stay unchecked in CI. Run `cargo test --manifest-path libs/ala/Cargo.toml -p ala-platform --all-features` when you touch it, on a GPU or with lavapipe selected as above. The three carry upstream's relaxed `[lints]` table instead of zz's pedantic set, and `[profile.dev.package]` in the root `Cargo.toml` optimizes them in debug builds so those stay fast enough to use. `libs/penna` is ours and carries zz's pedantic lints in its own manifest.
- `clients/web` consumes ala's WASM renderer in an excluded workspace; check `just web build` after an ala change.
- `knowledge/references/ala.md` has the full recipe; `libs/ala/UPSTREAM.md` maps upstream's paths to ala's for pulling an upstream fix. The `ghostty-sync` skill covers the vendored Ghostty.
</important>

<important if="a test fails under cargo test --workspace">

Rerun the named daemon test alone with `cargo test -p zz-daemon <test_name>`. If it passes
alone, keep the original failure in the results and check which concurrent setup or timing
condition differed. For `open terminal failed: not a terminal`, check the client kind and PTY
metadata in the attach request.
</important>

<important if="you are debugging a running daemon or checking the CLI">

- The daemon outlives the app: after installing a new build, existing sessions keep running the old daemon binary until it restarts. Don't chase "missing" behavior in a stale daemon.
- A packaged headless zz on Unix `exec`s the owned copy in `<data dir>/zz/owned/` when that copy is newer (`zz update --check` shows both), and on Windows starts it as the daemon; `ZZ_OWNED_COPY=0` turns that off, and panes never hand over.
- `just run` / `just watch` build zz Dev with `ZZ_DEV_BUILD=1`, clear inherited daemon/pane context, and use `zz-dev` config/data/socket paths. The macOS bundle is `dist/zz-dev/zz Dev.app` (`dev.zz.app.dev`); Linux launches `target/debug/zz-dev`. Installed and beta packages keep their existing behavior.
- `just web run` serves the dev client on port 8081. `just web build --release` builds with production identity. `just ios run [iPhone|iPad]` and `just android run` launch the shared thin client. See [running zz](knowledge/playbooks/running-zz.md) and the [browser client](knowledge/playbooks/browser-client.md) for endpoint selection and test rigs.
- `ZZ_SOCKET` overrides the socket the app dials. Unix socket paths have a low length cap (`sun_path`); put test sockets directly under `/tmp`.
- Recipes live in `knowledge/playbooks/running-zz.md`.
</important>

<important if="you are changing the wire protocol">

- `PROTOCOL_VERSION` (`crates/zz-protocol/src/message.rs`) is a wire epoch. Bump it only for a change an existing peer cannot decode or would get wrong: removing, reordering or reshaping a shipped message, variant or field, changing what one means, appending to a nested enum, or touching the terminal lane's packing.
- Never bump it to add a `ProtocolMessage` or `EventPayload` variant. Gate the variant with a capability instead; "Adding a message inside an epoch" in `knowledge/protocol/wire-protocol.md` has the steps for each direction.
- Bump `REVISION` (same file) by hand when behavior the other side relies on changes with no wire change, a fix included. Raise `MIN_DAEMON_REVISION` or `MIN_CLIENT_REVISION` only when an older peer would make a screen wrong.
- Changing the wire epoch prevents new clients from attaching to a daemon on the old epoch. Restarting that daemon ends the sessions and panes it owns.
</important>

<important if="you are changing an agent driver, or an agent CLI (claude, codex, agy, grok, cursor-agent) updated">

- The agent CLIs' vendors never promised to keep the interfaces zz drives, and some changes break a chat with no error. `knowledge/references/agent-cli-contracts.md` lists what zz relies on, the version each CLI was last checked at, and the commands that recheck it.
- When a CLI's `--version` differs from that page's table, run its cheap checks before trusting the driver, then update the table.
- When a change makes zz rely on a new vendor flag, frame, field, file, env var or string, add it to that page in the same change.
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

- Besides what the shared no-comments rule keeps, zz keeps the help headers `bench/run.sh`, `bench/inner.sh`, `bench/gen-fixtures.sh` and `compat/run.sh` print with `sed`, and module docstrings that argparse shows through `__doc__`.
