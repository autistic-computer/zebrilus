import "settings.just"

set working-directory := "../.."

org := "git@github.com:autistic-computer"

# Lint and test a lib (gpui|kit) on its own, the way its repo's CI does, against the gpui in libs/.
check lib:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ lib }}" in
        gpui) args=(--workspace) ;;
        kit) args=(--config "patch.\"https://github.com/autistic-computer/gpui\".zz-gpui.path=\"$PWD/libs/gpui/crates/zz-gpui\"") ;;
        *) echo "unknown lib: {{ lib }} (expected: gpui|kit)" >&2; exit 2 ;;
    esac
    cargo clippy --manifest-path libs/{{ lib }}/Cargo.toml "${args[@]}" --all-targets --all-features -- -D warnings
    if [[ "{{ lib }}" == gpui ]]; then
        cargo test --manifest-path libs/gpui/Cargo.toml --workspace --all-features --exclude zz-gpui-platform
    else
        cargo test --manifest-path libs/kit/Cargo.toml "${args[@]}" --all-features
    fi

# Push zz's commits under libs/<lib> to autistic-computer/<lib> main.
push lib:
    git subtree push --prefix=libs/{{ lib }} {{ org }}/{{ lib }}.git main

# Merge autistic-computer/<lib> main into libs/<lib>.
pull lib:
    git subtree pull --prefix=libs/{{ lib }} {{ org }}/{{ lib }}.git main -m "Pull {{ lib }} from autistic-computer/{{ lib }}"
