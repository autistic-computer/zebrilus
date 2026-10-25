import json
import sys

n = 0
for line in open(sys.argv[1]):
    try:
        e = json.loads(line)
    except ValueError:
        continue
    if e.get("type") == "assistant":
        for b in e.get("message", {}).get("content", []):
            if b.get("type") == "tool_use" and b.get("name") == "Bash" and "agent-send" in b.get("input", {}).get("command", ""):
                n += 1
print(n)
