import glob
import json
import os
import re
import sys

home = sys.argv[1]
commands = {}
res = {"claude": {"in": 0, "cached": 0, "out": 0, "models": []}, "codex": {"in": 0, "cached": 0, "out": 0, "models": []}}


def lines(path):
    for line in open(path, errors="replace"):
        try:
            yield json.loads(line)
        except ValueError:
            pass


def text(v):
    if isinstance(v, str):
        try:
            v = json.loads(v)
        except ValueError:
            return v
    if isinstance(v, list):
        return "\n".join(text(x.get("text", x.get("content", ""))) if isinstance(x, dict) else str(x) for x in v)
    if isinstance(v, dict):
        inner = v.get("output", v.get("content"))
        return json.dumps(v) if inner is None else text(inner)
    return str(v)


def results(path):
    for o in lines(path):
        m = o.get("message") if isinstance(o, dict) else None
        if isinstance(m, dict) and o.get("type") == "user" and isinstance(m.get("content"), list):
            for part in m["content"]:
                if isinstance(part, dict) and part.get("type") == "tool_result" and part.get("tool_use_id") in byid:
                    byid[part["tool_use_id"]]["out"] = text(part.get("content"))[:1500]


byid = {}
seen = set()
for path in glob.glob(os.path.join(home, ".claude", "projects", "**", "*.jsonl"), recursive=True):
    for o in lines(path):
        m = o.get("message") if isinstance(o, dict) else None
        if not isinstance(m, dict) or o.get("type") != "assistant":
            continue
        for part in m.get("content") or []:
            if isinstance(part, dict) and part.get("type") == "tool_use" and part.get("id") not in byid:
                inp = part.get("input") or {}
                cmd = inp.get("command") if part.get("name") == "Bash" else part.get("name") + " " + json.dumps(inp)[:200]
                byid[part.get("id")] = {"cmd": cmd, "out": ""}
                commands.setdefault(o.get("sessionId", ""), []).append(byid[part.get("id")])
        mid = m.get("id")
        if mid in seen:
            continue
        seen.add(mid)
        u = m.get("usage") or {}
        c = res["claude"]
        c["in"] += u.get("input_tokens", 0) + u.get("cache_creation_input_tokens", 0)
        c["cached"] += u.get("cache_read_input_tokens", 0)
        c["out"] += u.get("output_tokens", 0)
        if m.get("model") and m["model"] not in c["models"] and not m["model"].startswith("<"):
            c["models"].append(m["model"])
    results(path)

for path in glob.glob(os.path.join(home, ".codex", "sessions", "**", "*.jsonl"), recursive=True):
    total = None
    c = res["codex"]
    sid = ""
    for o in lines(path):
        p = o.get("payload") if isinstance(o, dict) else None
        if not isinstance(p, dict):
            continue
        if o.get("type") == "session_meta":
            sid = p.get("id", "")
        if o.get("type") == "response_item" and p.get("type") in ("function_call", "custom_tool_call", "local_shell_call"):
            arg = p.get("arguments") or p.get("input") or p.get("action")
            arg = arg if isinstance(arg, str) else json.dumps(arg)
            hit = re.search(r'cmd"?\s*:\s*("(?:[^"\\]|\\.)*")', arg)
            cmd = json.loads(hit.group(1)) if hit else arg
            if "require_escalated" in arg:
                cmd += "   [escalated]"
            byid[p.get("call_id")] = {"cmd": cmd, "out": ""}
            commands.setdefault(sid, []).append(byid[p.get("call_id")])
        if o.get("type") == "response_item" and p.get("type") in ("function_call_output", "custom_tool_call_output") and p.get("call_id") in byid:
            byid[p["call_id"]]["out"] = text(p.get("output"))[:1500]
        if o.get("type") == "turn_context" and p.get("model") and p["model"] not in c["models"]:
            c["models"].append(p["model"])
        if p.get("type") == "token_count" and isinstance(p.get("info"), dict):
            total = p["info"].get("total_token_usage") or total
    if total:
        c["in"] += total.get("input_tokens", 0) - total.get("cached_input_tokens", 0)
        c["cached"] += total.get("cached_input_tokens", 0)
        c["out"] += total.get("output_tokens", 0)

res["commands"] = commands
json.dump(res, sys.stdout)
