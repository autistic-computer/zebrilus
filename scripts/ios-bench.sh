#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 3 ]] || { echo "usage: scripts/ios-bench.sh <device-udid> <app-bundle> <endpoint>" >&2; exit 2; }
udid="$1"
app="$2"
endpoint="$3"
bundle_id=dev.zz.ios-dev
cli="${ZZ_BENCH_CLI:-zz-dev}"
session="${ZZ_BENCH_SESSION:-iphone-bench}"
cycles="${ZZ_BENCH_CYCLES:-16}"
out="${ZZ_BENCH_OUT:-$(mktemp -d "${TMPDIR:-/tmp}/zz-ios-bench.XXXXXX")}"
mkdir -p "$out"

xcrun devicectl device info lockState --device "$udid" 2>/dev/null | grep -q "passcodeRequired: false" \
    || { echo "unlock the device and keep it unlocked for the run" >&2; exit 1; }
! "$cli" has-session -t "$session" 2>/dev/null \
    || { echo "session $session already exists on $cli; set ZZ_BENCH_SESSION" >&2; exit 1; }

terminate_app() {
    local processes
    processes="$(mktemp)"
    xcrun devicectl device info processes --device "$udid" --json-output "$processes" >/dev/null 2>&1 || true
    python3 - "$processes" <<'PY' | while read -r pid; do
import json, sys
try:
    processes = json.load(open(sys.argv[1]))["result"]["runningProcesses"]
except Exception:
    processes = []
for process in processes:
    if process.get("executable", "").rstrip("/").endswith("/ZZ"):
        print(process["processIdentifier"])
PY
        xcrun devicectl device process terminate --device "$udid" --pid "$pid" >/dev/null 2>&1 || true
    done
    rm -f "$processes"
}

cleanup() {
    terminate_app
    "$cli" kill-session -t "$session" >/dev/null 2>&1 || true
}
trap cleanup EXIT

"$cli" new-session -d -s "$session" -x 60 -y 40
"$cli" send-keys -t "$session" 'seq 1 4000' Enter
"$cli" split-window -h -t "$session"
"$cli" send-keys -t "$session" 'seq 1 4000' Enter
"$cli" select-pane -L -t "$session"
sleep 2

xcrun devicectl device install app --device "$udid" "$app" >/dev/null

run() {
    local kind="$1" seconds="$2" log="$out/$1.log" env_json launch elapsed=0
    shift 2
    env_json="$(python3 -c 'import json, sys; print(json.dumps(dict(arg.split("=", 1) for arg in sys.argv[1:])))' \
        "ZZ_ALA_ENDPOINT=$endpoint" "ZZ_ALA_SESSION=$session" ALA_FRAME_LOG=1 \
        ${ZZ_ALA_GLASS:+"ZZ_ALA_GLASS=$ZZ_ALA_GLASS"} "$@")"
    xcrun devicectl device process launch --device "$udid" --terminate-existing --console \
        --environment-variables "$env_json" "$bundle_id" > "$log" 2>&1 &
    launch=$!
    while ((elapsed < seconds)) && ! grep -q "bench done" "$log"; do
        sleep 1
        elapsed=$((elapsed + 1))
    done
    terminate_app
    kill "$launch" 2>/dev/null || true
    wait "$launch" 2>/dev/null || true
}

kinds=" ${ZZ_BENCH_KINDS:-swipe scroll sheet idle} "
[[ "$kinds" != *" swipe "* ]] || run swipe $((15 + cycles * 14 / 10)) "ALA_BENCH=swipe:$cycles"
[[ "$kinds" != *" scroll "* ]] || run scroll $((15 + cycles * 3)) "ALA_BENCH=scroll:$cycles"
[[ "$kinds" != *" sheet "* ]] || run sheet $((15 + cycles * 3)) "ALA_BENCH=sheet:$cycles"
[[ "$kinds" != *" idle "* ]] || run idle 25

drawer() {
    local name="$1"
    shift
    run "drawer-$name" 45 ZZ_ALA_SHEET_LOOP=1500 ALA_GPU_TIMES=1 "ALA_FRAME_STATS=frames-$name.jsonl" "$@"
    xcrun devicectl device copy from --device "$udid" --domain-type appDataContainer \
        --domain-identifier "$bundle_id" --source "tmp/frames-$name.jsonl" \
        --destination "$out/frames-$name.jsonl" >/dev/null 2>&1 || true
}
if [[ "$kinds" == *" drawer "* ]]; then
    "$cli" new-window -t "$session" --kind agent claude >/dev/null
    sleep 5
    drawer before ALA_RETAIN_SHADER_LAYERS=0
    drawer after
    drawer after-noglass ZZ_ALA_GLASS=none
fi

echo "logs: $out"
for kind in swipe scroll sheet; do
    [[ "$kinds" != *" $kind "* ]] || echo "$kind: $(grep -h "frames total" "$out/$kind.log" || echo "no result, see $out/$kind.log")"
done
[[ "$kinds" != *" idle "* ]] || echo "idle: $(grep -h "link:" "$out/idle.log" | tail -2 | tr '\n' ' ')"
[[ "$kinds" != *" drawer "* ]] || python3 - "$out" <<'PY'
import json, pathlib, re, sys

def pct(values, q):
    values = sorted(values)
    return values[min(len(values) - 1, round((len(values) - 1) * q))] if values else float("nan")

out = pathlib.Path(sys.argv[1])
for name in ("before", "after", "after-noglass"):
    log = out / f"drawer-{name}.log"
    if not log.exists():
        continue
    moves, gpu = [], []
    for line in log.read_text(errors="replace").splitlines():
        if m := re.search(r"\[zz-sheet\] (open|close) at=([\d.]+)", line):
            moves.append((float(m.group(2)), m.group(1)))
        if m := re.search(r"\[ala-gpu\] at=([\d.]+) (?:gpu|wait)=([\d.]+|NaN) shaders=(\d+)", line):
            gpu.append((float(m.group(1)), float(m.group(2)), int(m.group(3))))

    def phase(t):
        kind = None
        for at, move in moves:
            if at <= t:
                kind = ("opening" if t - at < 450 else "open") if move == "open" else ("closing" if t - at < 600 else "closed")
        return kind

    frames = out / f"frames-{name}.jsonl"
    stats = [json.loads(line) for line in frames.read_text().splitlines()] if frames.exists() else []
    print(f"drawer {name}: {len(moves)} moves")
    for kind in ("opening", "closing", "open", "closed"):
        rows = [(ms, shaders) for at, ms, shaders in gpu if phase(at) == kind]
        times = [f["timestamp_unix_ns"] / 1e6 for f in stats if phase(f["timestamp_unix_ns"] / 1e6) == kind]
        gaps = [b - a for a, b in zip(times, times[1:]) if b - a < 100]
        cpu = [(f["draw"]["wall_ns"] + ((f["phases"]["present_submit"] or {}).get("wall_ns") or 0)) / 1e6
               for f in stats if phase(f["timestamp_unix_ns"] / 1e6) == kind]
        print(f"  {kind:8} frames={len(rows)} sky renders={sum(s for _, s in rows)} "
              f"gpu p50={pct([r[0] for r in rows if r[0] == r[0]], .5):.2f} "
              f"p95={pct([r[0] for r in rows if r[0] == r[0]], .95):.2f} "
              f"interval p50={pct(gaps, .5):.2f} p95={pct(gaps, .95):.2f} "
              f"main thread p50={pct(cpu, .5):.2f} p95={pct(cpu, .95):.2f}")
PY
