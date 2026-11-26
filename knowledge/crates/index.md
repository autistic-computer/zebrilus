<!-- okf:listing:start (managed by okf.py index — edit prose outside this fence) -->
# Concepts

* [ala-cef crate](ala-cef.md) - CEF off-screen browser hosting, cross-platform input and frame types, profiles, and optional Chrome data import.
* [zz-agent crate](zz-agent.md) - The agent runtime the daemon drives agent panes with, its Claude Code, Codex and ACP drivers, host, fanout, journal and logins.
* [zz-cli crate](zz-cli.md) - Headless executable, CLI dispatch and daemon startup shared with the desktop app, remote host targeting and owned updates.
* [zz-client crate](zz-client.md) - Renderer-independent client state, input routing, scrollback, layout and presentation rules shared by zz clients.
* [zz-daemon-client crate](zz-daemon-client.md) - Command and interactive connections, local and remote routes, host discovery, identity and process facts shared with the server.
* [zz-daemon crate](zz-daemon.md) - Server ownership of mux state, PTYs, agent runtimes, connection dispatch and per-client publication.
* [zz-mux crate](zz-mux.md) - The mux model, cell layout, tmux command execution, target resolution, config parser, formats and change journal.
* [zz-protocol crate](zz-protocol.md) - Shared Rust types and codecs for daemon messages, terminal frames, layout, agent chat, command discovery and keys.
* [zz-terminal crate](zz-terminal.md) - Terminal session actors, PTY and libghostty-vt ownership, and the renderer-independent viewport, input and appearance types.
* [zz-xtask crate](zz-xtask.md) - Build and validation commands for desktop CEF bundles on Linux, macOS and Windows.
* [zz desktop crate](zz.md) - Native desktop startup, daemon connections, workspace reconciliation, terminal painting, browser sessions and agent chat.
<!-- okf:listing:end -->

Workspace members without a crate page here: `ala` and the `ala-*` crates (our ala, see
[ala](/references/ala.md)), `penna` (widget kit, a maintained gpui-component fork, see [penna](/references/penna.md)), `ala-cef` (has a page here),
`pele` (Mermaid to SVG, in `libs/pele`, which the agent chat draws its diagrams with),
`zz-ui` (zz's application UI), `zz-tui` (the library behind `zz attach`), `zz-app` (the thin-client app
web, iOS and Android share, in `clients/app`), `zz-ios` (the iOS entry point, in `clients/ios`), and
`zz-android` (the Android entry point, in `clients/android`).
`zz-web` serves the [browser client](/playbooks/browser-client.md), whose ala/WASM frontend lives
in the separate `clients/web` workspace.
The map in
[architecture/overview](/architecture/overview.md) uses the live `zz-*` names.
