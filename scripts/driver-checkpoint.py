"""Prototype for knowledge/designs/durable-driver-chat.md: resume a driver from its checkpoint.

driver-checkpoint.py chat TRANSCRIPT              the conversation layer as JSON lines
driver-checkpoint.py packet TRANSCRIPT|CHAT LEDGER  the first prompt of a restarted driver
driver-checkpoint.py stop-hook                    Claude Code Stop hook: no turn ends with a stale ledger
"""

import json
import pathlib
import re
import sys

LEDGER_WRITE = re.compile(r">>?\s*\S*ledger\.md|tee\s+(-a\s+)?\S*ledger\.md|sed\s+-i[^\n;|&]*ledger\.md|mv\s+\S+\s+\S*ledger\.md")


def entries(path):
    out = []
    for line in pathlib.Path(path).read_text().splitlines():
        try:
            out.append(json.loads(line))
        except ValueError:
            pass
    return out


def text_of(content):
    if isinstance(content, str):
        return content
    return "\n".join(b.get("text", "") for b in content or [] if isinstance(b, dict) and b.get("type") == "text")


def writes_ledger(block):
    data = block.get("input") or {}
    if block.get("name") in ("Write", "Edit") and str(data.get("file_path", "")).endswith("ledger.md"):
        return True
    return block.get("name") == "Bash" and bool(LEDGER_WRITE.search(data.get("command", "")))


def chat(path):
    """User prompts, worker notices and the driver's last text of each turn, with the checkpoints between them."""
    out, last_text = [], None
    for e in entries(path):
        message = e.get("message") or {}
        content = message.get("content")
        if e.get("type") == "assistant":
            for block in content or []:
                if block.get("type") == "text" and block.get("text", "").strip():
                    last_text = (e.get("timestamp"), block["text"].strip())
                elif block.get("type") == "tool_use" and writes_ledger(block):
                    out.append({"ts": e.get("timestamp"), "role": "checkpoint"})
        elif e.get("type") == "user" and (e.get("origin") or {}).get("kind") == "peer":
            if last_text:
                out.append({"ts": last_text[0], "role": "driver", "text": last_text[1]})
                last_text = None
            origin = e["origin"]
            out.append({"ts": e.get("timestamp"), "role": "peer", "text": f"from {origin.get('name', '?')}: {origin.get('body', '')}"})
        elif e.get("type") == "user" and not e.get("isMeta"):
            if isinstance(content, list) and any(b.get("type") == "tool_result" for b in content if isinstance(b, dict)):
                continue
            text = text_of(content).strip()
            if not text:
                continue
            if last_text:
                out.append({"ts": last_text[0], "role": "driver", "text": last_text[1]})
                last_text = None
            if text.startswith("<task-notification>"):
                summary = re.search(r"<summary>(.*?)</summary>", text, re.S)
                status = re.search(r"<status>(.*?)</status>", text)
                out.append({"ts": e.get("timestamp"), "role": "notice",
                            "text": f"{status.group(1) if status else ''}: {summary.group(1).strip() if summary else text[:300]}"})
            elif text.startswith("<cross-session-message"):
                out.append({"ts": e.get("timestamp"), "role": "notice", "text": re.sub(r"<[^>]+>", "", text).strip()[:1500]})
            else:
                out.append({"ts": e.get("timestamp"), "role": "user", "text": text})
    if last_text:
        out.append({"ts": last_text[0], "role": "driver", "text": last_text[1]})
    return out


def packet(path, ledger):
    first = next(iter(entries(path)), {})
    log = entries(path) if "role" in first else chat(path)
    marks = [i for i, item in enumerate(log) if item["role"] == "checkpoint"]
    tail = log[marks[-1] + 1:] if marks else log
    users = [item for item in log if item["role"] == "user"]
    if users and users[-1] not in tail:
        tail = [users[-1]] + tail
    lines = [
        "You are fabrico's driver and you were restarted: your previous process died, and with it every "
        "wait you had running. Follow the Restarted section of your instructions.",
        "",
        "Your ledger.md as you last wrote it:",
        "",
        pathlib.Path(ledger).read_text().strip() if pathlib.Path(ledger).exists() else "(no ledger.md)",
        "",
        "The conversation since then (fabrico's last request first):",
        "",
    ]
    for item in tail:
        if item["role"] != "checkpoint":
            lines.append(f"[{item['role']} {item['ts']}] {item['text'][:2000]}")
    return "\n".join(lines)


def stop_hook():
    event = json.load(sys.stdin)
    if event.get("stop_hook_active"):
        return
    log = entries(event["transcript_path"])
    last_user = max((i for i, e in enumerate(log) if e.get("type") == "user" and not e.get("isMeta")
                     and not (isinstance((e.get("message") or {}).get("content"), list)
                              and any(b.get("type") == "tool_result" for b in e["message"]["content"] if isinstance(b, dict)))),
                    default=-1)
    turn = [b for e in log[last_user + 1:] if e.get("type") == "assistant"
            for b in (e.get("message") or {}).get("content") or [] if b.get("type") == "tool_use"]
    if turn and not any(writes_ledger(b) for b in turn):
        print(json.dumps({"decision": "block", "reason":
                          "You acted this turn but did not rewrite ledger.md. Rewrite it whole (Goal, Jobs, "
                          "Decisions, Waiting on fabrico, Next) before you end the turn."}))


if __name__ == "__main__":
    mode = sys.argv[1]
    if mode == "chat":
        for item in chat(sys.argv[2]):
            print(json.dumps(item))
    elif mode == "packet":
        print(packet(sys.argv[2], sys.argv[3]))
    elif mode == "stop-hook":
        stop_hook()
