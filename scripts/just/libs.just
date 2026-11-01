import "settings.just"

set working-directory := "../.."

org := "git@github.com:autistic-computer"

[doc("Lint and test a lib (ala|penna|ala-cef|pele) on its own, the way its repo's CI does, against the ala in libs/; penna includes its storybook; penna and ala-cef check their lockfiles against ala from GitHub and leave them as they were; pele checks its lockfile and lints for wasm.")]
check lib:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ lib }}" in
        ala) args=(--workspace) ;;
        penna|ala-cef) args=(--config "patch.\"https://github.com/autistic-computer/ala\".ala.path=\"$PWD/libs/ala/crates/ala\"") ;;
        pele) args=(--locked) ;;
        *) echo "unknown lib: {{ lib }} (expected: ala|penna|ala-cef|pele)" >&2; exit 2 ;;
    esac
    if [[ "{{ lib }}" == penna || "{{ lib }}" == ala-cef ]]; then
        source scripts/lib-locks.sh
    fi
    cargo fmt --manifest-path libs/{{ lib }}/Cargo.toml --all -- --check
    cargo clippy --manifest-path libs/{{ lib }}/Cargo.toml "${args[@]}" --all-targets --all-features -- -D warnings
    if [[ "{{ lib }}" == ala ]]; then
        cargo test --manifest-path libs/ala/Cargo.toml --workspace --all-features --exclude ala-platform
    else
        cargo test --manifest-path libs/{{ lib }}/Cargo.toml "${args[@]}" --all-features
    fi
    if [[ "{{ lib }}" == pele ]]; then
        cargo clippy --manifest-path libs/pele/Cargo.toml --locked --lib --target wasm32-unknown-unknown -- -D warnings
    fi
    if [[ "{{ lib }}" == penna ]]; then
        cargo clippy --manifest-path libs/penna/Cargo.toml "${args[@]}" --lib --no-default-features -- -D warnings
        cargo test --manifest-path libs/penna/Cargo.toml "${args[@]}" --test highlighter_api
        args+=(--config "patch.\"https://github.com/autistic-computer/ala\".ala-platform.path=\"$PWD/libs/ala/crates/ala-platform\"")
        cargo fmt --manifest-path libs/penna/storybook/Cargo.toml -- --check
        cargo clippy --manifest-path libs/penna/storybook/Cargo.toml "${args[@]}" --all-targets --all-features -- -D warnings
        cargo clippy --manifest-path libs/penna/storybook/Cargo.toml "${args[@]}" --target wasm32-unknown-unknown --lib --features page -- -D warnings
        cargo test --manifest-path libs/penna/storybook/Cargo.toml "${args[@]}" --all-features
    fi

[doc("Push zz's commits under libs/<lib> to autistic-computer/<lib> main (pele goes to pele-rs).")]
push lib:
    git subtree push --prefix=libs/{{ lib }} {{ org }}/{{ if lib == "pele" { "pele-rs" } else { lib } }}.git main

[doc("Merge autistic-computer/<lib> main into libs/<lib> (pele comes from pele-rs).")]
pull lib:
    git subtree pull --prefix=libs/{{ lib }} {{ org }}/{{ if lib == "pele" { "pele-rs" } else { lib } }}.git main -m "Pull {{ lib }} from autistic-computer/{{ if lib == "pele" { "pele-rs" } else { lib } }}"
