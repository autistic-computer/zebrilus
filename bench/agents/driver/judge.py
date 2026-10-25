"""Grade the briefs a driver sent its workers: judge.py RUN_DIR (prints JSON)."""

import json
import pathlib
import subprocess
import sys

D = pathlib.Path(sys.argv[1])
here = pathlib.Path(__file__).parent
briefs = json.loads(subprocess.run([sys.executable, str(here / "metrics.py"), str(D), "--briefs"],
                                   capture_output=True, text=True).stdout or "[]")
if not briefs:
    print(json.dumps({"n": 0, "mean": "", "scores": []}))
    sys.exit(0)

RUBRIC = """You grade task briefs that a Driver Agent sent to worker agents. Each worker starts with an
empty context and sees only its brief. Some items are shell commands that carry the brief
(an agent-send call, a heredoc, or a CLI started with a prompt); grade the brief text inside.
A follow-up message to a worker that already has a brief (a status question, an answer to its
question, a nudge) is not a brief: mark it "followup" and do not grade it.

Give one point for each criterion the brief meets:
1 goal: states the outcome wanted, not just an activity
2 self_contained: a newcomer can act on it; no reference to things only the driver knows
3 location: exact machine, paths, and branch where they matter
4 scope: says what is in and what is out (files not to touch, no pushing, etc.)
5 done: an explicit done criterion
6 verify: says how to check the result (a command or an observable fact)
7 reply: says what to send back, in what form and how short

Answer with JSON only: {"grades": [{"kind": "brief"|"followup", "points": 0-7, "missing": ["..."]}]}
in the same order as the items."""

items = "\n\n".join(f"=== item {i + 1} ===\n{b[:6000]}" for i, b in enumerate(briefs))
run = subprocess.run(["claude", "-p", "--model", "claude-sonnet-5-5", "--output-format", "json",
                      "--setting-sources", "project", "--strict-mcp-config", f"{RUBRIC}\n\n{items}"],
                     capture_output=True, text=True, timeout=600, cwd="/tmp")
try:
    text = json.loads(run.stdout)["result"]
    grades = json.loads(text[text.index("{"):text.rindex("}") + 1])["grades"]
except (ValueError, KeyError) as error:
    print(json.dumps({"n": len(briefs), "mean": "", "error": str(error), "raw": run.stdout[-2000:]}))
    sys.exit(0)
graded = [g for g in grades if g.get("kind") == "brief"]
mean = round(sum(g["points"] for g in graded) / (7 * len(graded)), 2) if graded else ""
print(json.dumps({"n": len(graded), "mean": mean, "grades": grades}))
