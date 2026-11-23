#!/usr/bin/env python3
import collections
import getpass
import json
import os
import re
import socket
import subprocess
import sys
import urllib.parse
from datetime import datetime, timedelta
from pathlib import Path

CREDENTIAL = re.compile(r"(?i)(token|secret|authorization|api_?key|cookie|password)")
CODE = re.compile(r"(?i)^(code|auth_?code|device_?code|user_?code)$")
PLAN = re.compile(r"(?i)(subscription|tier|plan_?type)")
SERVICE = re.compile(r"(?i)service_?tier")
EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@([A-Za-z0-9-]+\.)+[A-Za-z]{2,}")
DAEMON_SOCKET = re.compile(r"(/private)?/tmp/codex-daemon-\d+/[0-9a-f]{16,}")
CONNECTORS = re.compile(r"claude\.ai [A-Z]|mcp__claude_ai_|mcp__codex_apps__|openai-curated")
OFFSET = re.compile(r"(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}:\d{2})(\.\d+)?([+-])(\d{2}):(\d{2})")
PERIOD_START = "2026-01-01T00:00:00.000+00:00"
PERIOD_END = "2026-01-08T00:00:00.000+00:00"
RESET_AT = 1767225600
FIELDS = {
    "hostname": "host",
    "serverName": "host",
    "creditUsagePercent": 25.0,
    "billingPeriodStart": PERIOD_START,
    "billingPeriodEnd": PERIOD_END,
    "prepaidBalance": {"val": 0},
    "timezone": "UTC",
    "overageDisabledReason": "REDACTED",
}
IDS = {
    "accountUuid": "00000000-0000-4000-8000-000000000001",
    "creator_account_id": "00000000-0000-4000-8000-000000000001",
    "organizationUuid": "00000000-0000-4000-8000-000000000002",
    "creator_user_id": "user-00000000000000000000000000",
    "installationId": "00000000-0000-4000-8000-000000000003",
}
FIGURES = {
    "utilization": (25, 0.25),
    "usedPercent": (25, 25.0),
    "used_percent": (25, 25.0),
    "total_cost_usd": (0, 0.01),
    "totalCostUSD": (0, 0.01),
    "costUSD": (0, 0.01),
    "limit_dollars": (0, 0.0),
    "used_dollars": (0, 0.0),
    "remaining_dollars": (0, 0.0),
    "pid": (4242, 4242.0),
}
RESETS = ("resetsAt", "resets_at")
REQUEST_BODIES = {"api-request-blob": "message", "api-request-shape": "shape"}
CHAIN = ("type", "uuid", "parentUuid", "timestamp")
SHIPPED = {"computer-use", "zz-workspace"}

counts = collections.Counter()
removed = set()


def host_names():
    names = {socket.gethostname(), socket.gethostname().split(".")[0]}
    for key in ("ComputerName", "LocalHostName", "HostName"):
        try:
            names.add(subprocess.run(["scutil", "--get", key], check=False, capture_output=True, text=True).stdout.strip())
        except OSError:
            pass
    return sorted((name for name in names if len(name) >= 3), key=len, reverse=True)


def git(*args):
    try:
        return subprocess.run(["git", "-C", str(ROOT), *args], check=False, capture_output=True, text=True).stdout.strip()
    except OSError:
        return ""


def timezone():
    try:
        return os.readlink("/etc/localtime").split("zoneinfo/", 1)[-1]
    except OSError:
        return ""


def sanitized(path):
    return re.sub(r"[^A-Za-z0-9]", "-", path)


def paths(run):
    pairs = []
    for root in (run, Path(str(run).replace("/private/tmp/", "/tmp/", 1))):
        pairs += [(f"{root}/work", "/work"), (f"{root}/home", "/home/user"), (str(root), "/rig")]
        pairs += [(sanitized(f"{root}/work"), "-work"), (sanitized(str(root)), "-rig")]
    socket_path = f"/tmp/zzt-{run.name.rsplit('-', 1)[-1]}.sock"
    pairs += [(f"/private{socket_path}", "/tmp/zz.sock"), (socket_path, "/tmp/zz.sock")]
    pairs.append((str(ROOT), "/zz"))
    temp = os.environ.get("TMPDIR", "").rstrip("/")
    if temp.startswith("/var/folders/"):
        pairs += [(f"/private{temp}", "/tmp"), (temp, "/tmp")]
    pairs.append((str(Path.home()), "/home/user"))
    encoded = [(urllib.parse.quote(old, safe=""), urllib.parse.quote(new, safe="")) for old, new in pairs]
    return sorted(pairs + encoded, key=lambda pair: len(pair[0]), reverse=True)


def text(value, rules):
    for label, old, new in rules:
        pattern = re.compile(re.escape(old), re.IGNORECASE)
        value, replaced = pattern.subn(new, value)
        counts[label] += replaced
    value, replaced = DAEMON_SOCKET.subn("/tmp/codex-daemon/socket", value)
    counts["path codex daemon socket"] += replaced
    value, replaced = OFFSET.subn(utc, value)
    counts["time zone offset"] += replaced

    def mail(match):
        if match.group(0).endswith("@example.com"):
            return match.group(0)
        counts["email"] += 1
        return "user@example.com"

    return EMAIL.sub(mail, value)


def utc(match):
    day, clock, fraction, sign, hours, minutes = match.groups()
    if hours == minutes == "00":
        return match.group(0)
    offset = timedelta(hours=int(hours), minutes=int(minutes))
    local = datetime.fromisoformat(f"{day}T{clock}")
    moved = local - offset if sign == "+" else local + offset
    return f"{moved.isoformat(timespec='seconds')}{fraction or ''}+00:00"


def drop(value, size=4):
    if isinstance(value, str) and len(value) >= size and value != "REDACTED":
        removed.add(value)


def figure(key, value):
    whole, fraction = FIGURES[key]
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return value
    return whole if isinstance(value, int) else fraction


def walk(value, rules, key=None, parent=None):
    if key in FIELDS and value != FIELDS[key]:
        counts[f"field {key}"] += 1
        drop(value)
        return FIELDS[key]
    if key in IDS and isinstance(value, str) and value != IDS[key]:
        counts[f"id {key}"] += 1
        drop(value)
        return IDS[key]
    if key in FIGURES and figure(key, value) != value:
        counts[f"figure {key}"] += 1
        return figure(key, value)
    if key in RESETS and value is not None:
        counts[f"figure {key}"] += 1
        return RESET_AT if isinstance(value, (int, float)) else PERIOD_START
    if key == "balance" and parent == "credits" and value != "0":
        counts["figure credits.balance"] += 1
        return "0"
    if key in ("start", "end") and parent == "currentPeriod" and isinstance(value, str):
        counts["field currentPeriod"] += 1
        return PERIOD_START if key == "start" else PERIOD_END
    if key == "name" and parent == "tier" and isinstance(value, str) and value != "REDACTED":
        counts["field tier.name"] += 1
        drop(value)
        return "REDACTED"
    if isinstance(value, dict):
        return {name: walk(item, rules, name, key) for name, item in value.items()}
    if isinstance(value, list):
        return [walk(item, rules, None, key) for item in value]
    if isinstance(value, str):
        if key and (CREDENTIAL.search(key) or CODE.match(key)) and value:
            counts[f"credential {key}"] += 1
            drop(value, 6)
            return "REDACTED"
        if key and PLAN.search(key) and not SERVICE.search(key) and value:
            counts[f"field {key}"] += 1
            drop(value)
            return "REDACTED"
        return text(value, rules)
    return value


def identities(value, found, key=None):
    if key in IDS and isinstance(value, str) and value != IDS[key]:
        found[value] = IDS[key]
    if isinstance(value, dict):
        for name, item in value.items():
            identities(item, found, name)
    if isinstance(value, list):
        for item in value:
            identities(item, found)


def request_body(record):
    if not isinstance(record, dict):
        return record
    field = REQUEST_BODIES.get(record.get("type"))
    if field and record.get(field) not in (None, "REDACTED"):
        counts[f"request body {record['type']}"] += 1
        record = {**record, field: "REDACTED"}
    attachment = record.get("attachment")
    if record.get("type") == "attachment" and isinstance(attachment, dict) and attachment.get("type") != "queued_command":
        kept = {key: record[key] for key in CHAIN if key in record}
        kept["attachment"] = {"type": attachment.get("type")}
        if kept != record:
            counts[f"attachment body {attachment.get('type')}"] += 1
        record = kept
    return record


def home_names(run):
    shipped = set(SHIPPED)
    for _, folders, files in os.walk(run / "home"):
        shipped.update(folders)
        shipped.update(files)
    names = set()
    home = Path.home()
    for folder in (home / ".claude" / "skills", home / ".codex" / "skills"):
        if folder.is_dir():
            names.update(entry.name for entry in folder.iterdir() if not entry.name.startswith("."))
    try:
        plugins = json.loads((home / ".claude" / "plugins" / "installed_plugins.json").read_text())["plugins"]
        names.update(name.split("@")[0] for name in plugins)
    except (OSError, ValueError, KeyError, TypeError):
        pass
    try:
        names.update(json.loads((home / ".claude.json").read_text()).get("mcpServers", {}))
    except (OSError, ValueError, AttributeError):
        pass
    try:
        config = (home / ".codex" / "config.toml").read_text()
        names.update(re.findall(r"^\[mcp_servers\.([^\].]+)", config, re.MULTILINE))
    except OSError:
        pass
    return sorted(name for name in names - shipped if len(name) >= 6)


def run_of(raw):
    for parent in raw.parents:
        if parent.parent.name == "zz-tapes":
            return parent
    return raw.parent.parent if raw.parent.name == "tapes" else raw.parent


def main():
    if len(sys.argv) < 3 or len(sys.argv) % 2 == 0:
        print("usage: scrub.py <raw> <out> [<raw> <out> ...]", file=sys.stderr)
        return 2
    pairs = [(Path(raw).resolve(), Path(out)) for raw, out in zip(sys.argv[1::2], sys.argv[2::2])]
    run = run_of(pairs[0][0])
    login = getpass.getuser()
    hosts = host_names()
    branch, remote = git("branch", "--show-current"), git("remote", "get-url", "origin")
    records = {raw: [json.loads(line) for line in raw.read_text().splitlines() if line.strip()] for raw, _ in pairs}
    found = {}
    for lines in records.values():
        for record in lines:
            identities(record, found)
    rules = [("path " + new, old, new) for old, new in paths(run)]
    rules += [("account id", old, new) for old, new in found.items()]
    rules += [("host", name, "host") for name in hosts]
    rules += [("git remote", remote, "https://example.com/repo.git")] if remote else []
    rules += [("git branch", branch, "branch")] if len(branch) >= 4 else []
    rules.append(("login", login, "user"))
    removed.update(found)
    outputs = {}
    for raw, out in pairs:
        scrubbed = [walk(request_body(record), rules) for record in records[raw]]
        outputs[out] = "".join(json.dumps(record, separators=(",", ":"), ensure_ascii=False) + "\n" for record in scrubbed)
        out.write_text(outputs[out])
    for label, count in sorted(counts.items()):
        if count:
            print(f"{count:6} {label}")
    temp = os.environ.get("TMPDIR", "").rstrip("/")
    banned = {"login": [login], "home": [str(Path.home())], "host": hosts, "redacted field": sorted(removed)}
    banned["run"] = [str(run), str(run).replace("/private/tmp/", "/tmp/", 1), sanitized(str(run))]
    banned["run"].append(f"/tmp/zzt-{run.name.rsplit('-', 1)[-1]}.sock")
    banned["worktree"] = [str(ROOT), remote] + ([branch] if len(branch) >= 4 else [])
    banned["temp folder"] = [temp] if temp.startswith("/var/folders/") else []
    banned["time zone"] = [timezone()] if "/" in timezone() else []
    banned["home skill, plugin or MCP server"] = home_names(run)
    failed = 0
    for out, scrubbed in outputs.items():
        lowered = scrubbed.lower()
        held = [label for label, words in banned.items() if any(word.lower() in lowered for word in words if word)]
        if any(not match.group(0).endswith("@example.com") for match in EMAIL.finditer(scrubbed)):
            held.append("email")
        if DAEMON_SOCKET.search(scrubbed):
            held.append("codex daemon socket")
        if CONNECTORS.search(scrubbed):
            held.append("account connector")
        if held:
            print(f"check FAILED, {out} still holds: {', '.join(held)}", file=sys.stderr)
            failed = 1
        else:
            print(f"check passed: {out}, {scrubbed.count(chr(10))} lines")
    return failed


ROOT = Path(__file__).resolve().parents[3]

if __name__ == "__main__":
    sys.exit(main())
