---
type: Design Plan
title: Tailnet hosts . zz uses the Tailscale you already run
description: Plan, phases 1 to 3 built, for zz machines to find and attach to each other over Tailscale. When the machine already runs Tailscale and is signed in, zz uses it - no sign-in in zz, trust is "a machine of this tailnet's owner" checked with whois, discovery comes from the peer list. A daemon-owned node built on the tailscale crate (tailscale-rs 0.7, unpatched) exists only for machines you explicitly switch to it. Connections are bridged into the daemon's own local socket, so the daemon loop and wire protocol do not change. Covers macOS, Linux, Windows and WSL2 hosts and which thin clients can use it in which phase.
status: In progress
tags:
- remote
- multi-host
- tailscale
- discovery
- design-plan
timestamp: 2026-11-07T00:00:00-03:00
---

# Overview

> **Status: phases 1 to 3 built 2026-11-07.** Phase 3: a phone learns your other machines from the
> first one it reaches (`tailnet hosts`), and desktop Settings › Hosts shows a QR code with a
> `zz://host/` link to this machine. Verified with the iOS app in the simulator: started on
> alienware with nothing saved, it listed and saved macbook on its own.
>
> **Phase 2 built 2026-11-07.** Phase 2: the app finds your machines and lists
> them as hosts, Settings › Hosts has a Tailscale group, and the iOS and Android app connects to
> `tailnet://` through the Tailscale app. Verified with three machines: this Mac found alienware and
> xps, each Linux box found the other, and the iOS app in the simulator attached to alienware over
> `tailnet://` and typed into its shell.
>
> **Phase 1 built 2026-11-07.** The daemon listens through the running Tailscale, admits
> callers by whois, and `tailnet://` endpoints, `--host tailnet://...` and `zz tailnet status` work.
> Verified between a macOS and a CachyOS machine on a personal tailnet in both directions: commands,
> an interactive attach, `tailnet = off` dropping a live attach within one 30 s check, and the
> own-address refusal. Discovery, phones and zz's own node are not built. Checked against
> Tailscale 1.104 and the `tailscale` crate 0.7.1 (github.com/tailscale/tailscale-rs at a2a56c4),
> which zz uses as published, with no patches.

Today a remote host is an `ssh://` line ([fleet attach](/designs/fleet-attach.md)): every machine
needs sshd, a key, a host line and `zz` on the remote PATH, and nothing finds machines for you. The
2026-07-31 pairing design solved that with mDNS, QUIC and a short-code PAKE, and was deleted a day
later for being too much of zz's own network code ([fleet pairing](/designs/fleet-pairing.md)).

This plan uses Tailscale's identity instead. Tailscale gives each machine an address your other
devices reach through NAT, encrypts the traffic, and says which machine and which Tailscale user is
on the other end of a connection. Trust becomes "a machine signed in by the same Tailscale user", and
discovery becomes "your machines that answer on the zz port".

| `tailnet` | zz does |
| --- | --- |
| `auto` (default) | the daemon on the platform's default socket uses the machine's Tailscale when it is running, signed in and has a network interface; otherwise no tailnet, and `zz tailnet status` says why. Daemons on other sockets (tests, scratch daemons, a second daemon) stay off the tailnet |
| `on` | as `auto`, from whichever socket this daemon serves |
| `own` (phase 4) | runs the daemon's own node with the `tailscale` crate, for boxes without Tailscale; you sign it in once |
| `off` | no tailnet; a daemon that was listening stops and drops its tailnet clients at its next 30 s check |

`auto` never falls back to `own` by itself: a stopped or signed-out Tailscale is a choice zz does
not override. ssh hosts keep working unchanged. The port is 9922, and 9923 for zz Dev, so both can
run on one machine.

# What the user sees

1. On a machine signed in to Tailscale: nothing to do. Settings > Hosts says
   "Using Tailscale, signed in as you@example.com".
2. Your other machines running zz appear in the sidebar next to the ssh hosts within a minute and
   attach when clicked. Nothing is written to config.
3. On a box without Tailscale, Settings > Hosts offers **Use zz's own Tailscale connection**, which
   sets `tailnet = own` and signs in (`zz tailnet login --auth-key-file FILE` on a headless box).
4. On a phone with the Tailscale app on, scan the QR code desktop Settings shows, or type one
   machine's address; that machine tells the phone about the rest.

`tailnet://NAME` works anywhere an endpoint does (`host-arch = tailnet://arch-desktop`,
`zz --host tailnet://arch-desktop attach`), and `zz tailnet status` prints the mode, the account,
the addresses, and the reason when it is not working.

# Trust model

Admission identifies the machine, not the OS account on it. Anyone who can open a TCP connection
from one of your Tailscale machines can attach as you, the same way they could use that machine's
Tailscale SSH or any other tailnet service you reach from it. On your own computers that is you. On a
computer other people log in to, keep `tailnet = off` and use ssh, which authenticates the OS account.

The self-address rule below keeps other OS users of the target machine out; nothing in zz can tell
OS users apart on the calling machine.

# Using the running Tailscale

This mode adds no dependency (`crates/zz-daemon-client/src/tailnet.rs`). zz talks to the Tailscale
on the machine through its CLI, trying each candidate in order with a 5 s limit:

| Platform | Candidates |
| --- | --- |
| macOS | `tailscale` on PATH, then `/opt/homebrew/bin/tailscale` and `/usr/local/bin/tailscale` (a daemon the app spawns has a short PATH), then `/Applications/Tailscale.app/Contents/MacOS/Tailscale` run with `TAILSCALE_BE_CLI=1` so it acts as the CLI instead of opening the app |
| Linux | `tailscale` on PATH, then `/usr/bin`, `/usr/local/bin` and `/snap/bin`; normal users get read access to tailscaled's socket, which is all `status` and `whois` need |
| Windows | `tailscale` on PATH, then `%ProgramFiles%\Tailscale\tailscale.exe` |

Stderr is ignored (the CLI warns there when its version differs from the daemon's). When no
candidate answers, the error lists each one and why.

## Is it usable

The daemon (`crates/zz-daemon/src/tailnet.rs`) reads `tailnet` and `tailnet-allow` from `zz/config`
and runs `status --json` at start and every 30 seconds, logging only when the outcome changes. It
uses Tailscale only when all hold:

- `BackendState` is `Running`;
- `TUN` is true (in userspace-networking mode there is no tailnet interface to listen on or dial
  through);
- `Self.UserID` is set and not zero.

The same output gives the tailnet addresses (`TailscaleIPs`), our user and login name, and the peers.

## Accepting

The daemon listens on TCP 9922 (9923 for zz Dev) on each tailnet address, never on `0.0.0.0`, so the
LAN cannot reach it, and rebinds when the addresses change. Tailscale's access rules drop traffic they do not allow
before it reaches the socket. For each connection:

1. Refuse it if it comes from one of this machine's own tailnet addresses (local clients use the
   owner-only socket; without this rule another OS user here would pass as this machine's node).
2. Ask who it is: `whois --json <addr>` (an IPv4-mapped IPv6 caller is checked as its IPv4 address).
3. Run the [admission check](#who-may-attach). On a refusal or a whois error, close and log the
   peer's name and why.
4. Connect to the daemon's own local endpoint through `LocalTransport` (the Unix socket, or the named
   pipe on Windows) and copy bytes both ways.

Step 4 is what `zz proxy` does at the far end of ssh, in-process. The daemon sees an ordinary local
client, so the event loop, wire protocol and version check work as they do over ssh. Each admitted
connection is logged with the caller's machine and login. When listening stops (`tailnet = off`,
Tailscale stopped, an address gone), the daemon also shuts down the tailnet connections it bridged,
so turning it off ends live attaches too.

## Dialing

`Endpoint` has a `Tailnet { peer, port }` arm parsed from `tailnet://NAME[:port]` (a name, a full
MagicDNS name, or an address, IPv6 in brackets). The name resolves through the peer list, so
MagicDNS being off does not matter on desktop. Connecting dials the peer's address with the OS
network stack (Tailscale routes it) and fails at once with a plain reason when nothing answers.

On desktop the client then works the way it does over ssh: `TailnetForward` serves a private local
socket and pipes each connection made to it over TCP to the peer, reusing the first dial. Everything
after that is the existing local-socket path, so command clients reconnecting per command and the
raw-terminal client polling its descriptor need nothing new. A `tailnet://` endpoint uses the
remote facts scope (`PortableTerminalSize`), so the client sends `CLIENT_REMOTE_CAPABILITY` and asks
for the [adaptive in-flight window](/designs/remote-flow-window.md) as over ssh. A peer that closes
the connection before the hello is reported as refusing this machine, with a pointer to
`zz tailnet status` there. The `--host` resolver accepts `tailnet://`; the verbs that run on a host
over ssh (`fleet`, `list-hosts`, `tools`) refuse a tailnet host.

# Who may attach

| `tailnet-allow` | Running Tailscale | zz's own node |
| --- | --- | --- |
| `owner` (default), personal tailnet | the caller is tagged (a tagged device of this tailnet), or its login is the tailnet's owner | callers whose stable node ID is on the allow list |
| `owner`, organization tailnet | this machine is untagged, and the caller is untagged with the same user ID | as above |
| `members` | every caller whois identifies with a user or tags | every caller the access rules let through |

A personal tailnet is one named after a login (`CurrentTailnet.Name` holds an `@`, such as
`you@gmail.com`); everything in it is the owner's, and it is common to tag every server and desktop
(this design was first checked on a tailnet where every desktop is `tag:box` and only the phones
belong to the login). An organization tailnet is named after a domain, and there a tag says nothing
about who owns the device, so `owner` admits only untagged devices of the same user, and a tagged
machine admits nobody until `tailnet-allow = members`.

Refused under `owner`: other users invited into a personal tailnet, nodes shared in from another
tailnet (sharing strips tags and their login is the sharer's), whois failures, missing or zero IDs,
and anything from this machine's own addresses. `owner` is the default because an attached client
can run any command as you.

The check is a pure function of the caller's identity and ours, so its tests are a table of exact
cases.

# Discovery

The app scans every 30 seconds off the UI thread (`crates/zz/src/tailnet.rs`). From the peer list, a
machine is a candidate when it is online and this machine would admit it under `owner`
(`tailnet::mine`, the same rule as [Who may attach](#who-may-attach)), whatever `tailnet-allow`
says, so a work tailnet is never swept. All candidates are dialed at once on the zz port with a
one-second limit (`tailnet::discover`); a machine that answers becomes a host named by the first
label of its MagicDNS name, with a `tailnet://` endpoint holding the full name. A probe is a plain
connect, so the far daemon logs it as an admitted connection.

Found machines go after the host lines (`config::known_hosts`), skipping one whose name, or whose
tailnet machine, a line already has. The registry, auto-connect and both sidebars take the merged
list unchanged. **Remove host** on a found machine adds its name to `tailnet-hide` instead of editing
host lines, and Settings › Hosts lists hidden machines with a Show button. `zz tailnet status` prints
the same list.

# Platforms

| Host | `auto` | `own` |
| --- | --- | --- |
| macOS | works with the App Store, standalone and Homebrew Tailscale. With the application firewall on, macOS asks once to accept incoming connections for the daemon | arm64 only |
| Linux | works with kernel networking. A host firewall that drops inbound (`ufw` default) needs `ufw allow in on tailscale0`, which Tailscale's own docs recommend; `zz tailnet status` prints it when no peer has ever connected. Userspace-networking tailscaled is reported as unusable | x86_64 and arm64 |
| Windows | works with the Windows client. The bridge goes into the daemon's named pipe. The first listen raises a Windows Firewall prompt; the installer adds a rule for the daemon scoped to the Tailscale interface | not supported in phase 4 |
| WSL2 | Tailscale running inside the distro works as on Linux. Tailscale on the Windows side does not reach a daemon in WSL's default NAT network; that case is reported, not bridged | works as on Linux, the simplest setup for WSL |

# Thin clients

| Client | How | From phase |
| --- | --- | --- |
| `zz attach` (raw terminal) and command clients | `tailnet://` endpoint through the local forward socket | 1, built |
| iOS, Android | the Tailscale app is the VPN; `clients/app` connects through `Endpoint::parse`, and `tailnet://` there resolves with the OS (MagicDNS from the app) and dials a `ClientStream::Tcp`. Type the machine's full MagicDNS name or address in Add host | 2, built |
| phone bootstrap | a `tailnet://` connection asks its host `tailnet hosts`, gets the machines that host would admit with their addresses and port, dials each from the phone with a one-second limit, and saves and connects the ones that answer, by MagicDNS name. Desktop Settings › Hosts shows a QR code of `zz://host/<machine>.<tailnet>.ts.net:<port>`; the phone's camera opens it in zz, which adds and connects the machine | 3, built |
| web (WASM) and the HTML client | a browser cannot open raw TCP, and the page needs HTTPS for WebGPU and the cross-origin isolation headers zz-web sends. This needs an HTTPS gateway (`tailscale serve` in front of zz-web on loopback, checking the identity header Serve adds) and its own design | later |

Since 2026-11-07 zz-web serves nothing without a per-start token carried in the link it prints and
kept as a cookie (`crates/zz-web/src/lib.rs`); before that, any local process could forge the `Host`
and `Origin` headers it checked. A tailnet gateway would sit in front of that, not replace it.

# zz's own node

Only with `tailnet = own`. The daemon starts a `tailscale::Device` in a new crate, `crates/zz-tailnet`,
which runs one Tokio runtime on its own thread and hands the synchronous daemon a small handle:
status, dial, accept. The daemon depends on it behind a `tailnet-own` Cargo feature, on for Linux and
arm64 macOS desktop builds. Keys live in `<data dir>/tailnet/keys.json` (`0600`, in a `0700`
directory). The node is named `zz-<machine>`.

- **Sign in.** `Device::is_authorized` returns a login URL for a new node; Settings shows it, and the
  crate polls until the login completes.
- **Sign out.** No logout call in the crate: shut the node down and delete the keys. Settings links to
  the admin console to remove the device.
- **Accepting.** The node listens on 9922 in its own userspace network stack and bridges the same way.
- **Dialing.** The OS has no tailnet route here, so clients ask the daemon to dial: they write the
  peer to a second owner-only endpoint, `tailnet.sock`, and the daemon dials through the node and pipes
  the bytes.

What the 0.7.1 crate does not offer, and what zz does instead of patching it:

| Missing | zz does |
| --- | --- |
| which user owns a peer | admit by allow list. `zz tailnet allow NAME` takes a full MagicDNS name or a stable ID, refuses a short name that matches more than one node (the crate keeps only the most recent node for a duplicated short name), and stores the stable ID; on accept, `peer_by_tailnet_ip` gives the caller's stable ID to compare |
| a list of every peer | no discovery from this box; other machines still find it through their own Tailscale's peer list. Hosts here are added by name |
| logout, runtime rename | delete keys to sign out; restart the node to rename |
| MagicDNS, HTTPS certificates, Serve | names resolve with `peer_by_name`; no web client on this box |
| iOS, Android | phones use the Tailscale app |

The crate's own docs still carry an early "unstable" warning that predates its 0.6.0 audit release.
`own` stays opt-in for that reason as well as for the missing owner check. Pin an exact version.
Measure the release `zz` binary and the daemon's idle memory with the feature on and off before
landing it, and record both here.

# Settings

Keys in `zz/config`, which the daemon reads itself (Settings > Hosts will edit them; for now edit the
file). An invalid value is a config diagnostic and leaves the default:

| Key | Default | Meaning |
| --- | --- | --- |
| `tailnet` | `auto` | `auto`, `on`, `off`, or `own` once phase 4 lands; see [Overview](#overview) |
| `tailnet-allow` | `owner` | see [Who may attach](#who-may-attach) |
| `tailnet-hostname` | `zz-<machine>` | `own` only |
| `tailnet-control-url` | Tailscale's | `own` only, such as Headscale |

The auth key is never a config key: it comes only from `zz tailnet login --auth-key-file`, is used for
the first registration, and is not stored.

# Phases

1. **Accept and dial with the running Tailscale. Built 2026-11-07.** CLI detection, the usability
   rules, the listener with the self-address rule and whois admission, the bridge, dropping bridged
   connections when listening stops, `Endpoint::Tailnet` through a local forward socket, the remote
   facts scope, `--host tailnet://`, `zz tailnet status`.
   *Checked:* macOS and CachyOS, each way, on a personal tailnet with both machines tagged:
   `list-sessions`, `send-keys` and `capture-pane` over `--host tailnet://...`, an interactive
   `attach` typing into the remote shell, the own-address connection closed, `tailnet = off`
   dropping a live attach after 19 s and `on` serving again after 23 s, and plain errors for an
   unknown peer and a closed port. *Not yet checked:* a Windows host, a caller from another user
   (only the admission table covers it), and seeing the flow window negotiated (the tailnet arm
   sends the same remote facts as ssh).
2. **Discover, and phones. Built 2026-11-07.** Peer list probing, the merged host list, hiding, the
   Settings group; `tailnet://` in the iOS and Android app.
   *Checked:* with scratch daemons on alienware and xps, `zz tailnet status` on this Mac listed both
   (2.4 s with the probes) and each Linux box listed the other; the iOS app in the simulator attached
   to `tailnet://alienware.<tailnet>.ts.net` and a command typed on it ran there. A test pins the
   merge: a host line wins over a found machine with the same name or address. *Not yet checked:*
   the desktop sidebar drawing found machines on screen (the test reads the same list the sidebar
   does), an Android device, and a real iPhone, which joins as its own Tailscale device.
3. **Phone bootstrap. Built 2026-11-07.** The daemon answers `tailnet hosts` with JSON for the
   machines it would admit, from the status its 30-second check already holds, so asking costs no
   probes (`crates/zz-daemon/src/tailnet.rs`, `tailnet_offer::offers_json`); the daemon does not open
   that output as a view the way it does for other commands an attached client runs. The app asks
   once per `tailnet://` connection, primary or background, probes the answers itself
   (`tailnet_offer::reachable`) and adds what answers (`clients/app`). `zz://host/<name>:<port>` links
   add and connect a machine, and Settings › Hosts draws the QR code with `qrcodegen`.
   *Checked:* the iOS app in the simulator, started on alienware with no saved hosts, saved
   `tailnet://macbook...` from alienware's answer and listed both on its Hosts page. The macbook card
   stays Not connected there because the simulator dials from the Mac's own tailnet address, which
   the Mac refuses. *Not yet checked:* a real iPhone scanning the QR code (in the simulator two
   installed apps claim `zz://`, and iOS handed the link to the other one), Android (no NDK toolchain
   on this Mac; the code is shared with iOS), and the QR code on screen. *Changed from the plan:*
   saved hosts use the MagicDNS name so they read well, so a phone with MagicDNS off can learn
   machines but not reconnect to them.
4. **zz's own node.** `zz-tailnet`, `tailnet = own`, sign in and out, the allow list, `tailnet.sock`.
   *Done when* a Linux box with no Tailscale installed, signed in through zz, shows in the other
   machines' sidebars, attaches to them by name, and refuses a machine not on its allow list.

The web and HTML clients over the tailnet need their own design after phase 2.

# Tests

Few and exact:

- the admission table (built): tagged box and the owner's phone on a personal tailnet, an invited
  user, our own address, no identity, same user, coworker, tagged caller and tagged self on an
  organization tailnet, and `members`, each with its exact verdict; the allow-list cases join it
  in phase 4;
- status and whois parsing from fixed JSON, and the usability reasons for `NeedsLogin`, no `TUN` and
  no account (built);
- settings from config text, and `tailnet://` parsing and printing (built);
- discovery filtering from a fixed peer list: other user, tagged, offline, probe timeout and protocol
  mismatch each drop the peer, and a rename keeps the stable ID;
- allow-list enrollment: an ambiguous short name is refused, a full name stores the right stable ID.

Real tailnet traffic stays a manual acceptance step per phase, on two machines.

# Related

- [Fleet attach . one GUI, every daemon](/designs/fleet-attach.md) - the host list and `Endpoint` this extends
- [Fleet pairing and discovery](/designs/fleet-pairing.md) - the retired design whose goals this takes over
- [Adaptive in-flight window for remote attach](/designs/remote-flow-window.md) - why relayed links stay usable
- [Multi-device attach](/designs/multi-device-attach.md) - what several devices on one session look like
