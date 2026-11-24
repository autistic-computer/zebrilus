---
type: Design Plan
title: "Direct device pairing (retired)"
description: "The short-code pairing service and its trust registry were retired on 2026-08-01."
status: "Retired 2026-08-01"
resource: crates/zz-daemon-client/src/fleet_hosts.rs
tags:
- remote
- multi-host
- pairing
- discovery
- quic
- notifications
- sidebar
- design-plan
timestamp: 2026-11-24T00:00:00-03:00
---

The July 2026 service used mDNS discovery, a one-time code and mutual QUIC trust. Its private registry and firewall knock were removed. [Fleet attach](/designs/fleet-attach.md) describes the host model. [Tailnet nodes](/designs/tailnet-node.md) use the local Tailscale service to discover peers.
