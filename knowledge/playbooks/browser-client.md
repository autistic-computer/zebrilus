---
type: Playbook
title: Running the browser client
description: Build the shared ala browser client and connect it, through the local WebSocket gateway, to the gateway machine's daemon or any host its zz config lists.
resource: crates/zz-web/src/main.rs
tags: [browser, wasm, client, websocket, ala]
timestamp: 2026-11-24T00:00:00Z
---

## Run

Start a development daemon with `just run mac` or `just run linux`. In another shell:

```sh
just web setup
just web run
```

Open the token link printed by the gateway. The dev recipe builds into
`clients/web/dist-dev`, serves on `127.0.0.1:8081` and watches shared and client sources.
Reload after a rebuild. For a one-off build or a specific socket:

```sh
just web build
just web serve --bind 127.0.0.1:8081 --socket /tmp/zz-web-dev.sock
```

Replace the socket argument with the existing daemon's path or omit it for the dev default.
The dev recipes clear inherited pane and socket context. `just web setup` installs nightly,
the wasm32 target, cargo-watch and the pinned wasm-bindgen CLI. Syntax highlighting needs
`llvm-ar` (`brew install llvm` on macOS), or set `WEB_SYNTAX_HIGHLIGHTING=0`.
For release assets use `just web build --release`, then from a shell targeting the installed
daemon run `ZZ_DEV_BUILD=0 cargo run -p zz-web -- --assets clients/web/dist` (port 8080).
The defaults live in `crates/zz-web/src/main.rs`.

## Remote access

The gateway accepts loopback binds. To reach a remote dev gateway over SSH, keep the
local and remote ports equal so Host and Origin match:

```sh
ssh -N -L 8081:127.0.0.1:8081 user@host
```

Use 8080 at both ends for a default release gateway. Open the printed token URL locally.
The redirect exchanges the token for an HttpOnly, SameSite=Strict, port-named cookie.
Assets and WebSockets require the cookie. Host changes and WebSocket upgrades also require
the matching origin. After restarting the gateway, open its newly printed link.

## Hosts

The gateway reads its configured hosts at startup. Change the gateway machine's
`zz/config` (or `zz-dev/config`), then restart to pick up configuration edits. The
client keeps hosts added through **Settings > Hosts** in local storage. Remove those
hosts from the same page. Each connected machine has a WebSocket. The selected machine
gets an attachment, background machines keep the navigation tree current.
[Multi-host attachment](/designs/fleet-attach.md)

| Request | Result |
| --- | --- |
| `GET /hosts` | Returns this computer first, then configured entries. An older gateway without this endpoint still serves a single local connection. |
| `POST /hosts` with a plain-text SSH destination | Returns the validated host row for the page to save, or 422 with a reason. The gateway leaves its config on disk untouched. |
| `/ws`, `/ws?host=local` | Connects to the gateway machine's daemon. |
| `/ws?host=NAME` | Connects to the configured Unix socket, tailnet or SSH route. A valid SSH endpoint can also be supplied directly. Unknown names return 404, malformed SSH endpoints and duplicate host parameters return 400. |

SSH runs on the gateway using its user's identities. Password, host-key and agent prompts
travel over the host's authenticated WebSocket. The gateway queues up to 64 binary frames
during sign-in, including the client's hello. The browser shows the sign-in prompt and
sends the answer or cancellation on that connection. `crates/zz-web/src/lib.rs` implements
the relay, and `clients/app/src/connection.rs` defines `retry_after` and `page_out_of_date`.

| Symptom | Action |
| --- | --- |
| 4401 close | Sign-in was refused or cancelled. Read the reason and use **Connect** to retry. |
| 4502 close | Check the host, SSH route, remote binary and protocol. A failed first connection waits for **Connect**. Following a previously successful connection the page retries, backing off to 60 s on repeated 4502 closes. |
| Local connection dropped | The page retries every 2 s. **Reload page** means the local daemon is ahead of the page. Reload to fetch the gateway's current client. |
| Remote host is newer than the page | Align the gateway and remote versions. Reloading the same gateway's client cannot change its protocol. |

## Browser behavior

`clients/web/src/lib.rs` launches the same `clients/app` shell used by iOS and Android
and registers Plumula and Lilex. The daemon owns terminals, agents, sessions and files.
The browser draws terminal frames and images, handles input and clipboard pastes, replays
agent state, and persists appearance and host choices in its own local storage. ala attempts
WebGPU, then WebGL2. The WebGL2 fallback supports one ala window.

Browser panes offer **Open in browser tab**. Native Chromium and its pane screenshot commands
require a desktop client. Editor panes can load and save a host file via `file-edit-v1`
(up to 2 MiB of text, writes in 256 KiB chunks). The editor waits for a successful read
before accepting edits. Paths in daemon commands such as `source-file` resolve on the host.

For clipboard or preview regressions, paste an image into a terminal and hover its
`[Image #N]` marker. The daemon stores the image and pastes its path. On hover the client
fetches the preview immediately and shows it after 250 ms if the pointer stays. The encoded
preview cache holds up to 512 entries and 64 MiB.
Terminals and agent composers share `zz_ui::attachment::normalize`. PNG, JPEG, GIF and
WebP within 5 MiB and 1568 pixels on the long edge pass through. Larger images are
re-encoded, with JPEG kept as JPEG. BMP and TIFF become PNG. The client rejects images
still over 5 MiB after encoding.

## Checks

```sh
cargo test -p zz-web --locked
cargo clippy -p zz-web --all-targets --locked -- -D warnings
cargo test --manifest-path clients/web/Cargo.toml --lib --locked
scripts/ci.sh web
just web build
```

`scripts/ci.sh web` checks formatting and the standalone client's locked nightly WASM build.
The gateway tests in `crates/zz-web/tests/gateway.rs` exercise asset access, Origin checks,
host routing and a real daemon attachment. For a manual two-host check, run two throwaway
daemons with short sockets under `/tmp`, put `host-second = unix:///tmp/second.sock` in a
scratch `$XDG_CONFIG_HOME/zz-dev/config`, and launch `just web serve --socket /tmp/first.sock`
with that configuration. Open **second** and confirm the sessions belong to that daemon.
