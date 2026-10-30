import "settings.just"

set working-directory := "../.."

org := "git@github.com:autistic-computer"

[doc("Lint and test a lib (ala|penna|ala-cef) on its own, the way its repo's CI does, against the ala in libs/; penna includes its storybook.")]
check lib:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ lib }}" in
        ala) args=(--workspace) ;;
        penna|ala-cef) args=(--config "patch.\"https://github.com/autistic-computer/ala\".ala.path=\"$PWD/libs/ala/crates/ala\"") ;;
        *) echo "unknown lib: {{ lib }} (expected: ala|penna|ala-cef)" >&2; exit 2 ;;
    esac
    cargo fmt --manifest-path libs/{{ lib }}/Cargo.toml --all -- --check
    cargo clippy --manifest-path libs/{{ lib }}/Cargo.toml "${args[@]}" --all-targets --all-features -- -D warnings
    if [[ "{{ lib }}" == ala ]]; then
        cargo test --manifest-path libs/ala/Cargo.toml --workspace --all-features --exclude ala-platform
    else
        cargo test --manifest-path libs/{{ lib }}/Cargo.toml "${args[@]}" --all-features
    fi
    if [[ "{{ lib }}" == penna ]]; then
        args+=(--config "patch.\"https://github.com/autistic-computer/ala\".ala-platform.path=\"$PWD/libs/ala/crates/ala-platform\"")
        cargo fmt --manifest-path libs/penna/storybook/Cargo.toml -- --check
        cargo clippy --manifest-path libs/penna/storybook/Cargo.toml "${args[@]}" --all-targets --all-features -- -D warnings
        cargo clippy --manifest-path libs/penna/storybook/Cargo.toml "${args[@]}" --target wasm32-unknown-unknown --lib --features page -- -D warnings
        cargo test --manifest-path libs/penna/storybook/Cargo.toml "${args[@]}" --all-features
    fi

[doc("Push zz's commits under libs/<lib> to autistic-computer/<lib> main.")]
push lib:
    git subtree push --prefix=libs/{{ lib }} {{ org }}/{{ lib }}.git main

[doc("Merge autistic-computer/<lib> main into libs/<lib>.")]
pull lib:
    git subtree pull --prefix=libs/{{ lib }} {{ org }}/{{ lib }}.git main -m "Pull {{ lib }} from autistic-computer/{{ lib }}"
