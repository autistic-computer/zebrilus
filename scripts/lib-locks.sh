lib_manifests=(libs/penna/Cargo.toml libs/penna/storybook/Cargo.toml libs/ala-cef/Cargo.toml)
for lib_manifest in "${lib_manifests[@]}"; do
    if ! cargo metadata --locked --format-version 1 --manifest-path "$lib_manifest" >/dev/null; then
        echo "${lib_manifest%Cargo.toml}Cargo.lock does not match its manifest against ala from GitHub; refresh it without the libs/ala patch: cargo update --workspace --manifest-path $lib_manifest" >&2
        exit 1
    fi
done
lib_locks="$(mktemp -d)"
for lib_manifest in "${lib_manifests[@]}"; do
    lock="${lib_manifest%Cargo.toml}Cargo.lock"
    mkdir -p "$lib_locks/$(dirname "$lock")"
    cp "$lock" "$lib_locks/$lock"
done
trap 'for lib_manifest in "${lib_manifests[@]}"; do lock="${lib_manifest%Cargo.toml}Cargo.lock"; cp "$lib_locks/$lock" "$lock"; done; rm -rf "$lib_locks"' EXIT
