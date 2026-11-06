import json
from pathlib import Path
import subprocess


port = Path(__file__).resolve().parents[1]
repo = port.parent


def literal(value):
    return json.dumps(value, ensure_ascii=False)


def main():
    result = subprocess.run(
        ["cargo", "run", "--quiet", "-p", "zz-protocol", "--example", "port_vectors"],
        cwd=repo,
        stdout=subprocess.PIPE,
        check=True,
        timeout=180,
    )
    current = json.loads(result.stdout)
    fixture = json.loads((port / "tests/fixtures/protocol-v118.json").read_text())
    if current != fixture:
        raise SystemExit("protocol fixtures differ from the current Rust implementation")
    lines = [
        "from std.testing import assert_equal",
        "from zzmojo.protocol import client_hello, command_request, decode_response",
        "",
        "def main() raises:",
    ]
    for index, item in enumerate(current["hello"]):
        origin = "None" if item["origin"] is None else f'Optional[UInt64](UInt64({item["origin"]}))'
        lines.extend([
            f'    var hello_{index} = client_hello(UInt64({item["instance"]}), UInt32({item["process"]}), {origin})',
            f'    var expected_hello_{index}: List[UInt8] = {item["bytes"]}',
            f"    assert_equal(hello_{index}, expected_hello_{index})",
        ])
    for index, item in enumerate(current["requests"]):
        lines.extend([
            f'    var args_{index}: List[String] = {literal(item["args"])}',
            f'    var request_{index} = command_request(UInt64({item["request"]}), {literal(item["name"])}, Span(args_{index}))',
            f'    var expected_request_{index}: List[UInt8] = {item["bytes"]}',
            f"    assert_equal(request_{index}, expected_request_{index})",
        ])
    for index, item in enumerate(current["responses"]):
        lines.extend([
            f'    var frame_{index}: List[UInt8] = {item["bytes"]}',
            f"    var response_{index} = decode_response(frame_{index}^)",
            f'    assert_equal(response_{index}.request, UInt64({item["request"]}))',
            f'    var expected_output_{index}: List[UInt8] = {item["output"]}',
            f"    assert_equal(response_{index}.output, expected_output_{index})",
            f'    assert_equal(response_{index}.error, {literal(item["stderr"])})',
            f'    assert_equal(response_{index}.exit_code, UInt8({item["exit_code"]}))',
            f'    assert_equal(response_{index}.stdout_claim, UInt8({item["claim"]}))',
        ])
    count = len(current["hello"]) + len(current["requests"]) + len(current["responses"])
    lines.append(f'    print("{count} native protocol vectors match the current Rust encoder")')
    target = repo / "target/port"
    target.mkdir(parents=True, exist_ok=True)
    source = target / "protocol_vectors.mojo"
    source.write_text("\n".join(lines) + "\n")
    subprocess.run(["mojo", "-I", str(port), str(source)], cwd=repo, check=True, timeout=180)


if __name__ == "__main__":
    main()
