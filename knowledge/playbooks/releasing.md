---
type: Playbook
title: Releasing zz
description: Preview or ship a version, operate the four release runners and verify which assets were published.
resource: scripts/release-runners.sh
tags: [release, runners, self-hosted, signing, notarize, homebrew, aur, playbook]
timestamp: 2026-11-24T00:00:00Z
---

## Ship from the Mac

Use a clean `main` checkout. Compare installed agent CLI versions with the
[last checked versions](/references/agent-cli-contracts.md#last-checked) and run the contract
checks for any that moved. Preflight covers the repo's tests and builds.

| Command | Effect |
| --- | --- |
| `just release setup` | Install the pinned version of cargo-release. |
| `just release runners` | Install the Mac and Windows runner software and Inno Setup, build both Linux runner images. |
| `just release mac setup`, then `just release mac check` | Save the notary profile and check the Mac's identity and credentials. |
| `just release bump patch` | Preview the commit, annotated tag and push. Accepts `major`, `minor`, `patch`, `release`, `alpha`, `beta`, `rc` or an exact SemVer. |
| `just release dry-run` | Dispatch Release against remote `main`. Four runners build and package the current version, including macOS notarization. Release assets are uploaded only on a tag push. |
| `just release ship patch` | Run preflight on HEAD, bring four runners online, execute the bump, push the commit and tag, and watch the workflow to completion. |

`just release bump TARGET --execute` performs the commit, tag and push directly. The platform
jobs need registered runners for their labels. `ship` registers them before creating the tag
and removes them on exit. If interrupted, `just release stop` stops the workers and removes
registered runners whose names begin with `zz-`.

## Runners and signing

| Label | Host |
| --- | --- |
| `zz-macos` | This Mac, under `~/.local/share/zz-release-runner`. Developer ID is in the login keychain, the notary profile defaults to `zz-notary`. |
| `zz-linux-aarch64` | Docker on this Mac. |
| `zz-linux-x86_64` | Docker on `alienware`, configurable with `ZZ_RELEASE_LINUX`. |
| `zz-windows` | Native `win-desktop`, configurable with `ZZ_RELEASE_WINDOWS`. Uses a private runner home beneath `E:\zz-release-runner`. |

Each runner takes one job. The Linux containers reuse the `zz-release-work` and
`zz-release-cargo` volumes. If startup reports fewer than four online after five minutes,
check local Docker, SSH to both remote hosts and `target/release-runners/*.log`. At that
point `ship` has yet to tag anything.

To reproduce Mac signing locally, `just release mac build VERSION` requires the workspace
version and an unused output path. It writes `dist/zz-VERSION-macos-ARCH.dmg` and prints
the checksum. The release workflow saves that checksum in a file. The local recipe signs inside
out, enables Hardened Runtime, grants JIT to the GPU and Renderer helpers, signs the DMG,
submits it, staples the ticket and checks Gatekeeper. For individual stages use
`just release mac sign APP`, `notarize DMG` or `verify DMG` in that group.
`MACOS_SIGN_IDENTITY` selects a Developer ID identity and `MACOS_NOTARY_PROFILE` overrides
the default profile. `MACOS_NOTARY_TIMEOUT` changes the default 30-minute submission wait.
Windows signing runs when the workflow has the PFX secret.

## Check publication

`.github/workflows/release.yml` checks that the tag matches the manifest and creates a
draft release. Each platform uploads its versioned artifacts with SHA-256 files.

| Platform | Release artifacts |
| --- | --- |
| macOS arm64 | Notarized DMG and headless tarball |
| Linux x86_64 and aarch64 | Tarball, AppImage, Debian package and headless tarball per architecture |
| Windows x64 | ZIP, setup executable and headless tarball |

The `publish` job waits for all three platform jobs. Its condition requires a tag push,
successful macOS and Linux jobs, and a run that has not been cancelled. A failed Windows
job can therefore leave a public release without Windows assets. Inspect both the workflow
jobs and the release asset list when diagnosing a partial release.

Hosted jobs update Homebrew and AUR after publication. A hyphenated tag updates the beta
channels (`zz@beta`, `zz-beta-bin`), a stable tag updates stable and beta. The AUR push
requires its deploy key. To check the rendered cask locally, use
`scripts/render-cask.sh VERSION DMG_OR_SHA256 stable` or select `beta`.

The site's `install.sh` verifies each download against its checksum. It accepts `--beta`,
`--version VERSION`, `--headless` and `--prefix DIR`. On Apple Silicon it installs the
app in `/Applications` or `~/Applications` when the former is unwritable, and defers
Homebrew-managed installs to the cask. On Linux it uses apt where available with sufficient
privilege, otherwise the tarball under `~/.local`. An explicit prefix selects the tarball
for a Linux desktop install. Changes to the installer reach zzmux.sh through `just site`.
