---
type: Reference
title: proc-macro-error2 patch (retired)
description: Retired on 2026-09-12 after the dependency graph stopped using proc-macro-error2.
resource: Cargo.toml
tags:
- rust
- cargo
- dependencies
- proc-macro
timestamp: 2026-11-24T00:00:00Z
---

# Retirement

The September 2026 dependency cleanup removed the Cargo override and
`third_party/rust/proc-macro-error2/` snapshot. The old chain was
`ala → stacksafe → stacksafe-macro → proc-macro-error2`. In zz commit
`78df1544`, `stacksafe-macro` 1.0.3 switched to `proc-macro2`.

The standalone [ala](/references/ala.md) manifest still offers the optional
`stacker` feature backed by `stacksafe`. Its lockfile includes `stacksafe`
and `stacksafe-macro`. Neither the ala lockfile nor zz's root lockfile lists
`proc-macro-error2`.

# Historical provenance

zz vendored `proc-macro-error2` 2.0.1 to fix the
`pub_use_of_private_extern_crate` future-incompatibility warning on Rust 1.97.
The sole library change made `extern crate proc_macro` public with `#[doc(hidden)]`,
matching upstream PR `GnomedDev/proc-macro-error-2#14`, commit `53ff94b`.
The published package checksum was
`11ec05c52be0a07b08061f7dd003e7d7092e0472bc731b4af7bb1ef876109802`.
The former source, MIT and Apache licenses, and full patch remain in Git history.
