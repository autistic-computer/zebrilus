import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
RUST = ROOT / "target/debug/examples/cli_globals_vectors"
PROBE = TARGET / "cli-globals-probe"


class RustDebug:
    def __init__(self, text):
        self.text = text
        self.index = 0

    def space(self):
        while self.index < len(self.text) and self.text[self.index].isspace():
            self.index += 1

    def take(self, token):
        self.space()
        if self.text.startswith(token, self.index):
            self.index += len(token)
            return True
        return False

    def value(self):
        self.space()
        if self.take('"'):
            raw = bytearray()
            while self.text[self.index] != '"':
                ch = self.text[self.index]
                self.index += 1
                if ch != "\\":
                    raw.extend(ch.encode())
                    continue
                ch = self.text[self.index]
                self.index += 1
                if ch == "x":
                    raw.append(int(self.text[self.index:self.index + 2], 16))
                    self.index += 2
                elif ch == "u":
                    assert self.take("{")
                    end = self.text.index("}", self.index)
                    raw.extend(chr(int(self.text[self.index:end], 16)).encode())
                    self.index = end + 1
                else:
                    raw.extend({"n": b"\n", "r": b"\r", "t": b"\t", "0": b"\0", '"': b'"', "\\": b"\\"}[ch])
            self.index += 1
            annotation = re.match(r"<\d+ bytes>", self.text[self.index:])
            if annotation:
                self.index += len(annotation[0])
            return raw.decode("utf8", "replace")
        if self.take("["):
            result = []
            while not self.take("]"):
                result.append(self.value())
                if not self.take(","):
                    assert self.take("]")
                    break
            return result
        match = re.match(r"[\w-]+", self.text[self.index:])
        assert match, self.text[self.index:]
        word = match[0]
        self.index += len(word)
        if self.take("{"):
            result = {}
            while not self.take("}"):
                key = self.value()
                assert self.take(":")
                result[key] = self.value()
                if not self.take(","):
                    assert self.take("}")
                    break
            return result
        if self.take("("):
            result = {"tag": word, "value": self.value()}
            assert self.take(")")
            return result
        if word == "None":
            return None
        if word in ("true", "false"):
            return word == "true"
        return int(word) if word.isdigit() else word


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def blob(output, value):
    uint(output, len(value))
    output.extend(value)


def text(output, value):
    blob(output, value.encode())


def optional(output, value):
    output.append(value is not None)
    if value is not None:
        text(output, value["value"])


def words(output, values, raw=False):
    uint(output, len(values))
    for value in values:
        blob(output, bytes(value)) if raw else text(output, value)


def expected(records):
    output = bytearray()
    for record in records:
        words(output, record["cleaned"], raw=True)
        parsed = RustDebug(record["debug"]).value()
        if parsed["tag"] == "Err":
            error = parsed["value"]
            if error == "Usage":
                output.append(3)
                text(output, "usage: zz [-2CDhlNuVv] [-c shell-command] [-f file] [-L socket-name]\n            [-S socket-path] [-T features] [command [flags]]")
            else:
                output.append(1 if error["tag"] == "Message" else 2)
                text(output, error["value"])
            continue
        result = parsed["value"]
        output.append(0)
        blob(output, bytes(record["path"]))
        output.append({"Default": 0, "Path": 1, "Label": 2}[result["socket_source"]])
        optional(output, result["host"])
        words(output, result["remaining"])
        words(output, result["mux_config_files"])
        output.append(result["no_start_server"])
        output.append(result["control_mode"])
        optional(output, result["shell_command"])
        output.append(result["login_shell"])
        optional(output, result["early_output"])
        output.append(result["client_utf8"])
        words(output, result["client_features"])
    return bytes(output)


def vectors():
    cases = [[], ["--"], ["-"], [";"], ["-" + "C" * 260], ["--verbose"], ["--zz-verbose-log"], ["--zz-verbose-log=x"], ["--bootstrap-client-cwd", "--verbose"], ["--", "--verbose"], ["display-message", "-p", "--verbose"], ["display-message", "-p", "--", "--verbose"]]
    flags = "2qvuCDlNhVXø"
    for length in (1, 2):
        for parts in itertools.product(flags, repeat=length):
            for tail in ([], ["list-sessions"], ["--help"]):
                cases.append(["-" + "".join(parts), *tail])
    for flag in "cfLST":
        cases.append(["-" + flag])
        for value in ("", "value", "-x", "--", "世界", b"\xff"):
            for prefix in ("", "2u", "h", "C"):
                cases.append(["-" + prefix + flag, value, "list-sessions"])
                if isinstance(value, str):
                    cases.append(["-" + prefix + flag + value, "list-sessions"])
    for flag in ("--socket", "--host"):
        cases.append([flag])
        for value in ("", "value", "-x", "--", "世界", b"\xff"):
            cases.append([flag, value, "list-sessions"])
            if isinstance(value, str):
                cases.append([flag + "=" + value, "list-sessions"])
    for verb in ("daemon", "proxy", "app", "list-panes", "run-pane", "--version", "--help", "--kill-server"):
        for separator in ([], ["--"], ["ignored"]):
            cases.append([verb, *separator, "--socket", "/chosen", "--host", "foo"])
            cases.append([verb, *separator, "--socket=/chosen"])
    cases += [["-S", "first", "--socket=last", "list-sessions"], ["--socket=first", "-Slast", "list-sessions"], ["--host", "x", "-S", "s"], ["--host=-x", "-V"], ["-D", "-h"], ["-L", "../escape", "list-sessions"], ["-L", "/absolute", "list-sessions"], ["-L", "nested/name", "list-sessions"], ["--unknown"], ["-c", ""], ["-f", b"raw\xff", "display-message", b"raw\xff"]]
    return [[list(word.encode() if isinstance(word, str) else word) for word in case] for case in cases]


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--update-help", action="store_true")
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cargo", "build", "--quiet", "--locked", "-p", "zz-cli", "--example", "cli_globals_vectors"], cwd=ROOT, check=True)
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/cli_globals_probe.mojo"), "-o", str(PROBE)], cwd=ROOT, check=True)
    cases = vectors()
    feature_cases = [[], [""], ["sync,utf8", "sync@", "sync"], ["256:title@:RGB"], ["256,unknown,RGB", "mouse"], ["RGB", "rgb@", "RGB"], ["utf8"] * 17, ["x" * 201] * 20 + ["RGB"], ["RGB"] * 16 + ["RGB@"]]
    feature_cases += [["striKethrough"], ["ignorefKeys"], ["ſync"], ["STRİKETHROUGH"]]
    names = ["256", "appesc", "bpaste", "ccolour", "clipboard", "hyperlinks", "cstyle", "extkeys", "focus", "ignorefkeys", "margins", "mouse", "osc7", "overline", "progressbar", "rectfill", "RGB", "sixel", "strikethrough", "sync", "title", "usstyle", "utf8"]
    for name in names:
        for suffix in ("", "@", ",RGB", ":unknown,mouse", "@,RGB", "@@"):
            feature_cases.append([name.upper() + suffix, name.lower()])
    with tempfile.TemporaryDirectory(prefix="zz-cli-globals-", dir="/tmp") as directory:
        temporary = Path(directory)
        environment = os.environ.copy()
        environment["TMUX_TMPDIR"] = str(temporary)
        request = {"cases": cases, "features": feature_cases}
        result = json.loads(subprocess.run([str(RUST)], input=json.dumps(request).encode(), env=environment, check=True, capture_output=True).stdout)
        source = temporary / "source.bin"
        actual = temporary / "actual.bin"
        payload = bytearray()
        uint(payload, len(cases))
        for case in cases:
            words(payload, case, raw=True)
        source.write_bytes(payload)
        subprocess.run([str(PROBE), "parse", str(source), str(actual)], env=environment, check=True)
        wanted = expected(result["cases"])
        received = actual.read_bytes()
        if received != wanted:
            for index, (left, right) in enumerate(zip(received, wanted)):
                if left != right:
                    offset = 0
                    for case_index, record in enumerate(result["cases"]):
                        offset += len(expected([record]))
                        if index < offset:
                            raise AssertionError(f"CLI global parser differs in case {case_index} {cases[case_index]} at byte {index}: {left} != {right}; {record['debug']}")
                    raise AssertionError("CLI parser emitted unexpected bytes")
            raise AssertionError(f"CLI global parser byte lengths differ: {len(received)} != {len(wanted)}")
        payload = bytearray()
        uint(payload, len(feature_cases))
        for case in feature_cases:
            words(payload, case)
        source.write_bytes(payload)
        subprocess.run([str(PROBE), "features", str(source), str(actual)], env=environment, check=True)
        wanted = bytearray()
        for mask in result["masks"]:
            uint(wanted, mask)
        assert actual.read_bytes() == wanted, "native feature mask differs"
        filesystem = 0
        base = temporary / f"tmux-{os.getuid()}"

        def compare_label():
            nonlocal filesystem
            selected = [[list(b"-L"), list(b"scope"), list(b"list-sessions")]]
            reference = json.loads(subprocess.run([str(RUST)], input=json.dumps({"cases": selected, "features": []}).encode(), env=environment, capture_output=True, check=True).stdout)
            data = bytearray()
            uint(data, 1)
            words(data, selected[0], raw=True)
            source.write_bytes(data)
            subprocess.run([str(PROBE), "parse", str(source), str(actual)], env=environment, check=True)
            assert actual.read_bytes() == expected(reference["cases"]), (reference, actual.read_bytes())
            filesystem += 1

        for mode in (0o700, 0o770, 0o701, 0o707, 0o775):
            base.chmod(mode)
            compare_label()
        base.chmod(0o700)
        base.rmdir()
        base.write_text("private fixture")
        compare_label()
        base.unlink()
        target = temporary / "target"
        target.mkdir()
        base.symlink_to(target, target_is_directory=True)
        compare_label()
        base.unlink()
        base.mkdir(mode=0o700)
        defaults = 0
        for overrides in ({"ZZ_SOCKET": ""}, {"ZZ_SOCKET": "/explicit"}, {"XDG_RUNTIME_DIR": ""}, {"XDG_RUNTIME_DIR": "/runtime"}, {"USER": ""}, {"USER": "世界", "TMPDIR": str(temporary)}, {"USER": "user", "TMPDIR": ""}, {}):
            env = environment.copy()
            for name in ("ZZ_SOCKET", "XDG_RUNTIME_DIR", "USER", "TMPDIR"):
                env.pop(name, None)
            env.update(overrides)
            reference = json.loads(subprocess.run([str(RUST)], input=b'{"cases":[],"features":[]}', env=env, capture_output=True, check=True).stdout)
            subprocess.run([str(PROBE), "default", "-", str(actual)], env=env, check=True)
            assert actual.read_bytes() == bytes(reference["default_socket"]), (overrides, actual.read_bytes(), reference["default_socket"])
            defaults += 1
        help_text = subprocess.run([str(ROOT / "target/debug/zz_cli"), "--help"], env=environment, capture_output=True, check=True).stdout.decode()
        generated = "comptime CLI_HELP = " + json.dumps(help_text, ensure_ascii=False) + "\n"
        help_path = PORT / "zzmojo/cli_help.mojo"
        if options.update_help:
            help_path.write_text(generated)
        assert help_path.read_text() == generated, "regenerate CLI help from the source catalog with --update-help"
        report = {"passed": True, "parser_cases": len(cases), "features": len(feature_cases), "default_sockets": defaults, "filesystem_cases": filesystem, "help_sha256": hashlib.sha256(help_text.encode()).hexdigest(), "source": "actual public Rust application_arguments and diagnostics cleanup, terminal feature helper and default_socket_path"}
        (TARGET / "cli-globals-vectors.json").write_text(json.dumps({"report": report, "arguments": cases, "source": result}, indent=2) + "\n")
        print(json.dumps(report))


if __name__ == "__main__":
    main()
