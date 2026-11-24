#!/usr/bin/env python3

import difflib
import re
import subprocess
import sys
from pathlib import Path

WIRE_DIR = "crates/zz-protocol/src"
NOT_WIRE = {f"{WIRE_DIR}/catalog.rs", f"{WIRE_DIR}/lib.rs"}
MENTION_TEXT = f"{WIRE_DIR}/agent_mention.rs"
FILE_HEX = f"{WIRE_DIR}/file_hex.rs"
WIRE_CODE = re.compile(
    r"\b(?:serde(?:_\w+)?|(?i:(?:de)?serializ\w*)|postcard|ProtocolMessage|"
    r"EventPayload|framing|pane_frame|terminal_codec|\w*varint\w*|(?:to|from)_(?:le|be)_bytes)\b"
)
VERSION = re.compile(r"pub const PROTOCOL_VERSION: u16 = (\d+);")
REVISIONS = re.compile(r"pub const (?:MIN_DAEMON_|MIN_CLIENT_)?REVISION: u32 = \d+;")


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True)


def version_in(text):
    found = VERSION.search(text)
    return int(found.group(1)) if found else None


def meaningful(before, after):
    lines = []
    for line in difflib.unified_diff(before.splitlines(), after.splitlines(), n=0):
        if line[:1] not in ("+", "-") or line[:3] in ("+++", "---"):
            continue
        body = line[1:].strip()
        if not body or body.startswith("//") or REVISIONS.fullmatch(body):
            continue
        lines.append(line)
    return lines


def main():
    root = Path(__file__).resolve().parent.parent
    tags = [t for t in git(root, "tag", "--list", "v*", "--sort=-v:refname").split() if t]
    if not tags:
        print("wire-version: no release tag to compare against")
        return 0
    tag = tags[0]

    version_file = f"{WIRE_DIR}/message.rs"
    released = version_in(git(root, "show", f"{tag}:{version_file}"))
    here = version_in((root / version_file).read_text(encoding="utf-8"))
    if released is None or here is None:
        print(f"wire-version: cannot read PROTOCOL_VERSION at {tag} or in the tree")
        return 1

    if here > released:
        print(f"wire-version: {here} is unreleased ({tag} shipped {released})")
        return 0
    if here < released:
        print(f"wire-version: the tree says {here} but {tag} shipped {released}")
        return 1

    tagged = set(git(root, "ls-tree", "-r", "--name-only", tag, "--", WIRE_DIR).splitlines())
    present = {p.relative_to(root).as_posix() for p in (root / WIRE_DIR).rglob("*.rs")}
    watched = sorted(p for p in (tagged | present) - NOT_WIRE if p.endswith(".rs"))
    changed = []
    dirty = []
    for path in watched:
        before = git(root, "show", f"{tag}:{path}") if path in tagged else ""
        after = (root / path).read_text(encoding="utf-8") if path in present else ""
        if before == after:
            continue
        text_only = path == MENTION_TEXT or (path == FILE_HEX and path not in tagged)
        if text_only and not (WIRE_CODE.search(before) or WIRE_CODE.search(after)):
            continue
        lines = meaningful(before, after)
        if lines:
            dirty.append(path)
            changed.extend(lines)
    if not changed:
        print(f"wire-version: {here} matches {tag}; no wire changes flagged "
              f"({len(watched)} sources watched)")
        return 0

    print(f"wire-version: {', '.join(dirty)} changed since {tag}, which shipped "
          f"PROTOCOL_VERSION {here},")
    print(f"  and the tree still says {here}. Review these source changes for wire compatibility.")
    print("  Incompatible payload or packing changes require a new PROTOCOL_VERSION.")
    print("  Capability-gated additions to ProtocolMessage or EventPayload can keep the epoch.")
    print("  Check sender and receiver gates, or prove unchanged encoding for a shape-neutral edit.")
    print("  See knowledge/protocol/wire-protocol.md for the procedure.")
    print(f"  {len(changed)} changed line(s), first few:")
    for line in changed[:6]:
        print(f"    {line}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
