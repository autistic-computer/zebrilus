import argparse
import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
DATA = PORT / "zzmojo/cli_catalog.mojo"
ORACLE = ROOT / "target/debug/examples/cli_preflight_vectors"


def oracle(cases):
    result = subprocess.run([str(ORACLE)], input=json.dumps({"cases": cases}).encode(), capture_output=True, check=True, timeout=60)
    return json.loads(result.stdout)


def catalog_text(catalog):
    lines = []
    for spec in catalog:
        options = []
        for option in spec["options"]:
            flags = int(option["value"]) | int(option["attached"]) << 1 | int(option["optional"]) << 2 | int(option["native"]) << 3
            options.append(option["name"] + ":" + str(flags))
        fields = (spec["name"], ",".join(spec["aliases"]), str(int(spec["native"])), str(spec["minimum"]), str(spec["maximum"] if spec["maximum"] is not None else -1), spec["usage"], ",".join(options))
        assert all("\t" not in field and "\n" not in field and '"' not in field and "\\" not in field for field in fields)
        lines.append("\t".join(fields))
    return 'comptime CLI_CATALOG = """' + "\n".join(lines) + '"""\n'


def command(name, *args):
    return {"name": name, "args": [list(arg.encode() if isinstance(arg, str) else arg) for arg in args]}


def vectors(catalog):
    cases = [[]]
    names = {"", "does-not-exist", "__zz-command-alias-group", "🔎", "DISPLAY", "new_session"}
    for spec in catalog:
        name = spec["name"]
        names.update(spec["aliases"])
        names.update(name[:index] for index in range(1, len(name) + 1))
        for args in [[], ["--"], ["-"], ["-?"], ["-Q"], ["--no-such-flag"], ["-é"], [b"-\xff"], [b"-\0"], ["-1"], ["--", "-Q"], [b"raw\xff\0"], ["x", "-Q"], ["x", "y", "z"], ["--json", "-F", "#{pane_id}"], ["-F", "x", "--json"]]:
            cases.append([command(name, *args)])
        for option in spec["options"]:
            flag = option["name"]
            candidates = [[flag], [flag, "value"], [flag, "-Q"], [flag, "--"], [flag, "-1"], [flag + "value"], [flag + "=value"], [flag + "="], [flag, b"\xff\0"]]
            if len(flag) == 2:
                candidates.extend([["-" + flag[1] * 2], ["-" + flag[1] + "?"], ["-" + flag[1] + "Q"], ["-" + flag[1] + "-"], ["-" + flag[1] + "é"]])
            for args in candidates:
                cases.append([command(name, *args)])
        for length in range(0, min(7, (spec["maximum"] or 3) + 2)):
            cases.append([command(name, *[str(index) for index in range(length)])])
    cases.extend([command(name)] for name in sorted(names))
    cases.extend([[command("new-session", "-s", "before"), bad] for bad in [command("does-not-exist"), command("display", "-Q"), command("list-sessions", "x"), command("kill-s"), command("display-message", "-t")]])
    cases.extend([[command("new-session"), command("display-message", b"\xff\0")], [command("new-session"), command("inspect", "--anything"), command("link-window", "-t", "@1")]])
    for name in ("display-message", "new-session", "list-sessions", "run-shell", "set-hook", "inspect"):
        cases.extend([command(name, b"-" + bytes([value]))] for value in range(256))
        for raw in (b"-\xc0\xaf", b"-\xf0\x80\x80", b"-\xe2\x82", b"-\xf4\x90\x80\x80", b"-\xed\xa0\x80"):
            cases.append([command(name, raw)])
    unique = {json.dumps(case, sort_keys=True): case for case in cases}
    return list(unique.values())


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    value = value.encode() if isinstance(value, str) else bytes(value)
    uint(output, len(value))
    output.extend(value)


def encoded(cases):
    output = bytearray()
    uint(output, len(cases))
    for case in cases:
        uint(output, len(case))
        for invocation in case:
            text(output, invocation["name"])
            uint(output, len(invocation["args"]))
            for arg in invocation["args"]:
                text(output, arg)
    return output


def expected(results):
    output = bytearray()
    for result in results:
        text(output, result["canonical"])
        output.extend((int(result["starts_server"]), result["code"], int(result["error"] is not None)))
        if result["error"] is not None:
            text(output, result["error"])
    return output


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--generate", action="store_true")
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cargo", "build", "--quiet", "--locked", "--offline", "-p", "zz-mux", "--example", "cli_preflight_vectors", "--config", "profile.dev.debug=0", "--config", 'profile.dev.package."*".opt-level=0'], cwd=ROOT, check=True, timeout=300)
    catalog = oracle([])["catalog"]
    generated = catalog_text(catalog)
    if args.generate:
        DATA.write_text(generated)
        print(json.dumps({"catalog": len(catalog), "bytes": len(generated)}))
        return
    assert DATA.read_text() == generated, "CLI catalog diverged from actual Rust metadata; regenerate explicitly"
    cases = vectors(catalog)
    results = oracle(cases)["cases"]
    source = TARGET / "cli-preflight-input.bin"
    actual = TARGET / "cli-preflight-output.bin"
    source.write_bytes(encoded(cases))
    wanted = expected(results)
    probe = TARGET / "cli-preflight-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/cli_preflight_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(probe), str(source), str(actual)], check=True, timeout=120)
    found = actual.read_bytes()
    if found != wanted:
        for index, (left, right) in enumerate(zip(found, wanted)):
            if left != right:
                raise AssertionError(f"preflight byte {index}: {found[max(0,index-100):index+150]!r} != {wanted[max(0,index-100):index+150]!r}")
        raise AssertionError(f"preflight lengths {len(found)} != {len(wanted)}")
    report = {"passed": True, "catalog": len(catalog), "cases": len(cases), "accepted": sum(result["error"] is None for result in results), "bytes": len(wanted), "input_bytes": source.stat().st_size, "reference": "actual Rust validate_static_command_chain, command resolution, native startup policy, option parser and arity metadata", "scope": "ordinary raw CLI invocations with no typed config blocks; exact messages/status, every catalog name/alias/prefix and every option, attached/cluster/optional/native flags, end-of-options, raw bytes and later chain failures"}
    (TARGET / "cli-preflight-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
