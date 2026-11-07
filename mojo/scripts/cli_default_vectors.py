import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    data = value.encode() if isinstance(value, str) else bytes(value)
    uint(output, len(data))
    output.extend(data)


def invocation(output, command):
    text(output, command["name"])
    uint(output, len(command["args"]))
    for argument in command["args"]:
        text(output, argument)
    uint(output, int(command["source"] is not None))
    if command["source"] is not None:
        text(output, command["source"]["source"])
        uint(output, command["source"]["line"])
        uint(output, command["source"]["column"])
    uint(output, len(command["blocks"]))
    for index in command["blocks"]:
        uint(output, index)
    output.extend(b"\0\0\0")


def vectors():
    inputs = ["", " ", "new-session", "new-session -A", " a b c", "cmd 'unterminated", 'cmd "unterminated', "cmd ${}", "cmd $", "cmd ${", "cmd ${a-b}", "A=one cmd $A ${A}", "A=one; B=$A-$A; cmd $B", "A=one B=two cmd", "%hidden SECRET=one; cmd $SECRET", "%hidden", "%if 1 yes; %else no; %endif", "%if #{==:a, b} yes; %else no; %endif", "%if 1; %if 2; a; %else; b; %endif; %else; c; %endif", "%elif x", "%else", "%endif", "%if x", "%if x; %else; %else; %endif", "if-shell x { a; b } { c }", "run-shell { a { b } c }", "{ command }", "run-shell { #comment", "cmd a#b #comment\nnext", "cmd 'a\n # comment\n b'", 'cmd "a\n #{format}\n b"', "cmd 'a\\\nb'", "cmd a\\\nb", "cmd a\r\nb", "cmd %1:pause", "cmd '%1:pause'", "%1", "cmd %%", "cmd x%abc", "cmd ~ ~/bin ~root /~ x~", "HOME=/literal/path; cmd ~", "HOME= cmd ~", "cmd \\377 \\000", "cmd λ\u2003z \\377", "cmd \\UF0100", "cmd \\U000F0100 \\377", "cmd \\U000F0080 \\377"]
    for value in range(512):
        inputs.append("cmd \\" + format(value, "03o"))
    for escape in ("u0000", "u0001", "uD7FF", "uD800", "uDFFF", "uE000", "uFFFF", "U00010000", "U0010FFFF", "U00110000", "UFFFFFFFF", "u12", "u12x4", "U1234\n5678", "a", "b", "e", "f", "s", "v", "r", "n", "t", "x", "8", "9", "12x", "3", "00", "u0\n00", "u03bb", "U0001F980"):
        for quote in ("", '"', "'"):
            inputs.append("cmd " + quote + "\\" + escape + quote)
    atoms = ["", "x", " ", "\t", "\n", "\r", "\\", "\\\n", "'", '"', "$", "${x}", "$x", "~", "~root", "~zz-default-missing-user", "{", "}", ";", "#", "%if", "%else", "%endif", "%hidden", "%1", "%1:pause", "\u2003", "λ", "🌍", "\0", "\U000f0100"]
    for left in atoms:
        for right in atoms:
            inputs.append("cmd " + left + " " + right)
    seed = 0x73A5C149
    alphabet = "ab \t\n\\'\"${};#%=1~λ\r"
    for _ in range(2048):
        value = "cmd "
        for _ in range(24):
            seed = (seed * 1664525 + 1013904223) & 0xFFFFFFFF
            value += alphabet[seed % len(alphabet)]
        inputs.append(value)
    for size in (1021, 1022, 1023):
        inputs += ["cmd $" + "X" * size, "cmd ${" + "X" * size + "}", "cmd ~" + "X" * size]
    for size in (16383, 16384, 16385):
        inputs.append("X=" + "a" * (size - 2) + "; cmd $X")
    inputs.append("X=" + "a" * 1000 + "; cmd " + "$X" * 1024)
    inputs += [" " * (4 * 1024 * 1024), "cmd " + "a " * 65535, "x;" * 4096, "%if 0;" * 256 + "cmd;" + "%endif;" * 256, "run-shell " + "{ " * 256 + "} " * 256, "%if " + "#{" * 256 + "1" + "}" * 256 + " cmd; %endif;"]
    cases = [{"text": value} for value in dict.fromkeys(inputs)]
    for home in (None, "", "/virtual/user", "/a λ/b", "\U000f0100", "\0"):
        homes = {"": home, "alice": "/virtual/alice", "missing": None}
        for text in ("cmd ~ ~/bin ~alice ~missing", "cmd ~alice ~/bin", 'cmd "~alice" x"~alice"', "HOME=/override; cmd ~", "HOME=; cmd ~", "cmd ~alice \\377", "cmd ~missing"):
            cases.append({"text": text, "homes": homes})
    return cases


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    cases = vectors()
    subprocess.run(["cargo", "build", "--locked", "-p", "zz-mux", "--example", "cli_default_vectors"], cwd=ROOT, check=True, timeout=300)
    result = subprocess.run([str(ROOT / "target/debug/examples/cli_default_vectors")], input=json.dumps({"cases": cases}).encode(), cwd=ROOT, capture_output=True, check=True, timeout=120)
    source = json.loads(result.stdout)["cases"]
    (TARGET / "cli-default-source.json").write_text(json.dumps({"inputs": cases, "results": source}, ensure_ascii=True) + "\n")
    bounded = [" " * (4 * 1024 * 1024 + 1), "cmd " + "a " * 65536, "x;" * 4097, "%if 0;" * 257 + "cmd;" + "%endif;" * 257, "run-shell " + "{ " * 257 + "} " * 257, "%if " + "#{" * 257 + "1" + "}" * 257 + " cmd; %endif;"]
    bounded.append("X=" + "a" * 16000 + "; cmd " + "$X" * 2048)
    for value in bounded:
        cases.append({"text": value})
        source.append({"bounded": True, "commands": [], "diagnostics": [], "homes": {}, "home": None})
    cases.append({"text": "cmd ~zz-default-home-query"})
    source.append({"bounded": True, "commands": [], "diagnostics": [], "homes": {}, "home": None, "needed_home": "zz-default-home-query"})
    requests = bytearray()
    wanted = bytearray()
    uint(requests, len(cases))
    boundaries = []
    for case, parsed in zip(cases, source, strict=True):
        text(requests, case["text"])
        uint(requests, int(parsed["home"] is not None))
        if parsed["home"] is not None:
            text(requests, parsed["home"])
        uint(requests, len(parsed["homes"]))
        for name, value in parsed["homes"].items():
            text(requests, name)
            uint(requests, int(value is not None))
            if value is not None:
                text(requests, value)
        item = bytearray()
        uint(item, 2 if parsed.get("bounded") else int(bool(parsed["diagnostics"])))
        if parsed["diagnostics"]:
            diagnostic = parsed["diagnostics"][0]
            uint(item, diagnostic["line"])
            uint(item, diagnostic["column"])
            text(item, diagnostic["message"])
        uint(item, len(parsed["commands"]))
        for command in parsed["commands"]:
            invocation(item, command)
        needed = parsed.get("needed_home")
        uint(item, int(needed is not None))
        if needed is not None:
            text(item, needed)
        boundaries.append(len(wanted))
        uint(wanted, len(item))
        wanted.extend(item)
    request = TARGET / "cli-default-input.bin"
    output = TARGET / "cli-default-output.bin"
    binary = TARGET / "cli-default-probe"
    request.write_bytes(requests)
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/cli_default_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(request), str(output)], cwd=ROOT, check=True, timeout=120)
    actual = output.read_bytes()
    if actual != wanted:
        difference = next((index for index, pair in enumerate(zip(actual, wanted)) if pair[0] != pair[1]), min(len(actual), len(wanted)))
        case_index = max(0, sum(boundary <= difference for boundary in boundaries) - 1)
        raise AssertionError(f"default parser differs near case {case_index}, byte {difference}: {cases[case_index]["text"][:200]!r}; sizes {len(actual)} != {len(wanted)}")
    report = {"passed": True, "cases": len(cases), "bytes": len(wanted), "source_cases": len(source) - len(bounded) - 1, "bounded_cases": len(bounded), "needed_home_cases": 1, "source": "actual Rust parse_config and injected-home parse_config_with_expansions", "scope": "default-context grammar, quoted/unquoted words, assignments, false conditions, command blocks, exact source spans, raw octal/Unicode escapes, encoded-byte replay and diagnostics"}
    (TARGET / "cli-default-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
