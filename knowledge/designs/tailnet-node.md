---
type: Design Plan
title: Tailnet hosts . zz uses the Tailscale you already run
description: Proposed plan for zz machines to find and attach to each other over Tailscale. When the machine already runs Tailscale and is signed in, zz uses it - no sign-in in zz, trust is "a machine signed in by the same Tailscale user" checked with whois, discovery comes from the peer list. A daemon-owned node built on the tailscale crate (tailscale-rs 0.7, unpatched) exists only for machines you explicitly switch to it. Connections are bridged into the daemon's own local socket, so the daemon loop and wire protocol do not change. Covers macOS, Linux, Windows and WSL2 hosts and which thin clients can use it in which phase.
status: Proposed
tags:
- remote
- multi-host
- tailscale
- discovery
- design-plan
timestamp: 2026-11-07T00:00:00-03:00
---

# Overview

> **Status: proposed 2026-11-07, revised after review the same day.** Nothing here is built.
> Checked against Tailscale 1.104 and the `tailscale` crate 0.7.1
> (github.com/tailscale/tailscale-rs at a2a56c4). The crate is used as published; zz carries no
> patches to it.

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
| `auto` (default) | uses the machine's Tailscale when it is running, signed in and has a network interface; otherwise no tailnet, and `zz tailnet status` says why |
| `own` | runs the daemon's own node with the `tailscale` crate, for boxes without Tailscale; you sign it in once |
| `off` | no tailnet |

`auto` never falls back to `own` by itself: a stopped or signed-out Tailscale is a choice zz does
not override. ssh hosts keep working unchanged.

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

This mode adds no dependency. zz talks to the Tailscale on the machine through its CLI, trying in
order and keeping the first that answers `status --json`:

| Platform | Candidates |
| --- | --- |
| macOS | `tailscale` on PATH (Homebrew or the app's installed CLI), then `/Applications/Tailscale.app/Contents/MacOS/Tailscale` run with `TAILSCALE_BE_CLI=1` so it acts as the CLI instead of opening the app |
| Linux | `tailscale` on PATH; normal users get read access to tailscaled's socket, which is all `status` and `whois` need |
| Windows | `tailscale` on PATH, then `%ProgramFiles%\Tailscale\tailscale.exe` |

Stderr is ignored (the CLI warns there when its version differs from the daemon's). Each failed
candidate is kept as a line for `zz tailnet status`.

## Is it usable

The daemon runs `status --json` at start and every 30 seconds. It uses Tailscale only when all hold:

- `BackendState` is `Running`;
- `TUN` is true (in userspace-networking mode there is no tailnet interface to listen on or dial
  through);
- `Self.UserID` is set and not zero.

The same output gives the tailnet addresses (`TailscaleIPs`), our user and login name, and the peers.

## Accepting

The daemon listens on TCP 9922 on each tailnet address, never on `0.0.0.0`, so the LAN cannot reach
it, and rebinds when the addresses change. Tailscale's access rules drop traffic they do not allow
before it reaches the socket. For each connection:

1. Refuse it if it comes from one of this machine's own tailnet addresses (local clients use the
   owner-only socket; without this rule another OS user here would pass as this machine's node).
2. Ask who it is: `whois --json <addr>`, with IPv6 written `[addr]:port`.
3. Run the [admission check](#who-may-attach). On a refusal or a whois error, close and log the
   peer's name and why.
4. Connect to the daemon's own local endpoint through `LocalTransport` (the Unix socket, or the named
   pipe on Windows) and copy bytes both ways.

Step 4 is what `zz proxy` does at the far end of ssh, in-process. The daemon sees an ordinary local
client, so the event loop, wire protocol and version check work as they do over ssh.

## Dialing

The client connects straight to the peer's tailnet address on 9922 with the OS network stack;
Tailscale routes it. `Endpoint` gains `Tailnet { peer, port }` parsed from `tailnet://`, and
`ClientStream` a `Tcp` arm. The name resolves through the peer list, so MagicDNS being off does not
matter on desktop. A `tailnet://` endpoint counts as remote (`EndpointFactsScope`), so the client
sends `CLIENT_REMOTE_CAPABILITY` and asks for the
[adaptive in-flight window](/designs/remote-flow-window.md) as over ssh. The `--host` resolver
(`fleet_hosts::resolve_fleet_host`, ssh-only today) accepts `tailnet://` too.

# Who may attach

| `tailnet-allow` | Running Tailscale | zz's own node |
| --- | --- | --- |
| `owner` (default) | whois returns a user ID that is set, not zero and equal to `Self.UserID`, and the caller has no tags | callers whose stable node ID is on the allow list |
| `members` | every caller whois identifies with a set user or tags | every caller the access rules let through |

Refused under `owner`: tagged nodes (Tailscale may still report a creator user for them, so the tag
check comes first), nodes shared in from another tailnet (their user is the sharer), whois failures,
and missing or zero IDs. `owner` is the default because an attached client can run any command as
you; on a work tailnet `members` lets coworkers attach if the access rules let them reach this
machine.

The check is a pure function of the caller's identity and ours, so its tests are a table of exact
cases.

# Discovery

From the peer list, a machine shows in the sidebar when it is online, owned by our user, untagged,
and a TCP dial to its port 9922 completes within one second with a zz hello in a protocol version we
speak. Probes run when the peer list changes and once a minute, a few at a time. The row's label is
the peer's hostname; its key is the stable node ID, so a rename keeps the row. A discovered host and
an `ssh://` host for the same machine are two rows; removing the discovered one hides it here
(`tailnet-hide = <stable id>`).

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
| `zz attach` (raw terminal) | `tailnet://` endpoint; `ClientStream::Tcp` hands its socket descriptor to the TUI's poll loop (`receive_fd`), like the local stream does | 1 |
| iOS, Android | the Tailscale app is the VPN; `clients/app` already connects through `Endpoint::parse`, so `tailnet://` with `ClientStream::Tcp` works there | 2 |
| phone bootstrap | QR code and the host list carry each machine's tailnet addresses and stable ID, not only its name, so MagicDNS being off on the phone does not matter | 3 |
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

Daemon-owned keys in `zz/config`, edited by Settings > Hosts:

| Key | Default | Meaning |
| --- | --- | --- |
| `tailnet` | `auto` | `auto`, `own` or `off`, see [Overview](#overview) |
| `tailnet-allow` | `owner` | see [Who may attach](#who-may-attach) |
| `tailnet-hostname` | `zz-<machine>` | `own` only |
| `tailnet-control-url` | Tailscale's | `own` only, such as Headscale |

The auth key is never a config key: it comes only from `zz tailnet login --auth-key-file`, is used for
the first registration, and is not stored.

# Phases

1. **Accept and dial with the running Tailscale.** CLI detection, the usability rules, the listener
   with the self-address rule and whois admission, the bridge, `Endpoint::Tailnet`,
   `ClientStream::Tcp` with its descriptor, the remote facts scope, `--host tailnet://`,
   `zz tailnet status`.
   *Done when* `zz --host tailnet://<machine> attach` from another of your machines attaches with the
   flow window on, and a machine signed in as another user, a tagged machine and a local OS user
   dialing the machine's own tailnet address are each refused with a logged reason. Run on macOS,
   Linux and Windows hosts.
2. **Discover, and phones.** Peer list probing, sidebar rows, hiding, the Settings group;
   `tailnet://` in the iOS and Android app.
   *Done when* starting zz on a third machine makes it appear on the other two within a minute with
   no config edit, and the iPhone attaches to one by address with the Tailscale app on.
3. **Phone bootstrap.** The hosts-from-first-machine request and the QR code, both carrying addresses
   and stable IDs.
   *Done when* the iPhone lists and attaches every discovered machine after one scan, with MagicDNS
   off.
4. **zz's own node.** `zz-tailnet`, `tailnet = own`, sign in and out, the allow list, `tailnet.sock`.
   *Done when* a Linux box with no Tailscale installed, signed in through zz, shows in the other
   machines' sidebars, attaches to them by name, and refuses a machine not on its allow list.

The web and HTML clients over the tailnet need their own design after phase 2.

# Tests

Few and exact:

- the admission table: same user, other user, tagged caller with a creator user, shared-in node,
  zero user, whois error, our own address, IPv6 caller, a stable ID on and off the allow list, under
  both policies, each with its expected verdict;
- mode choice from fixed `status --json` outputs: `Running` with and without `TUN`, `NeedsLogin`,
  `Stopped`, zero user, no working CLI candidate;
- discovery filtering from a fixed peer list: other user, tagged, offline, probe timeout and protocol
  mismatch each drop the peer, and a rename keeps the stable ID;
- allow-list enrollment: an ambiguous short name is refused, a full name stores the right stable ID.

Real tailnet traffic stays a manual acceptance step per phase, on two machines.

# Related

- [Fleet attach . one GUI, every daemon](/designs/fleet-attach.md) - the host list and `Endpoint` this extends
- [Fleet pairing and discovery](/designs/fleet-pairing.md) - the retired design whose goals this takes over
- [Adaptive in-flight window for remote attach](/designs/remote-flow-window.md) - why relayed links stay usable
- [Multi-device attach](/designs/multi-device-attach.md) - what several devices on one session look like
