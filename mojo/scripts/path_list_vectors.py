import hashlib
import json
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    value = value.encode()
    uint(output, len(value))
    output.extend(value)


def optional(output, value, encode):
    output.append(int(value is not None))
    if value is not None:
        encode(output, value)


def style(value):
    if value == "Claude":
        return 4
    if value == "Codex":
        return 5
    return ("Posix", "Fish", "Pwsh", "Nu").index(value["Shell"])


def arguments(message):
    name, data = next(iter(message.items()))
    kind = ("PathListRequest", "PathListBegin", "PathListChunk", "PathListGit", "PathListCancel", "Event").index(name)
    request = data.get("request_id", 0)
    sequence = 0
    if kind == 5:
        sequence = data["sequence"]
        data = data["payload"]["OpenPathPicker"]
    output = bytearray([kind])
    uint(output, request)
    optional(output, data.get("pane"), uint)
    optional(output, data.get("dir", data.get("start_dir")), text)
    root = data.get("result", {}).get("Ok")
    output.append(int(root is not None))
    if root is not None:
        text(output, root["root"])
        text(output, root["display_root"])
        optional(output, root["cwd"], text)
        uint(output, style(root["insert"]))
    optional(output, data.get("result", {}).get("Err"), text)
    entries = data.get("entries", [])
    uint(output, len(entries))
    for entry in entries:
        text(output, entry["rel"])
        output.extend([int(entry["kind"] == "Dir"), int(entry["symlink"])])
    marks = data.get("marks", [])
    uint(output, len(marks))
    for rel, mark in marks:
        text(output, rel)
        output.append(("Modified", "Added", "Untracked", "Conflicted").index(mark))
    output.extend([int(data.get("done", False)), int(data.get("truncated", False))])
    uint(output, sequence)
    return output


def root_bytes(output, root):
    output.append(int(root is not None))
    if root is not None:
        text(output, root["root"])
        text(output, root["display_root"])
        optional(output, root["cwd"], text)
        output.append(style(root["insert"]))


def listing_vectors():
    source = (ROOT / "crates/zz-ui/src/files.rs").read_text()
    begin = source.index("#[derive(Clone, Debug, Default)]\npub struct GitMarks")
    end = source.index("#[derive(Clone, Debug, PartialEq, Eq)]\npub struct FileRow", begin)
    extracted = source[begin:end]
    if (PORT / "scripts/path_listing_reference.rs").read_text() != extracted:
        raise AssertionError("retained listing reference no longer equals the current native source")
    result = subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-p", "zz-protocol", "--example", "path_listing_vectors"], cwd=ROOT, check=True, stdout=subprocess.PIPE)
    vectors = json.loads(result.stdout)
    vectors["source_sha256"] = hashlib.sha256(extracted.encode()).hexdigest()
    (TARGET / "path-listing-vectors.json").write_text(json.dumps(vectors, indent=2) + "\n")
    return vectors


def listing_input(vectors, frames):
    request = bytearray()
    expected = bytearray()
    uint(request, len(vectors["traces"]))
    for trace in vectors["traces"]:
        text(request, trace["host"])
        text(request, trace["dir"])
        uint(request, trace["request"])
        for collection in [trace["lookups"], trace["dirs"], ["here", "there"]]:
            uint(request, len(collection))
            for value in collection:
                text(request, value)
        uint(request, len(trace["updates"]))
        for message, state in zip(trace["updates"], trace["states"], strict=True):
            frame = frames[json.dumps(message, sort_keys=True)]
            uint(request, len(frame))
            request.extend(frame)
            expected.append(int(state["changed"]))
            root_bytes(expected, state["root"])
            uint(expected, len(state["chunks"]))
            for chunk in state["chunks"]:
                uint(expected, len(chunk))
                for entry in chunk:
                    text(expected, entry["rel"])
                    expected.extend([int(entry["kind"] == "Dir"), int(entry["symlink"])])
            for mark, letter in zip(state["marks"], state["letters"], strict=True):
                optional(expected, None if mark is None else ("Modified", "Added", "Untracked", "Conflicted").index(mark), uint)
                optional(expected, letter, text)
            optional(expected, state["error"], text)
            expected.extend([int(state["done"]), int(state["truncated"]), int(state["empty"])])
            expected.extend(map(int, state["covers"]))
    return request, expected


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    reference = subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-p", "zz-protocol", "--example", "path_list_vectors"], cwd=ROOT, check=True, stdout=subprocess.PIPE)
    vectors = json.loads(reference.stdout)
    (TARGET / "path-list-wire-vectors.json").write_text(json.dumps(vectors, indent=2) + "\n")
    listings = listing_vectors()
    probe = TARGET / "path-list-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/path_list_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True)
    accepted = rejected = encoders = 0
    with tempfile.TemporaryDirectory(prefix="zzpath-vectors-") as directory:
        source = Path(directory) / "source.bin"
        actual = Path(directory) / "actual.bin"
        frames = {}
        for case in vectors["cases"]:
            source.write_bytes(bytes(case["bytes"]))
            result = subprocess.run([str(probe), "roundtrip", str(source), str(actual)], capture_output=True)
            if (result.returncode == 0) != case["accepted"]:
                raise AssertionError("path decode acceptance differs: " + case["name"] + result.stderr.decode())
            if case["accepted"]:
                accepted += 1
                if actual.read_bytes() != bytes(case["canonical"]):
                    raise AssertionError("path roundtrip differs: " + case["name"])
            else:
                rejected += 1
            if "message" in case:
                source.write_bytes(arguments(case["message"]))
                result = subprocess.run([str(probe), "build", str(source), str(actual)], capture_output=True)
                if (result.returncode == 0) != case["accepted"]:
                    raise AssertionError("path encoder acceptance differs: " + case["name"] + result.stderr.decode())
                if case["accepted"]:
                    if actual.read_bytes() != bytes(case["canonical"]):
                        raise AssertionError("path encoder differs: " + case["name"])
                    encoders += 1
        for trace in listings["traces"]:
            for message in trace["updates"]:
                key = json.dumps(message, sort_keys=True)
                if key not in frames:
                    source.write_bytes(arguments(message))
                    subprocess.run([str(probe), "build", str(source), str(actual)], check=True)
                    frames[key] = actual.read_bytes()
        request, expected = listing_input(listings, frames)
        source.write_bytes(request)
        subprocess.run([str(probe), "listing", str(source), str(actual)], check=True)
        if actual.read_bytes() != expected:
            raise AssertionError("retained listing trace differs from actual source")
        for hello in vectors["hellos"]:
            request = bytearray([int(hello["search"]), int(hello["paths"])])
            optional(request, hello["origin"], uint)
            source.write_bytes(request)
            subprocess.run([str(probe), "hello", str(source), str(actual)], check=True)
            if actual.read_bytes() != bytes(hello["bytes"]):
                raise AssertionError("path capability changed the native Hello encoding")
    print(json.dumps({"source": "actual ProtocolMessage serializer/decoder and verbatim Listing/GitMarks source", "accepted": accepted, "rejected": rejected, "encoders": encoders, "hello_capabilities": len(vectors["hellos"]), "listing_traces": len(listings["traces"]), "listing_steps": sum(len(trace["states"]) for trace in listings["traces"]), "source_sha256": listings["source_sha256"]}))


if __name__ == "__main__":
    main()
