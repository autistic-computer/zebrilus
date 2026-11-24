---
type: Reference
title: CEF artifact lock
description: Where the CEF/Chromium pin and its per-platform archive SHA-1 mirror live, and how the archive is actually verified before extraction.
resource: third_party/cef/ARTIFACTS.md
tags: [cef, chromium, pin, sha1, reference]
timestamp: 2026-11-24T00:00:00Z
---

# Overview

`Cargo.lock` pins the Rust CEF packages. The sys build uses `download-cef` for
fresh minimal distributions. It fetches the build index and verifies the archive's
SHA-1 before extraction. On Linux x86_64 the build script lives in the local
`libs/ala-cef/third_party/cef-dll-sys` adapter. Other targets use the upstream
sys build. `zz-xtask` assembles and checks the resulting bundle.

# The pin

`third_party/cef/ARTIFACTS.md` maps the crate, CEF and Chromium versions to the
archive name and SHA-1 for each target. The table records what reviewers
checked. The downloader obtains its expected digest from the CEF build index.
Refresh the mirror, lockfile, CI cache key and platform bundle checks together.

# Citations

- `third_party/cef/ARTIFACTS.md` records the archives and verification date.
- [CEF build index](https://cef-builds.spotifycdn.com/index.json)
- `libs/ala-cef/third_party/cef-dll-sys/build.rs` selects a fresh download or an existing distribution on Linux x86_64.

# Related

- [xtask crate](/crates/zz-xtask.md) . assembles and validates the bundle after the verified extraction
- [Build/verify a CEF bundle playbook](/playbooks/build-cef-bundle.md) . operational use
- [Updating CEF playbook](/playbooks/updating-cef.md) . coordinated bump procedure
- [CEF runtime concept](/browser/cef-runtime.md) . what the verified distribution provides at runtime
