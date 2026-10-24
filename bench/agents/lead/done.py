import json
import sys

for line in open(sys.argv[1]):
    try:
        entry = json.loads(line)
    except ValueError:
        continue
    if entry.get("type") != "assistant":
        continue
    for block in entry.get("message", {}).get("content", []):
        if block.get("type") == "text" and any(l.strip() == "LEAD-DONE" for l in block.get("text", "").splitlines()):
            sys.exit(0)
sys.exit(1)
