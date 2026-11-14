#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

repo=autistic-computer/zebrilus
url="https://github.com/$repo"
version=2.338.2
linux_host="${ZZ_RELEASE_LINUX:-alienware}"
windows_host="${ZZ_RELEASE_WINDOWS:-win-desktop}"
mac_dir="$HOME/.local/share/zz-release-runner"
windows_dir='E:\zz-release-runner'
image=zz-release-runner
logs=target/release-runners
names=(zz-macos zz-windows zz-linux-x86_64 zz-linux-aarch64)

die() { echo "error: $*" >&2; exit 1; }
note() { echo "==> $*"; }

usage() {
    echo "usage: release-runners.sh <setup|dry-run|ship <level-or-version>|stop>" >&2
    exit 2
}

[[ "$(uname -s)" == Darwin ]] || die "run this from the Mac: it hosts the macOS and Linux aarch64 runners"

setup() {
    note "macOS runner in $mac_dir"
    if [[ ! -x "$mac_dir/config.sh" ]]; then
        mkdir -p "$mac_dir"
        curl -fsSL "https://github.com/actions/runner/releases/download/v$version/actions-runner-osx-arm64-$version.tar.gz" \
            | tar -xz -C "$mac_dir"
    fi

    note "Linux aarch64 image on this Mac"
    docker build -q --build-arg RUNNER_VERSION="$version" --build-arg RUNNER_ARCH=arm64 \
        -t "$image" scripts/release-runner

    note "Linux x86_64 image on $linux_host"
    tar -C scripts/release-runner -c . | ssh -o BatchMode=yes "$linux_host" \
        "docker build -q --build-arg RUNNER_VERSION=$version --build-arg RUNNER_ARCH=x64 -t $image -"

    note "Windows runner and Inno Setup on $windows_host"
    ssh -o BatchMode=yes "$windows_host" "
        \$ErrorActionPreference = 'Stop'
        New-Item -ItemType Directory -Force '$windows_dir' | Out-Null
        if (-not (Test-Path '$windows_dir\config.cmd')) {
            \$zip = Join-Path \$env:TEMP 'actions-runner.zip'
            Invoke-WebRequest -Uri 'https://github.com/actions/runner/releases/download/v$version/actions-runner-win-x64-$version.zip' -OutFile \$zip
            Expand-Archive \$zip -DestinationPath '$windows_dir' -Force
            Remove-Item \$zip
        }
        if (-not (Test-Path \"\$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe\") -and -not (Test-Path 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe')) {
            winget install --id JRSoftware.InnoSetup -e --silent --accept-package-agreements --accept-source-agreements
        }
    "

    if ! xcrun notarytool history --keychain-profile zz-notary >/dev/null 2>&1; then
        echo "note: save the notary profile once with: just release mac setup"
    fi
}

registered() {
    gh api "repos/$repo/actions/runners" --paginate \
        -q '.runners[] | select(.name | startswith("zz-")) | "\(.id) \(.name) \(.status)"'
}

stop() {
    pkill -f "$mac_dir/bin/Runner.Listener" 2>/dev/null || true
    docker rm -f zz-linux-aarch64 >/dev/null 2>&1 || true
    ssh -o BatchMode=yes "$linux_host" "docker rm -f zz-linux-x86_64 >/dev/null 2>&1 || true" || true
    ssh -o BatchMode=yes "$windows_host" \
        "Get-Process Runner.Listener,Runner.Worker -ErrorAction SilentlyContinue | Where-Object { \$_.Path -like '$windows_dir*' } | Stop-Process -Force" \
        2>/dev/null || true
    local id name status
    while read -r id name status; do
        [[ -n "$id" ]] || continue
        gh api -X DELETE "repos/$repo/actions/runners/$id" >/dev/null && note "removed $name ($status)"
    done < <(registered)
    rm -f "$mac_dir/.runner" "$mac_dir/.credentials" "$mac_dir/.credentials_rsaparams"
}

start() {
    mkdir -p "$logs"
    local token
    token="$(gh api -X POST "repos/$repo/actions/runners/registration-token" -q .token)"

    (
        cd "$mac_dir"
        rm -f .runner .credentials .credentials_rsaparams
        LANG="${LANG:-en_US.UTF-8}" ./config.sh --unattended --ephemeral --replace --disableupdate \
            --url "$url" --token "$token" --name zz-macos --labels zz-macos --work _work
        LANG="${LANG:-en_US.UTF-8}" exec ./run.sh
    ) > "$logs/zz-macos.log" 2>&1 &

    RUNNER_TOKEN="$token" docker run --rm --name zz-linux-aarch64 \
        -e RUNNER_URL="$url" -e RUNNER_TOKEN -e RUNNER_NAME=zz-linux-aarch64 -e RUNNER_LABELS=zz-linux-aarch64 \
        -v zz-release-work:/home/runner/_work -v zz-release-cargo:/home/runner/.cargo/registry \
        "$image" > "$logs/zz-linux-aarch64.log" 2>&1 &

    printf '%s\n' "$token" | ssh -o BatchMode=yes "$linux_host" "
        read -r RUNNER_TOKEN; export RUNNER_TOKEN
        exec docker run --rm --name zz-linux-x86_64 \
            -e RUNNER_URL=$url -e RUNNER_TOKEN -e RUNNER_NAME=zz-linux-x86_64 -e RUNNER_LABELS=zz-linux-x86_64 \
            -v zz-release-work:/home/runner/_work -v zz-release-cargo:/home/runner/.cargo/registry \
            $image" > "$logs/zz-linux-x86_64.log" 2>&1 &

    printf '%s\n' "$token" | ssh -o BatchMode=yes "$windows_host" "
        \$token = [Console]::In.ReadLine()
        \$env:Path = 'C:\Program Files\Git\bin;' + \$env:LOCALAPPDATA + '\Programs\Inno Setup 6;' + \$env:Path
        \$env:CARGO_HOME = \"\$env:USERPROFILE\.cargo\"
        \$env:RUSTUP_HOME = \"\$env:USERPROFILE\.rustup\"
        \$env:USERPROFILE = '$windows_dir\home'
        \$env:HOME = \$env:USERPROFILE
        New-Item -ItemType Directory -Force \$env:USERPROFILE | Out-Null
        Set-Location '$windows_dir'
        Remove-Item .runner,.credentials,.credentials_rsaparams -ErrorAction SilentlyContinue
        .\config.cmd --unattended --ephemeral --replace --disableupdate --url $url --token \$token --name zz-windows --labels zz-windows --work _work
        .\run.cmd" > "$logs/zz-windows.log" 2>&1 &

    note "waiting for four runners to come online (logs in $logs)"
    local deadline=$((SECONDS + 300)) online
    while true; do
        online="$(registered | awk '$3 == "online"' | wc -l | tr -d ' ')"
        [[ "$online" -eq ${#names[@]} ]] && break
        if (( SECONDS > deadline )); then
            registered >&2
            die "only $online of ${#names[@]} runners came online"
        fi
        sleep 5
    done
    note "runners online: ${names[*]}"
}

watch_run() {
    local event="$1" since="$2" id=""
    local deadline=$((SECONDS + 120))
    while [[ -z "$id" ]]; do
        id="$(gh run list -R "$repo" -w Release -e "$event" -L 5 --json databaseId,createdAt \
            -q "[.[] | select(.createdAt >= \"$since\")][0].databaseId // empty")"
        (( SECONDS < deadline )) || die "no Release run started"
        [[ -n "$id" ]] || sleep 3
    done
    note "watching $url/actions/runs/$id"
    gh run watch -R "$repo" "$id" --exit-status --interval 15 > /dev/null
}

[[ $# -ge 1 ]] || usage
command="$1"
shift

case "$command" in
    setup) setup ;;
    stop) stop ;;
    dry-run)
        trap stop EXIT
        start
        since="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        gh workflow run release.yml -R "$repo" --ref main
        watch_run workflow_dispatch "$since"
        note "dry run passed"
        ;;
    ship)
        [[ $# -eq 1 ]] || usage
        scripts/preflight.sh
        trap stop EXIT
        start
        since="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        scripts/release.sh "$1" --execute
        watch_run push "$since"
        note "released"
        ;;
    *) usage ;;
esac
