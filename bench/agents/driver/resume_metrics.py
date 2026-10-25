"""Score a killed-and-resumed driver run: resume_metrics.py RUN_DIR [--row]."""

import datetime
import json
import pathlib
import re
import subprocess
import sys

D = pathlib.Path(sys.argv[1])
META = dict(kv.split("=", 1) for kv in D.joinpath("meta").read_text().split() if "=" in kv)
START = re.compile(r"\b(new-session|new-window|split-window|new-pane)\b")
AGENT = re.compile(r"\b(claude|codex)\b")


def load(path):
    out = []
    for line in path.read_text().splitlines() if path.exists() else []:
        try:
            out.append(json.loads(line))
        except ValueError:
            pass
    return out


def ts(value):
    return datetime.datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()


def usage_rows(entries):
    seen, rows = set(), []
    for e in entries:
        m = e.get("message") or {}
        if e.get("type") == "assistant" and m.get("usage") and m.get("id") not in seen:
            seen.add(m.get("id"))
            u = m["usage"]
            if not (u.get("input_tokens", 0) + u.get("cache_creation_input_tokens", 0) + u.get("cache_read_input_tokens", 0)):
                continue
            rows.append((e["timestamp"], u.get("input_tokens", 0) + u.get("cache_creation_input_tokens", 0)
                         + u.get("cache_read_input_tokens", 0), u.get("cache_read_input_tokens", 0), u.get("output_tokens", 0)))
    return rows


pre = load(D / "pre.jsonl")
final = load(D / "transcript.jsonl")
post = final[len(pre):] if META.get("resume") == "full" else final
calls = [b for e in post if e.get("type") == "assistant" for b in (e.get("message") or {}).get("content") or []
         if b.get("type") == "tool_use"]
bash = [c["input"].get("command", "") for c in calls if c.get("name") == "Bash"]
rows = usage_rows(post)
pre_rows = usage_rows(pre)
done_ts = next((e["timestamp"] for e in post if e.get("type") == "assistant" and any(
    b.get("type") == "text" and "DRIVER-DONE" in b.get("text", "") for b in e["message"]["content"])), None)
first_ts = next((e["timestamp"] for e in post if e.get("timestamp")), None)

outcome = json.loads(subprocess.run([sys.executable, str(pathlib.Path(__file__).with_name("metrics.py")), str(D)],
                                    capture_output=True, text=True).stdout or "{}")
m = {
    "run": D.name, "resume": META.get("resume", ""), "model": META.get("model", ""), "status": META.get("status", ""),
    "pass": outcome.get("pass", ""), "detail": outcome.get("detail", ""), "wrong_done": outcome.get("wrong_done", ""),
    "pre_ctx": pre_rows[-1][1] if pre_rows else 0,
    "first_ctx": rows[0][1] if rows else 0,
    "first_cached": rows[0][2] if rows else 0,
    "post_peak_ctx": max((r[1] for r in rows), default=0),
    "post_requests": len(rows),
    "post_input_tok": sum(r[1] for r in rows),
    "post_out_tok": sum(r[3] for r in rows),
    "first_reply_s": round(ts(rows[0][0]) - ts(first_ts)) if rows and first_ts else "",
    "post_wall_s": round(ts(done_ts) - float(META["killed"])) if done_ts and META.get("killed") else "",
    "post_tools": len(calls),
    "new_workers": sum(1 for c in bash if START.search(c) and AGENT.search(c)),
    "resent_briefs": sum(1 for c in bash if "agent-send" in c and (re.search(r"<\s*\S+\.(md|txt)", c) or len(c) > 600)),
    "rearms": sum(1 for c in bash if "agent-send" in c and not re.search(r"<\s*\S+\.(md|txt)", c) and len(c) <= 600),
    "packet_bytes": (D / "packet.md").stat().st_size if (D / "packet.md").exists() else "",
}
COLUMNS = list(m)
if len(sys.argv) > 2 and sys.argv[2] == "--row":
    print("\t".join(str(m[c]) for c in COLUMNS))
elif len(sys.argv) > 2 and sys.argv[2] == "--header":
    print("\t".join(COLUMNS))
else:
    print(json.dumps(m, indent=1))
