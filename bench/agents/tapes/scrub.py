#!/usr/bin/env python3
import collections
import getpass
import json
import re
import socket
import subprocess
import sys
import urllib.parse
from pathlib import Path

CREDENTIAL = re.compile(r"(?i)(token|secret|authorization|api_?key|cookie|password)")
CODE = re.compile(r"(?i)^(code|auth_?code|device_?code|user_?code)$")
PLAN = re.compile(r"(?i)(subscription|tier|plan_?type)")
EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@([A-Za-z0-9-]+\.)+[A-Za-z]{2,}")
PERIOD_START = "2026-01-01T00:00:00.000+00:00"
PERIOD_END = "2026-01-08T00:00:00.000+00:00"
FIELDS = {
    "hostname": "host",
    "creditUsagePercent": 25.0,
    "billingPeriodStart": PERIOD_START,
    "billingPeriodEnd": PERIOD_END,
    "prepaidBalance": {"val": 0},
}

counts = collections.Counter()
removed = set()


def host_names():
    names = {socket.gethostname(), socket.gethostname().split(".")[0]}
    for key in ("ComputerName", "LocalHostName", "HostName"):
        try:
            names.add(subprocess.run(["scutil", "--get", key], capture_output=True, text=True).stdout.strip())
        except OSError:
            pass
    return sorted((name for name in names if len(name) >= 3), key=len, reverse=True)


def paths(run):
    pairs = []
    for root in (run, Path(str(run).replace("/private/tmp/", "/tmp/", 1))):
        pairs += [(f"{root}/work", "/work"), (f"{root}/home", "/home/user"), (str(root), "/rig")]
    pairs.append((str(Path.home()), "/home/user"))
    encoded = [(urllib.parse.quote(old, safe=""), urllib.parse.quote(new, safe="")) for old, new in pairs]
    return sorted(pairs + encoded, key=lambda pair: len(pair[0]), reverse=True)


def text(value, rules):
    for label, old, new in rules:
        pattern = re.compile(re.escape(old), re.IGNORECASE)
        value, replaced = pattern.subn(new, value)
        counts[label] += replaced

    def mail(match):
        if match.group(0).endswith("@example.com"):
            return match.group(0)
        counts["email"] += 1
        return "user@example.com"

    return EMAIL.sub(mail, value)


def walk(value, rules, key=None, parent=None):
    if key in FIELDS and value != FIELDS[key]:
        counts[f"field {key}"] += 1
        if isinstance(value, str) and len(value) >= 4:
            removed.add(value)
        return FIELDS[key]
    if key in ("start", "end") and parent == "currentPeriod" and isinstance(value, str):
        counts["field currentPeriod"] += 1
        return PERIOD_START if key == "start" else PERIOD_END
    if key == "name" and parent == "tier" and isinstance(value, str):
        counts["field tier.name"] += 1
        removed.add(value)
        return "REDACTED"
    if isinstance(value, dict):
        return {name: walk(item, rules, name, key) for name, item in value.items()}
    if isinstance(value, list):
        return [walk(item, rules, None, key) for item in value]
    if isinstance(value, str):
        if key and (CREDENTIAL.search(key) or CODE.match(key)) and value:
            counts[f"credential {key}"] += 1
            if len(value) >= 6:
                removed.add(value)
            return "REDACTED"
        if key and PLAN.search(key) and value:
            counts[f"field {key}"] += 1
            removed.add(value)
            return "REDACTED"
        return text(value, rules)
    return value


def main():
    if len(sys.argv) != 3:
        print("usage: scrub.py <raw tape> <out>", file=sys.stderr)
        return 2
    raw, out = Path(sys.argv[1]).resolve(), Path(sys.argv[2])
    run = raw.parent.parent if raw.parent.name == "tapes" else raw.parent
    login = getpass.getuser()
    hosts = host_names()
    rules = [("path " + new, old, new) for old, new in paths(run)]
    rules += [("host", name, "host") for name in hosts]
    rules.append(("login", login, "user"))
    lines = []
    for line in raw.read_text().splitlines():
        if line.strip():
            lines.append(json.dumps(walk(json.loads(line), rules), separators=(",", ":"), ensure_ascii=False))
    scrubbed = "\n".join(lines) + "\n"
    out.write_text(scrubbed)
    for label, count in sorted(counts.items()):
        if count:
            print(f"{count:6} {label}")
    banned = {"login": [login], "home": [str(Path.home())], "host": hosts, "redacted field": sorted(removed)}
    banned["run"] = [str(run), str(run).replace("/private/tmp/", "/tmp/", 1)]
    lowered = scrubbed.lower()
    found = [label for label, words in banned.items() if any(word.lower() in lowered for word in words if word)]
    if any(not match.group(0).endswith("@example.com") for match in EMAIL.finditer(scrubbed)):
        found.append("email")
    if found:
        print(f"check FAILED, {out} still holds: {', '.join(found)}", file=sys.stderr)
        return 1
    print(f"check passed: {len(lines)} lines")
    return 0


if __name__ == "__main__":
    sys.exit(main())
