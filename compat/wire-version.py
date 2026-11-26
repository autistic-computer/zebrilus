#!/usr/bin/env python3

import difflib
import re
import subprocess
import sys
from collections import Counter
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
VISIBILITY = re.compile(r"pub(?:\s*\([^)]*\))?\s+")
QUALIFIERS = re.compile(r"(?:default|const|async|unsafe|extern(?:\s+\"[^\"]*\")?)\s+(?=(?:fn|impl|trait|const|async|unsafe|extern)\b)")
NAMED = re.compile(r"(fn|struct|enum|union|trait|type|mod|static(?:\s+mut)?|const|macro_rules!)\s*([A-Za-z_][A-Za-z0-9_]*)")
BRACED = re.compile(r"(?:(?:fn|struct|enum|union|trait|impl|mod|extern)\b|macro_rules!|\w+!\s*\{)")
RESTRICTED = re.compile(r"\bpub\s*\((?:crate|super|self|in [^)]*)\)\s*")
MOD_GATE_FREE = re.compile(r"#\[(?:path\s*=.*|cfg\(test\))\]")


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True)


def version_in(text):
    found = VERSION.search(text)
    return int(found.group(1)) if found else None


def skip_literal(text, index):
    char = text[index]
    if char == "/" and text.startswith("//", index):
        end = text.find("\n", index)
        return len(text) if end < 0 else end
    if char == "/" and text.startswith("/*", index):
        depth = 0
        while index < len(text):
            if text.startswith("/*", index):
                depth += 1
                index += 2
            elif text.startswith("*/", index):
                depth -= 1
                index += 2
                if depth == 0:
                    return index
            else:
                index += 1
        return index
    raw = re.match(r"b?r(#*)\"", text[index:index + 260])
    if raw and (index == 0 or not (text[index - 1].isalnum() or text[index - 1] == "_")):
        closing = "\"" + raw.group(1)
        end = text.find(closing, index + raw.end())
        return len(text) if end < 0 else end + len(closing)
    if char == "\"" or (char == "b" and text.startswith("b\"", index)):
        index = text.index("\"", index) + 1
        while index < len(text) and text[index] != "\"":
            index += 2 if text[index] == "\\" else 1
        return index + 1
    if char == "'" or (char == "b" and text.startswith("b'", index)):
        start = text.index("'", index)
        if text[start + 1:start + 2] == "\\":
            end = start + 2
            while end < len(text) and text[end] != "'":
                end += 1
            return end + 1
        if text[start + 2:start + 3] == "'":
            return start + 3
        return start + 1
    return None


def split_items(text):
    items = []
    start = None
    depth = 0
    index = 0
    while index < len(text):
        char = text[index]
        if char in "/\"'br":
            previous = text[index - 1] if index else " "
            literal = None
            if char in "/\"'" or not (previous.isalnum() or previous == "_"):
                literal = skip_literal(text, index)
            if literal is not None and literal > index:
                if start is None and not text.startswith("//", index) and not text.startswith("/*", index):
                    start = index
                index = literal
                continue
        if char.isspace():
            index += 1
            continue
        if start is None:
            start = index
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
            if depth == 0:
                piece = text[start:index + 1]
                head = strip_attributes(piece)
                if char == "]" and piece.lstrip().startswith("#!") and not head:
                    items.append(piece)
                    start = None
                elif char == "}" and BRACED.match(strip_prefix(head)):
                    items.append(piece)
                    start = None
        elif char == ";" and depth == 0:
            items.append(text[start:index + 1])
            start = None
        index += 1
    if start is not None and text[start:].strip():
        items.append(text[start:])
    return items


def strip_attributes(piece):
    rest = piece
    while True:
        rest = re.sub(r"^\s*(?://[^\n]*\n|/\*.*?\*/)", "", rest, flags=re.S)
        stripped = rest.lstrip()
        if not stripped.startswith("#"):
            return stripped
        bracket = stripped.find("[")
        depth = 0
        index = bracket
        while index < len(stripped):
            literal = skip_literal(stripped, index) if stripped[index] in "\"'" else None
            if literal is not None and literal > index:
                index = literal
                continue
            if stripped[index] == "[":
                depth += 1
            elif stripped[index] == "]":
                depth -= 1
                if depth == 0:
                    break
            index += 1
        rest = stripped[index + 1:]


def strip_prefix(head):
    head = VISIBILITY.sub("", head, count=1) if head.startswith("pub") else head
    while True:
        trimmed = QUALIFIERS.sub("", head, count=1) if QUALIFIERS.match(head) else head
        if trimmed == head:
            return head
        head = trimmed


def attributes_of(piece):
    head = strip_attributes(piece)
    return piece[: len(piece) - len(head)]


def normalize(text):
    lines = []
    for line in text.splitlines():
        body = RESTRICTED.sub("", line.strip())
        if body and not body.startswith("//"):
            lines.append(body)
    return "\n".join(lines)


def tokens(text):
    found = []
    index = 0
    while index < len(text):
        char = text[index]
        previous = text[index - 1] if index else " "
        if char in "/\"'br" and (char in "/\"'" or not (previous.isalnum() or previous == "_")):
            literal = skip_literal(text, index)
            if literal is not None and literal > index:
                if not text.startswith("//", index) and not text.startswith("/*", index):
                    found.append(text[index:literal])
                index = literal
                continue
        if char.isspace():
            index += 1
        elif char.isalnum() or char == "_":
            end = index
            while end < len(text) and (text[end].isalnum() or text[end] == "_"):
                end += 1
            found.append(text[index:end])
            index = end
        else:
            found.append(char)
            index += 1
    return " ".join(found)


def body_bounds(piece):
    head = strip_attributes(piece)
    offset = len(piece) - len(head)
    index = 0
    depth = 0
    while index < len(head):
        literal = skip_literal(head, index) if head[index] in "/\"'" else None
        if literal is not None and literal > index:
            index = literal
            continue
        char = head[index]
        if char == "{" and depth == 0:
            return offset + index, len(piece) - 1
        if char in "([<":
            depth += 1
        elif char in ")]>" and not head[index - 1:index + 1] == "->":
            depth -= 1
        elif char == ";" and depth == 0:
            return None
        index += 1
    return None


def surface(text):
    found = []
    for piece in split_items(text):
        head = strip_prefix(strip_attributes(piece))
        attributes = attributes_of(piece)
        if not head or head.startswith("use ") or head.startswith("extern crate"):
            continue
        if head.startswith("impl") and re.match(r"impl\b", head):
            bounds = body_bounds(piece)
            header = " ".join(piece[len(attributes):bounds[0] if bounds else len(piece)].split())
            prefix = normalize(attributes) + ("\n" if attributes.strip() else "") + header
            members = surface(piece[bounds[0] + 1:bounds[1]]) if bounds else []
            if not members:
                found.append((("impl", header), normalize(piece)))
            for (kind, name), member in members:
                found.append((("impl " + header, kind, name), prefix + "\n" + member))
            continue
        named = NAMED.match(head)
        if named and named.group(1) == "mod":
            bounds = body_bounds(piece)
            gate = [line for line in normalize(attributes).splitlines() if not MOD_GATE_FREE.fullmatch(line)]
            if gate:
                found.append((("mod", named.group(2)), "\n".join(gate)))
            if bounds:
                found.extend(surface(piece[bounds[0] + 1:bounds[1]]))
            continue
        text_of = normalize(piece)
        if named and named.group(1) == "const" and REVISIONS.fullmatch(text_of):
            continue
        if named:
            found.append(((named.group(1).split()[0], named.group(2)), text_of))
        else:
            found.append((("item", text_of.splitlines()[0] if text_of else ""), text_of))
    return found


def changes(before_files, after_files):
    before = {}
    after = {}
    homes = {}
    shown = {}
    for side, files in ((before, before_files), (after, after_files)):
        for path, text in files.items():
            for key, item in surface(text):
                form = tokens(item)
                side.setdefault(key, Counter())[form] += 1
                homes.setdefault((key, form), set()).add(path)
                shown[form] = item
    flagged = sorted((key for key in before.keys() | after.keys()
                      if before.get(key, Counter()) != after.get(key, Counter())), key=repr)
    lines = []
    dirty = set()
    for key in flagged:
        was = before.get(key, Counter())
        now = after.get(key, Counter())
        for item in (was - now) | (now - was):
            dirty |= homes[(key, item)]
        old = "\n".join(sorted(shown[form] for form in was.elements())).splitlines()
        new = "\n".join(sorted(shown[form] for form in now.elements())).splitlines()
        for line in difflib.unified_diff(old, new, n=0):
            if line[:1] in ("+", "-") and line[:3] not in ("+++", "---"):
                lines.append(line)
    return sorted(dirty), lines


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
    before_files = {}
    after_files = {}
    for path in watched:
        before = git(root, "show", f"{tag}:{path}") if path in tagged else ""
        after = (root / path).read_text(encoding="utf-8") if path in present else ""
        text_only = path == MENTION_TEXT or (path == FILE_HEX and path not in tagged)
        if text_only and not (WIRE_CODE.search(before) or WIRE_CODE.search(after)):
            continue
        before_files[path] = before
        after_files[path] = after
    dirty, changed = changes(before_files, after_files)
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
