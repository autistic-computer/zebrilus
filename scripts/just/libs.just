import "settings.just"

set working-directory := "../.."

org := "git@github.com:autistic-computer"

# Lint and test a lib (ala|penna) on its own, the way its repo's CI does, against the ala in libs/.
check lib:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ lib }}" in
        ala) args=(--workspace) ;;
        penna) args=(--config "patch.\"https://github.com/autistic-computer/ala\".ala.path=\"$PWD/libs/ala/crates/ala\"") ;;
        *) echo "unknown lib: {{ lib }} (expected: ala|penna)" >&2; exit 2 ;;
    esac
    cargo clippy --manifest-path libs/{{ lib }}/Cargo.toml "${args[@]}" --all-targets --all-features -- -D warnings
    if [[ "{{ lib }}" == ala ]]; then
        cargo test --manifest-path libs/ala/Cargo.toml --workspace --all-features --exclude ala-platform
    else
        cargo test --manifest-path libs/penna/Cargo.toml "${args[@]}" --all-features
    fi

# Push zz's commits under libs/<lib> to autistic-computer/<lib> main.
push lib:
    git subtree push --prefix=libs/{{ lib }} {{ org }}/{{ lib }}.git main

# Merge autistic-computer/<lib> main into libs/<lib>.
pull lib:
    git subtree pull --prefix=libs/{{ lib }} {{ org }}/{{ lib }}.git main -m "Pull {{ lib }} from autistic-computer/{{ lib }}"
