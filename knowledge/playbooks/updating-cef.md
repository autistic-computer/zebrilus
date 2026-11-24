---
type: Playbook
title: Updating the CEF pin
description: Update the Rust packages, generated bindings, distribution hashes and CI caches together, then check each native bundle.
resource: third_party/cef/ARTIFACTS.md
tags: [cef, upgrade, playbook, sha1, coordinated-change]
timestamp: 2026-11-24T00:00:00Z
---

1. Choose a cef-rs release and its full `cef-dll-sys` version, including the CEF build
   metadata. The adapter selects the archive on Linux x86_64, upstream selects it on other
   targets. Match both versions. Check that the release's accelerated texture importer can
   accept the same `wgpu::Device` as ala.
2. Update the exact `cef` constraints in the root and `libs/ala-cef/Cargo.toml`. Align
   wgpu constraints with ala if needed. In `libs/ala-cef/third_party/cef-dll-sys`, update
   the adapter version, upstream Git revision, binding SHA-256 and expected function count.
   Run `generate.py` on the pinned checkout's `sys/src/bindings/x86_64_unknown_linux_gnu.rs`,
   then repeat with `--check`. Its `UPSTREAM.md` describes the generated files and loading tests.
3. Refresh the locks for the root, standalone ala-cef and standalone adapter. Root and
   ala-cef resolve both a path adapter and a Git package named `cef-dll-sys`. From the repo root:

   ```sh
   adapter="path+file://$PWD/libs/ala-cef/third_party/cef-dll-sys"
   cargo update -p cef -p "$adapter"
   cargo update --manifest-path libs/ala-cef/Cargo.toml -p cef -p "$adapter"
   cargo update --manifest-path libs/ala-cef/third_party/cef-dll-sys/Cargo.toml --workspace
   ```

4. Look up the exact distribution in the [CEF build index](https://cef-builds.spotifycdn.com/index.json).
   Refresh `third_party/cef/ARTIFACTS.md` with the full version, lookup date, minimal archive
   names and SHA-1 for Linux x64/arm64/arm, macOS x64/arm64 and Windows x64/arm64/x86.
   The downloader verifies against the published index. The table records what was reviewed.
5. Find every download cache key with `rg 'key: cef-' .github/workflows`. Update the version
   segment in CI, nightly corpus and the three platform jobs in Release. Refresh the
   [CEF pin reference](/references/cef-artifacts.md).
6. Run the adapter's Linux loading tests, `just libs check ala-cef` and a native browser
   check using the [loopback fixture](/playbooks/running-zz.md#exercising-the-browser-pane-the-loopback-fixture).
   Run `cargo xtask bundle-cef --release --output target/cef-bundle` on Linux, macOS and
   Windows. Normal push/PR CI covers the first two. The scheduled and manually dispatched
   CI matrix includes Windows. Keep the pin, lockfiles, generated declarations, cache keys
   and verification results together. [Bundle verification](/playbooks/build-cef-bundle.md)
