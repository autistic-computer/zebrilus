import argparse
import bisect
import copy
import json
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
ORACLE = ROOT / "target/debug/examples/palette_place_vectors"
PROBE = TARGET / "palette-places-probe"


def uint(output, value):
    while value >= 128:
        output.append(value & 127 | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    value = value.encode()
    uint(output, len(value))
    output.extend(value)


def optional(output, value, writer=uint):
    uint(output, value is not None)
    if value is not None:
        writer(output, value)


def place(output, value):
    for part in value:
        optional(output, part)


def packet(fleets, cases):
    output = bytearray()
    uint(output, len(fleets))
    for here, hosts in fleets:
        uint(output, here)
        uint(output, len(hosts))
        for identity, name, online, current, sessions in hosts:
            uint(output, identity)
            text(output, name)
            uint(output, online)
            optional(output, current)
            uint(output, len(sessions))
            for identity, name, label, windows in sessions:
                uint(output, identity)
                text(output, name)
                text(output, label)
                uint(output, len(windows))
                for identity, index, name, active, active_pane, panes in windows:
                    for number in (identity, index):
                        uint(output, number)
                    text(output, name)
                    uint(output, active)
                    uint(output, active_pane)
                    uint(output, len(panes))
                    for identity, index, title, program, cwd, terminal, needs in panes:
                        uint(output, identity)
                        uint(output, index)
                        text(output, title)
                        optional(output, program, text)
                        text(output, cwd)
                        uint(output, terminal)
                        uint(output, needs)
    uint(output, len(cases))
    for case in cases:
        uint(output, case["tag"])
        uint(output, case["fleet"])
        place(output, case["place"])
        uint(output, case["level"])
        text(output, case["query"])
        text(output, case["target"])
        text(output, case["command"])
        uint(output, len(case["boosts"]))
        for key, score in case["boosts"]:
            place(output, key)
            uint(output, score * 2 if score >= 0 else -score * 2 - 1)
    return output


def catalog_source(catalog):
    lines = ["\t".join((name, ",".join(aliases), str(code), str(minimum), str(int(single)), label, ".")) for name, aliases, code, minimum, single, label in catalog]
    return '''comptime PLACE_TARGET_CATALOG = """''' + "\n".join(lines) + '''"""


@fieldwise_init
struct PaletteTargetTraits(Copyable, Movable):
    var name: String
    var supported: Bool
    var kind: Optional[UInt8]
    var minimum: Int
    var single: Bool
    var text_label: String


def target_traits(name: String) raises -> Optional[PaletteTargetTraits]:
    for line in String(PLACE_TARGET_CATALOG).split("\\n"):
        var fields = line.split("\\t")
        if len(fields) != 7 or not (fields[6] == "."):
            raise Error("invalid palette target metadata")
        var matching = fields[0] == name
        for alias in fields[1].split(","):
            matching = matching or alias.byte_length() > 0 and alias == name
        if matching:
            var code = Int(fields[2])
            var kind: Optional[UInt8] = None
            if code >= 2:
                kind = UInt8(code - 2)
            return PaletteTargetTraits(String(fields[0]), code != 0, kind, Int(fields[3]), fields[4] == "1", String(fields[5]))
    return None
'''


def fixtures():
    hosts = [
        [0, "local", True, 0, [
            [0, "work", "Work µ", [
                [10, 2, "editor", True, 100, [[100, 0, "  λShell  ", "bash", "/tmp", True, False], [101, 1, "\u2003", "python", "/src", True, True], [102, 7, "Term İ", None, "", False, False]]],
                [11, 8, "logs", False, 110, [[110, 0, "", None, "", True, False]]],
            ]],
            [1, "08", "Number eight", [[12, 0, "2", False, 120, [[120, 4, "cargo", "rust", "/repo", True, False]]]]],
            [2, "%404", "Reserved looking", []],
            [3, "space name", "session renamed", []],
        ]],
        [1, "offline", False, 0, [[0, "work", "Other work", [[10, 2, "editor", False, 100, [[100, 0, "offline pane", "ssh", "~", True, True]]]]]]],
        [2, "ÅHost", True, 18446744073709551615, [[18446744073709551615, "$huge", "Remote i\u0307", [[18446744073709551615, 4294967295, "远程", True, 18446744073709551615, [[18446744073709551615, 4294967295, "界\u00a0界", "", "/🌱", True, False]]]]]]],
        [3, "empty", True, None, []],
    ]
    fleets = [[0, []], [0, hosts], [1, copy.deepcopy(hosts)], [99, copy.deepcopy(hosts)], [0, copy.deepcopy(hosts)]]
    fleets[4][1][0][3] = 999
    scopes = [[None, None, None, None], [0, None, None, None], [0, 0, None, None], [0, 0, 10, None], [0, 0, 10, 100], [0, 1, None, None], [1, 0, 10, 100], [2, 18446744073709551615, 18446744073709551615, 18446744073709551615], [0, 999, 404, None], [None, 0, 10, None], [3, None, None, None]]
    queries = ["", " ", "0", "00", "1", "2", "08", "8", "4294967295", "4294967296", "WORK", "w µ", "Work 2", "Work cargo", "python", "ssh", "λ", "İ", "i\u0307", "界", "\u00a0\t", "not found", "\0"]
    cases = []

    def add(tag, fleet=1, scope=None, level=0, query="", target="", command="", boosts=None):
        cases.append({"tag": tag, "fleet": fleet, "place": [None] * 4 if scope is None else scope, "level": level, "query": query, "target": target, "command": command, "boosts": [] if boosts is None else boosts})

    for fleet in range(len(fleets)):
        add(6, fleet)
        for scope in scopes:
            add(2, fleet, scope)
            add(7, fleet, scope)
            add(4, fleet, scope)
            for query in queries:
                for level in range(4):
                    add(3, fleet, scope, level, query)
                add(5, fleet, scope, query=query)
    for query in queries + [":", " : select-pane -t %1 ", "@:2", "@", "\u3000:display-message x\u00a0"]:
        add(0, query=query)
    for text_value in ["@local", " @local ", "@offline:%100", "@local:%100", "@local:%+100", "@local:%404", "@local:@10", "@local:$0", "@local:work:2.1", "@local:work:editor.%101", "@local:space\\ name", "@local:space name", "@local:~/a\\ b:42:7", "@local:/etc/file:4294967296:2", "@local:/etc/file:2:4294967296", "@local:/etc/:", "@ÅHost:%18446744073709551615", "@ÅHost:@18446744073709551615", "@unknown:%1", "@local:\n", "@local:work:2.0", "@local:work:2.%+100", "@local:\\", "@local:~/end\\", "@empty", "@local:work:2:extra"]:
        for fleet in range(len(fleets)):
            add(1, fleet, query=text_value)
    for query in ["", "   ", "select-pane", " select-pane  -Z", "select-pane -t%1", "select-pane -lZt %1", "select-pane --topic", "select-pane -t\"x\"", "select-pane -st2", "select-pane ; other -t %1", "select-pane \\; other -t %1", "select-pane -tλ", "select-pane -ẗ", "select-pane\u00a0-Z", "select-pane -Zt"]:
        for target in ["%1", "@42", "$7", "name space"]:
            add(8, query=query, target=target)
    boosts = [[scopes[4], 12], [scopes[6], -3], [scopes[7], 7], [scopes[4], 2]]
    for scope in scopes:
        for query in ["", "0", "work", "ssh"]:
            for level in range(4):
                add(3, scope=scope, level=level, query=query, boosts=boosts)
            add(5, scope=scope, query=query, boosts=boosts)
    return fleets, cases, scopes


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--generate", action="store_true")
    options = parser.parse_args()
    subprocess.run(["cargo", "build", "--offline", "-j", "1", "-p", "zz-client", "--example", "palette_place_vectors"], cwd=ROOT, check=True)
    fleets, cases, scopes = fixtures()
    catalog = json.loads(subprocess.check_output([str(ORACLE)], input=json.dumps({"fleets": [], "cases": []}).encode()))["catalog"]
    for name, aliases, *_ in catalog:
        for command in [name, *aliases]:
            for scope in scopes:
                cases.append({"tag": 9, "fleet": 1, "place": scope, "level": 0, "query": "", "target": "", "command": command, "boosts": []})
    reference = json.loads(subprocess.check_output([str(ORACLE)], input=json.dumps({"fleets": fleets, "cases": cases}).encode()))
    generated = catalog_source(reference["catalog"])
    destination = PORT / "zzmojo/palette_target_catalog.mojo"
    if options.generate:
        destination.write_text(generated)
    assert destination.read_text() == generated, "palette target metadata changed; regenerate with --generate"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), "-I", str(PORT / "tests"), str(PORT / "tests/palette_places_probe.mojo"), "-o", str(PROBE)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-palette-places-", dir="/tmp") as directory:
        source = Path(directory) / "in"
        target = Path(directory) / "out"
        source.write_bytes(packet(fleets, cases))
        subprocess.run([str(PROBE), str(source), str(target)], check=True)
        actual = target.read_bytes()
        expected = bytes(reference["bytes"])
        if actual != expected:
            offset = next((index for index in range(min(len(actual), len(expected))) if actual[index] != expected[index]), min(len(actual), len(expected)))
            index = bisect.bisect_right(reference["ends"], offset)
            raise AssertionError((offset, cases[index], actual[max(0, offset - 20):offset + 60].hex(), expected[max(0, offset - 20):offset + 60].hex()))
    report = {"passed": True, "cases": len(cases), "bytes": len(expected), "reference": "actual zz-client place helpers", "scope": "Unicode trim, current/landing place, written places/paths/line suffixes, target injection, full catalog target policy, ranked candidates/children/search, scopes/numeric queries/offline hosts/boosts and stable ties"}
    (TARGET / "palette-places-vectors.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
