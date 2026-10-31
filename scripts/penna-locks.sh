for penna_manifest in libs/penna/Cargo.toml libs/penna/storybook/Cargo.toml; do
    if ! cargo metadata --locked --format-version 1 --manifest-path "$penna_manifest" >/dev/null; then
        echo "${penna_manifest%Cargo.toml}Cargo.lock does not match its manifest against ala from GitHub; refresh it without the libs/ala patch: cargo update --workspace --manifest-path $penna_manifest" >&2
        exit 1
    fi
done
penna_locks="$(mktemp -d)"
cp libs/penna/Cargo.lock "$penna_locks/penna"
cp libs/penna/storybook/Cargo.lock "$penna_locks/storybook"
trap 'cp "$penna_locks/penna" libs/penna/Cargo.lock; cp "$penna_locks/storybook" libs/penna/storybook/Cargo.lock; rm -rf "$penna_locks"' EXIT
