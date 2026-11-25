import json
import os
import subprocess
import sys
import time

run, start, tmo = sys.argv[1], float(sys.argv[2]), float(sys.argv[3])
meta = json.load(open(os.path.join(run, "meta.json")))
host = meta["host_pane"]
scenario = meta["scenario"]
z = os.path.join(run, "z")
pre = set(open(os.path.join(run, "pre-panes.txt")).read().split())
SETTLE = 12
HOST_BLOCK = 90
CHILD_BLOCK = 120
BUSY = {"starting", "working", "needs-approval", "needs-input"}


def zz(*args):
    return subprocess.run([z, *args], capture_output=True, text=True, timeout=30)


def panes():
    out = zz("inspect", "-a", "--json").stdout
    res = {}
    for line in out.splitlines():
        try:
            o = json.loads(line)
        except ValueError:
            continue
        res[o["pane_id"]] = o
    return res


timeline, last, created, sessions = [], {}, [], {}
killed = None
seen_work = False
idle_since = host_blocked = child_blocked = None
end = "timeout"
while True:
    now = time.time()
    t = round(now - start, 1)
    ps = panes()
    for pid, o in ps.items():
        if o.get("agent_session_id"):
            sessions[pid] = o["agent_session_id"]
        key = (o.get("agent_state", ""), o.get("agent_pending_permission", ""))
        if o.get("pane_kind") == "agent" and last.get(pid) != key:
            perm = json.dumps(o["permission"])[:400] if o.get("permission") else ""
            timeline.append({"t": t, "pane": pid, "state": key[0], "pending": key[1], "permission": perm, "parent": o.get("agent_parent", "")})
            last[pid] = key
        if pid not in pre and pid not in created:
            created.append(pid)
            timeline.append({"t": t, "pane": pid, "created": o.get("pane_kind"), "parent": o.get("agent_parent", ""), "provider": o.get("agent_provider", "")})
    for pid in last:
        if pid not in ps and last[pid] != ("gone", ""):
            timeline.append({"t": t, "pane": pid, "state": "gone"})
            last[pid] = ("gone", "")
    if host not in ps:
        end = "host-gone"
        break
    if scenario == "m5" and killed is None:
        for pid in created:
            o = ps.get(pid, {})
            if o.get("pane_kind") == "agent" and o.get("agent_state") == "working":
                with open(os.path.join(run, "transcripts", pid + ".before-kill.txt"), "w") as f:
                    f.write(zz("capture-pane", "-p", "-S", "-", "-t", pid).stdout)
                r = zz("kill-pane", "-t", pid)
                killed = {"pane": pid, "t": round(time.time() - start, 1), "rc": r.returncode}
                timeline.append({"t": killed["t"], "pane": pid, "killed": r.returncode})
                break
    hs = ps[host].get("agent_state", "")
    if hs != "idle":
        seen_work = True
    if now - start > 20:
        seen_work = True
    states = {pid: o.get("agent_state", "") for pid, o in ps.items() if o.get("pane_kind") == "agent"}
    if hs in ("needs-approval", "needs-input"):
        host_blocked = host_blocked or now
        if now - host_blocked > HOST_BLOCK:
            end = "host-blocked"
            break
    else:
        host_blocked = None
    stuck = [p for p, s in states.items() if p != host and s in ("needs-approval", "needs-input")]
    if stuck and hs not in BUSY:
        child_blocked = child_blocked or now
        if now - child_blocked > CHILD_BLOCK:
            end = "child-blocked"
            break
    else:
        child_blocked = None
    busy = any(s in BUSY for s in states.values())
    if seen_work and not busy:
        idle_since = idle_since or now
        if now - idle_since >= SETTLE:
            end = "done"
            break
    else:
        idle_since = None
    if now - start > tmo:
        break
    time.sleep(0.3 if scenario == "m5" and killed is None else 1)

wall = (idle_since or time.time()) - start if end == "done" else time.time() - start
ps = panes()
json.dump(
    {
        "end": end,
        "start": start,
        "wall": round(wall, 1),
        "created": created,
        "sessions": sessions,
        "killed": killed,
        "final_states": {p: o.get("agent_state", "") for p, o in ps.items() if o.get("pane_kind") == "agent"},
        "host_permission": json.dumps(ps.get(host, {}).get("permission") or "")[:400] if ps.get(host, {}).get("permission") else "",
        "timeline": timeline,
    },
    open(os.path.join(run, "follow.json"), "w"),
    indent=1,
)
