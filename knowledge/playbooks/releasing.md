---
type: Playbook
title: Releasing zz
description: Ship a zz release from the Mac with just release ship, building every platform on our own machines through single-use self-hosted runners instead of hosted CI.
resource: scripts/release-runners.sh
tags: [release, runners, self-hosted, signing, notarize, homebrew, aur, playbook]
timestamp: 2026-11-15T00:00:00Z
---

# Overview

Releases build on our own machines, not on hosted runners. From the Mac:

```bash
just release ship 0.25.0
```

`ship` takes a version or a level (`patch`, `minor`). It runs `scripts/preflight.sh`, registers
four single-use runners, bumps the version, tags `vX.Y.Z` and pushes, then watches the Release
workflow until it publishes. On exit it stops the runners and removes them from GitHub, whether
the release passed or failed. v0.24.0 went from tag to published in 23.6 min, against 1h to 2h38m
on hosted runners.

# Runners

| Label | Machine |
|---|---|
| `zz-macos` | this Mac, under `~/.local/share/zz-release-runner` |
| `zz-linux-aarch64` | Docker (OrbStack) on this Mac |
| `zz-linux-x86_64` | Docker on alienware (`ZZ_RELEASE_LINUX` overrides) |
| `zz-windows` | win-desktop under `E:\zz-release-runner` (`ZZ_RELEASE_WINDOWS` overrides) |

The macOS, Linux and Windows jobs in `.github/workflows/release.yml` ask for these labels, so a
tag pushed without `ship` waits with no runner to pick it up. The draft, publish, homebrew and
aur jobs are small and stay on hosted `ubuntu-24.04`.

Each runner registers `--ephemeral`: it takes one job and is gone. The Linux containers keep
`zz-release-work` and `zz-release-cargo` volumes, so later builds reuse the cargo registry and
build dirs.

# Commands

| Command | What it does |
|---|---|
| `just release runners` | One-time setup: runner binaries on the Mac and win-desktop, the Docker image on both Linux hosts, Inno Setup on Windows |
| `just release mac setup` | Saves the `zz-notary` notarytool profile once |
| `just release dry-run` | Builds every leg through `workflow_dispatch` with no tag and no publish |
| `just release ship <version>` | Preflight, runners, bump, tag, push, watch |
| `just release stop` | Kills runners and deregisters any `zz-*` runner left on GitHub |

# Signing

The macOS leg signs with the login keychain's Developer ID and notarizes with the `zz-notary`
profile. `~/dev/.zz-signing` holds the notary API key, the Windows p12 and its password, and the
Homebrew tap and AUR deploy keys.

# Known traps

- cargo-release asks y/N; `ship` sets `ZZ_RELEASE_NO_CONFIRM=1`.
- The Windows runner hides `.runner` and `.credentials`, so removing them needs `-Force`.
- Checkout copies `~/.gitconfig`, a dotfiles symlink. On Windows that is fatal, so the runner
  gets its own home under `E:\zz-release-runner\home`.
- `daemon_autostart` tests in `zz-cli`'s `cli_binary` can hang under full-suite load on the Mac
  during preflight.
- If `ship` dies before its trap runs, `just release stop` clears the runners.
