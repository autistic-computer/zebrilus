#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

linux_host="${ZZ_PREFLIGHT_LINUX:-alienware}"
macos_host="${ZZ_PREFLIGHT_MACOS:-macbook}"
windows_host="${ZZ_PREFLIGHT_WINDOWS:-win-desktop}"
git_bash='C:\Program Files\Git\bin\bash.exe'

case "$(uname -s)" in
    Linux) local_os=linux ;;
    Darwin) local_os=macos ;;
    *) local_os=none ;;
esac

if [[ $# -eq 0 ]]; then
    set -- linux macos windows
fi

sha="$(git rev-parse HEAD)"
logs="target/preflight/${sha:0:9}"
mkdir -p "$logs"

if ! git diff --quiet HEAD; then
    echo "note: uncommitted changes are not checked, preflight runs ${sha:0:9}"
fi

sync_cmd() {
    local repo="$1" wt="$2"
    echo "set -e; [ -d $wt ] || git -C $repo worktree add -q --detach $wt $sha; cd $wt; git checkout -q --detach --force $sha; git clean -fdq"
}

run_platform() {
    local platform="$1" host repo wt
    case "$platform" in
        linux) host="$linux_host" ;;
        macos) host="$macos_host" ;;
        windows) host="$windows_host" ;;
        *) echo "unknown platform: $platform" >&2; return 2 ;;
    esac

    if [[ "$platform" == "$local_os" ]]; then
        wt="$HOME/dev/worktrees/zebrilus/preflight"
        bash -c "$(sync_cmd "$PWD" "$wt")"
        LANG="${LANG:-en_US.UTF-8}" "$wt/scripts/ci.sh" "$platform"
        return
    fi

    if [[ "$platform" == windows ]]; then
        repo=/e/autistic-computer/zebrilus
        wt=/e/worktrees/zebrilus/preflight
        git push -q --force "$host:E:/autistic-computer/zebrilus" "$sha:refs/preflight/head"
        ssh -o BatchMode=yes "$host" "& '$git_bash' -lc '$(sync_cmd "$repo" "$wt")'"
        ssh -o BatchMode=yes "$host" "& '$git_bash' -lc '$wt/scripts/ci.sh windows'"
    else
        repo='~/dev/autistic-computer/zebrilus'
        wt='~/dev/worktrees/zebrilus/preflight'
        git push -q --force "$host:dev/autistic-computer/zebrilus" "$sha:refs/preflight/head"
        ssh -o BatchMode=yes "$host" "bash -lc '$(sync_cmd "$repo" "$wt")'"
        ssh -tt -o BatchMode=yes "$host" "bash -lc '$wt/scripts/ci.sh $platform'"
    fi
}

pids=()
for platform in "$@"; do
    log="$logs/$platform.log"
    echo "$platform: $log"
    run_platform "$platform" < /dev/null > "$log" 2>&1 &
    job=$!
    pids+=("$job")
    tail -n +1 -F --pid="$job" "$log" 2>/dev/null | grep --line-buffered -E '^== ' | sed -u "s/^== /[$platform] /" &
done

status=0
for pid in "${pids[@]}"; do
    wait "$pid" || status=1
done
wait

echo
for platform in "$@"; do
    log="$logs/$platform.log"
    if grep -q '^== all passed' "$log"; then
        echo "$platform: passed"
    else
        echo "$platform: FAILED ($log)"
        grep -E '^== failed|FAILED|^error(\[|:)|panicked at' "$log" | head -12 | sed 's/^/  /'
        status=1
    fi
done
exit "$status"
