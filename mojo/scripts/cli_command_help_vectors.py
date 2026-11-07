import argparse
import json
from pathlib import Path
import runpy
import subprocess

from cli_preflight_vectors import uint, text


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'
DATA = ROOT / 'mojo/zzmojo/cli_command_help_data.mojo'
ORACLE = ROOT / 'target/debug/examples/cli_command_help_vectors'


def oracle(cases):
    result = subprocess.run([str(ORACLE)], input=json.dumps({'cases': cases}).encode(), capture_output=True, cwd=ROOT, check=True, timeout=60)
    return json.loads(result.stdout)


def encoded_catalog(catalog):
    data = bytearray()
    uint(data, len(catalog))
    for item in catalog:
        text(data, item['name'])
        text(data, item['help'])
    if len(catalog) > 512 or len(data) > 512 * 1024:
        raise AssertionError('native help catalog exceeds its application bound')
    return data


def generated_catalog(catalog):
    data = encoded_catalog(catalog)
    return 'comptime COMMAND_HELP = "' + data.hex() + '''"


@inline(.never)
def command_help_bytes() -> List[UInt8]:
    var data = StringSlice(COMMAND_HELP).as_bytes()
    var result = List[UInt8](capacity=len(data) // 2)
    for offset in range(0, len(data), 2):
        var value: UInt8 = 0
        for index in range(2):
            var digit = data[offset + index]
            value = (value << 4) | (digit - 48 if digit <= 57 else digit - 87)
        result.append(value)
    return result^
'''


def cases(catalog):
    names = {'', 'unknown-command', 'new_session', 'DISPLAY', '🔎', 'a\0b', 'a' * (4 * 1024 * 1024)}
    for item in catalog:
        name = item['name']
        names.update(name[:index] for index in range(1, len(name) + 1))
        names.update(item['aliases'])
    return sorted(names)


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--generate', action='store_true')
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    subprocess.run(['cargo', 'build', '--quiet', '--locked', '-p', 'zz-cli', '--example', 'cli_command_help_vectors'], cwd=ROOT, check=True, timeout=300)
    source = oracle([])
    catalog = source['catalog']
    generated = generated_catalog(catalog)
    if args.generate:
        DATA.write_text(generated)
        print(json.dumps({'catalog': len(catalog), 'bytes': len(encoded_catalog(catalog))}))
        return
    if DATA.read_text() != generated:
        raise AssertionError('command help catalog changed; regenerate explicitly')
    names = cases(catalog)
    results = oracle(names)['cases']
    names.append('a' * (4 * 1024 * 1024 + 1))
    results.append({'code': 2, 'help': '', 'error': 'zz: command help name exceeds its byte limit'})
    data = bytearray()
    wanted = bytearray()
    uint(data, len(names))
    for name, result in zip(names, results, strict=True):
        text(data, name)
        uint(wanted, result['code'])
        text(wanted, result['help'])
        uint(wanted, int(result['error'] is not None))
        if result['error'] is not None:
            text(wanted, result['error'])
    source_path = TARGET / 'cli-command-help-input.bin'
    output = TARGET / 'cli-command-help-output.bin'
    binary = TARGET / 'cli-command-help-probe'
    source_path.write_bytes(data)
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/cli_command_help_probe.mojo'), '-o', str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(source_path), str(output)], cwd=ROOT, check=True, timeout=120)
    actual = output.read_bytes()
    if actual != wanted:
        index = next((index for index, pair in enumerate(zip(actual, wanted)) if pair[0] != pair[1]), min(len(actual), len(wanted)))
        raise AssertionError(f'command help byte {index} differs; lengths {len(actual)} != {len(wanted)}')
    report = {'passed': True, 'commands': len(catalog), 'cases': len(names), 'bytes': len(wanted), 'bounded_cases': 1, 'source': 'actual command_help and command resolution', 'scope': 'canonical names, aliases, every prefix, exact option/positional help, unknown/ambiguous names and command byte limit'}
    (TARGET / 'cli-command-help-differential.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
