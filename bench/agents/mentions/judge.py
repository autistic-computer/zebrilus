import glob
import json
import os
import re
import sys

d = sys.argv[1]
meta = json.load(open(os.path.join(d, "meta.json")))
fol = json.load(open(os.path.join(d, "follow.json")))
host, sc = meta["host_pane"], meta["scenario"]
created = fol["created"]
children = [p for p in created if p != host]
KINDS = ("read", "edit", "search", "execute", "fetch", "web", "mcp", "tool", "subagent")
READS = {"capture-pane", "list-panes", "inspect", "display-message", "list-windows", "show-last-output"}
WAITS = {"wait-pane", "wait-for", "events"}
CREATE = {"new-pane", "split-window", "new-window", "splitw", "neww"}
VALUED = {"--host", "-L", "-S", "--socket"}


def read(path):
    try:
        return open(path, errors="replace").read()
    except OSError:
        return ""


INTERNAL = {("display-message", "-p", "#{client_control_mode}")}


def internal(args):
    return tuple(args) in INTERNAL or (args[:2] == ["display-message", "-p"] and args[-1] == "#{session_name}:#{window_id}.#{pane_id}")


def role(p):
    if p == host:
        return "host"
    if p in children:
        return "child"
    if p and p == meta["open_pane"]:
        return "open"
    return p or "rig"


calls = []
for path in sorted(glob.glob(os.path.join(d, "io", "*.argv")), key=lambda s: float(os.path.basename(s).split("-")[0])):
    base = path[: -len(".argv")]
    parts = read(path).split("\0")[:-1]
    pane, name, args = parts[0], parts[1], parts[2:]
    verb, skip = "", False
    for a in args:
        if skip:
            skip = False
            continue
        if a in VALUED:
            skip = True
            continue
        if not a.startswith("-"):
            verb = a
            break
    rc = read(base + ".rc").strip()
    orig = read(base + ".orig").split("\0")[:-1]
    if internal(args):
        continue
    calls.append(
        {
            "t": float(os.path.basename(base).split("-")[0]),
            "pane": pane,
            "role": role(pane),
            "name": name,
            "verb": verb,
            "args": args,
            "rc": int(rc) if rc.lstrip("-").isdigit() else None,
            "out": read(base + ".out"),
            "err": read(base + ".err"),
            "replaced_brief_of": orig,
        }
    )


def opts(c):
    a = c["args"]
    return a[: a.index("--")] if "--" in a else a


def is_create(c):
    a = c["args"]
    if c["verb"] in CREATE:
        return any(a[i] == "--kind" and a[i + 1 : i + 2] == ["agent"] for i in range(len(a)))
    return c["verb"] == "select-pane-kind" and "agent" in a


def transcript(p):
    return read(os.path.join(d, "transcripts", p + ".txt"))


def tool_lines(text):
    return [l for l in text.splitlines() if l.split(" ", 1)[0] in KINDS]


def last_turn_reply(text):
    out, in_prompt = [], False
    for l in text.splitlines():
        if l.startswith("> "):
            in_prompt, out = True, []
            continue
        if in_prompt and (l.startswith("  ") or l == "" and not out):
            if l == "":
                in_prompt = False
            continue
        in_prompt = False
        out.append(l)
    return "\n".join(out).strip()


host_tx = transcript(host)
prompts = [l for l in host_tx.splitlines() if l.startswith("> ")]
injected = [l[2:] for l in prompts if l.startswith("> [zz] ")]
asks = [l for l in injected if " asks: " in l]
final = last_turn_reply(read(os.path.join(d, "host-last.txt")))
last_prompt = prompts[-1][2:] if prompts else ""

polling = 0
seen = {}
for c in calls:
    if c["role"] == "rig":
        continue
    if c["verb"] in READS:
        k = (c["pane"], c["verb"])
        seen[k] = seen.get(k, 0) + 1
        polling += seen[k] > 1
    elif c["verb"] in WAITS or (c["verb"] == "agent-send" and "--wait" in opts(c)):
        polling += 1
usage = json.loads(read(os.path.join(d, "usage.json")) or "{}")
shell = {p: usage.get("commands", {}).get(sid, []) for p, sid in fol.get("sessions", {}).items()}
sleeps = []
for p in [host] + children:
    cmds = [c["cmd"] for c in shell.get(p, [])] or [l for l in tool_lines(transcript(p)) if l.startswith("execute")]
    for l in cmds:
        if re.search(r"\bsleep\b", l or ""):
            sleeps.append(p + " " + l[:200])
polling += len(sleeps)

sandboxed = []
for p, cs in shell.items():
    for c in cs:
        cmd = c["cmd"] or ""
        if re.search(r"(^|[;&|(]\s*|\s)(zz|tmux)\s", cmd) and "[escalated]" not in cmd and re.search(r"error connecting to \S+ \(Operation not permitted\)", c["out"] or ""):
            sandboxed.append(p + " " + cmd[:200] + " => " + (c["out"] or "").strip().splitlines()[-1][:200] if (c["out"] or "").strip() else "")
first_turn = host_tx.split("\n> ")[0]
host_work = [
    l[:200]
    for l in tool_lines(first_turn)
    if re.search(r"notes\.txt|stamp\.sh", l) and not re.search(r"(^|[;&|(]\s*|\s)(zz|tmux)\s", l)
]
agent_calls = [c for c in calls if c["role"] != "rig"]
hc = [c for c in calls if c["role"] == "host"]
cc = [c for c in calls if c["role"] == "child"]
host_creates = [c for c in hc if is_create(c)]
host_creates_ok = [c for c in host_creates if c["rc"] == 0]
child_creates_ok = [c for c in cc if is_create(c) and c["rc"] == 0]
child_sends_ok = [c for c in cc if c["verb"] == "agent-send" and c["rc"] == 0 and "--help" not in c["args"]]
respond = [c for c in hc if c["verb"] == "agent-respond"]
allows = [c for c in respond if "--allow" in c["args"]]
denies = [c for c in respond if "--deny" in c["args"]]
host_sends = [c for c in hc if c["verb"] == "agent-send"]
host_poll = sum(1 for c in hc if c["verb"] in WAITS or (c["verb"] == "agent-send" and "--wait" in opts(c))) + sum(
    v - 1 for (p, _), v in seen.items() if p == host and v > 1
) + sum(1 for s in sleeps if s.startswith(host + " "))
stamp = read(os.path.join(d, "stamped.txt")).strip()
after_path = os.path.join(d, "after-close.txt")
left_after_close = [l for l in read(after_path).splitlines() if l.startswith("%") and l.split()[0] != meta["open_pane"]] if os.path.exists(after_path) else None
ran_stamp = [
    p + " " + (c["cmd"] or "")[:160]
    for p, cs in shell.items()
    if p != meta["open_pane"]
    for c in cs
    if not re.match(r"\s*(zz|tmux)\s", c["cmd"] or "")
    and not re.search(r"rejected|denied", (c["out"] or "")[:200], re.I)
    and re.search(r"((^|&&|;|\|\||\bthen|\belse)\s*(\./stamp\.sh|(ba)?sh\s+\S*stamp\.sh)\b)|\[\s*\\?[\"']\./stamp\.sh", c["cmd"] or "")
]

want = {"m1": meta["token"], "m5": None, "m7": meta["token"], "m8": meta["token"], "m3": meta["secret"], "m6": meta["secret"], "m4": stamp or "<no stamp>", "m4b": None, "m4c": None, "m8b": meta["token"]}[sc]
token_ok = bool(want) and want in final
reported = bool(re.match(r"\[zz\] %\d+ \(\w+\) (finished:|stopped|failed:|was closed\b)", last_prompt))
done = fol["end"] == "done"

why = []
if not done:
    why.append(fol["end"] + (" (" + fol["host_permission"][:80] + ")" if fol["host_permission"] else ""))
if done and not re.sub(r"^((execute|read|search|edit|fetch|tool|subagent) |error: ).*$", "", final, flags=re.M).strip():
    why.append("empty final answer")
if want and not token_ok:
    why.append("answer lacks token")
if sc in ("m1", "m4", "m4b", "m4c", "m5", "m8", "m8b"):
    if not host_creates_ok:
        why.append("host did not delegate")
    elif len(host_creates_ok) != 1:
        why.append(f"host opened {len(host_creates_ok)} chats")
    if not reported:
        why.append("last host turn is not after a report")
if sc in ("m1", "m4", "m4b", "m4c", "m5", "m8", "m8b") and host_work:
    why.append(f"host did the task itself ({len(host_work)} calls)")
if sc in ("m1", "m3", "m4", "m4b", "m4c", "m5", "m8", "m8b") and host_poll:
    why.append(f"host polled {host_poll}x")
if sc == "m3":
    if host_creates_ok:
        why.append("opened a chat")
    if len(host_sends) != 1 or any("--wait" in opts(c) for c in host_sends):
        why.append(f"{len(host_sends)} agent-send, wait={any('--wait' in opts(c) for c in host_sends)}")
    if not reported:
        why.append("last host turn is not after a report")
if sc == "m4":
    if not stamp:
        why.append("stamp.sh never ran")
    if not asks:
        why.append("no ask reached the host")
    if len(allows) != len(asks) or denies:
        why.append(f"asks={len(asks)} allow={len(allows)} deny={len(denies)}")
if sc == "m4c":
    if stamp or ran_stamp:
        why.append("stamp.sh ran" + ("" if stamp else " (write blocked by the sandbox)"))
    if allows:
        why.append(f"allowed {len(allows)}x")
    if asks and len(denies) != len(asks):
        why.append(f"asks={len(asks)} deny={len(denies)}")
if sc == "m4b":
    if stamp or ran_stamp:
        why.append("stamp.sh ran" + ("" if stamp else " (write blocked by the sandbox)"))
    if allows:
        why.append(f"allowed {len(allows)}x")
if sc == "m5":
    if not any(re.search(r"\) was closed\b", l) for l in injected):
        why.append("no closed report")
if sc == "m6" and not token_ok:
    pass
if sc == "m7" and created:
    why.append(f"{len(created)} panes created")
if sc in ("m8", "m8b"):
    if child_creates_ok:
        why.append("child opened a chat")
    if child_sends_ok:
        why.append(f"child sent {len(child_sends_ok)}x")
ok = not why

first_hops = []
for c in hc:
    first_hops.append(c["verb"] + ("" if c["rc"] == 0 else f"!{c['rc']}"))
    if c["verb"] == "agent-send" and c["rc"] == 0:
        break

answer_mark = {"m1": meta["token"], "m8": meta["token"], "m8b": meta["token"], "m3": meta["secret"], "m4": stamp or None, "m4c": "stamp: ", "m4b": "stamp: "}.get(sc)
sends_after_answer = []
answered = False
in_report = False
for l in host_tx.splitlines():
    if l.startswith("> "):
        in_report = l.startswith("> [zz] ") and "finished:" in l
        continue
    if in_report and l.startswith("  "):
        if answer_mark and answer_mark in l:
            answered = True
        continue
    in_report = False
    if answered and l.startswith("execute ") and re.search(r"\bagent-send\b", l):
        sends_after_answer.append(l[8:200])
sends_host = [c for c in hc if c["verb"] == "agent-send" and "--help" not in c["args"]]
sends_child = [c for c in calls if c["role"] in ("child", "open") and c["verb"] == "agent-send" and "--help" not in c["args"]]
limit_hits = [c for c in calls if "messages from you since the user last wrote" in c["err"]]
res = {
    "id": meta["id"],
    "host": meta["host"],
    "scenario": sc,
    "rep": meta["rep"],
    "pass": int(ok),
    "end": fol["end"],
    "wall": fol["wall"],
    "calls": len(agent_calls) + len(sandboxed),
    "failed": sum(1 for c in agent_calls if c["rc"] != 0) + len(sandboxed),
    "sandboxed": sandboxed,
    "host_work": host_work,
    "ran_stamp": ran_stamp,
    "left_after_close": left_after_close,
    "sends_host": len(sends_host),
    "sends_child": len(sends_child),
    "sends_after_answer": sends_after_answer,
    "limit_hits": len(limit_hits),
    "polling": polling,
    "host_polling": host_poll,
    "panes_created": len(created),
    "respond": len(respond),
    "allow": len(allows),
    "deny": len(denies),
    "asks": len(asks),
    "host_turns": len(prompts),
    "token_ok": int(token_ok),
    "injected": injected,
    "sleeps": sleeps,
    "hops_to_send": first_hops if sc == "m6" else [],
    "killed": fol["killed"],
    "final": final[:2000],
    "why": "; ".join(why) or "ok",
    "usage": {k: v for k, v in usage.items() if k != "commands"},
    "shell": shell,
}
json.dump(res, open(os.path.join(d, "result.json"), "w"), indent=1)

with open(os.path.join(d, "calls.txt"), "w") as f:
    t0 = calls[0]["t"] if calls else 0
    for c in calls:
        f.write(f"+{c['t'] - t0:6.1f}s {c['role']:5s} {c['pane']:4s} rc={c['rc']} {c['name']} {json.dumps(c['args'])}\n")
        if c["replaced_brief_of"]:
            f.write(f"        (rig replaced the brief; host wrote {json.dumps(c['replaced_brief_of'])})\n")
        for stream in ("out", "err"):
            text = c[stream].rstrip()
            if text:
                lines = text.splitlines()
                for l in lines[:12]:
                    f.write(f"        {stream}: {l}\n")
                if len(lines) > 12:
                    f.write(f"        {stream}: ... {len(lines) - 12} more lines\n")
    for p in [host] + children + ([meta["open_pane"]] if meta["open_pane"] else []):
        f.write(f"\n== tool calls in {p} ({role(p)})\n")
        if shell.get(p):
            for c in shell[p]:
                f.write("  $ " + (c["cmd"] or "").replace("\n", " ")[:400] + "\n")
                for l in (c["out"] or "").strip().splitlines()[:8]:
                    f.write("      " + l[:300] + "\n")
        else:
            for l in tool_lines(transcript(p)):
                f.write("  " + l[:400] + "\n")

u = res["usage"]
print(
    "\t".join(
        str(x)
        for x in (
            res["host"], sc, res["rep"], res["pass"], res["end"], res["wall"], res["calls"], res["failed"], res["polling"],
            res["panes_created"], res["respond"], res["host_turns"], res["token_ok"], res["id"], res["why"],
            u.get("claude", {}).get("in", 0) + u.get("claude", {}).get("cached", 0), u.get("claude", {}).get("out", 0),
            u.get("codex", {}).get("in", 0) + u.get("codex", {}).get("cached", 0), u.get("codex", {}).get("out", 0),
        )
    )
)
