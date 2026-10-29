#!/usr/bin/env python3

import re
import subprocess
import sys
from pathlib import Path

WIRE_DIR = "crates/zz-protocol/src"
NOT_WIRE = {"catalog.rs", "lib.rs"}
VERSION = re.compile(r"pub const PROTOCOL_VERSION: u16 = (\d+);")


def git(root, *args):
    out = subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True)
    return out.stdout


def wire_files(root):
    here = sorted(
        f"{WIRE_DIR}/{p.name}"
        for p in (root / WIRE_DIR).glob("*.rs")
        if p.name not in NOT_WIRE
    )
    return here


def version_in(text):
    found = VERSION.search(text)
    return int(found.group(1)) if found else None


def meaningful(diff):
    lines = []
    for line in diff.splitlines():
        if line[:1] not in ("+", "-") or line[:3] in ("+++", "---"):
            continue
        body = line[1:].strip()
        if not body or body.startswith("//"):
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
        print(f"wire-version: {here} is unreleased ({tag} shipped {released}); appends are free")
        return 0
    if here < released:
        print(f"wire-version: the tree says {here} but {tag} shipped {released}")
        return 1

    watched = wire_files(root)
    changed = []
    dirty = []
    for path in watched:
        lines = meaningful(git(root, "diff", tag, "--", path))
        if lines:
            dirty.append(path)
            changed.extend(lines)
    if not changed:
        print(f"wire-version: {here} matches {tag} and the wire is unchanged "
              f"({len(watched)} sources watched)")
        return 0

    print(f"wire-version: {', '.join(dirty)} changed since {tag}, which shipped "
          f"PROTOCOL_VERSION {here},")
    print(f"  but the tree still says {here}. A released version cannot take appends: two builds")
    print(f"  would both claim {here} and disagree about the bytes.")
    print(f"  Set PROTOCOL_VERSION to {here + 1}, move this cycle's appends into a v{here + 1}")
    print(f"  entry in knowledge/protocol/wire-protocol.md's version history, and update the two")
    print(f"  assertions that pin the number (crates/zz-protocol/src/message.rs and")
    print(f"  crates/zz-protocol/tests/hunt_claims.rs).")
    print(f"  If the change is provably shape-neutral - a #[serde(skip)] field, an impl block, or a")
    print(f"  #[cfg(test)] module - it does not need the bump. This guard reads the diff, not the")
    print(f"  encoding, so prove it with a round-trip test asserting the serialized bytes equal those")
    print(f"  of a value without the change, the way CommandInvocation::stdin_spent does, and say so.")
    print(f"  {len(changed)} changed line(s), first few:")
    for line in changed[:6]:
        print(f"    {line}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
