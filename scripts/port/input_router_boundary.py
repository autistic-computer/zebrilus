#!/usr/bin/env python3
import argparse
import base64
import copy
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import time

from input_router_packets import packed, payload_arena, trace_packet, outcome, request


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(Path(__file__).with_name('guard.py')))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--skip-reference', action='store_true')
    parser.add_argument('--skip-proof', action='store_true')
    parser.add_argument('--worker', type=Path)
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    tools = runpy.run_path(str(ROOT / 'bend2/toolchain.py'))['ensure']()
    environment = os.environ.copy()
    environment['PATH'] = os.pathsep.join(['/usr/bin', str(Path(tools['bend']).parent), str(Path(tools['lean']).parent), environment['PATH']])
    if not options.skip_proof:
        subprocess.run([tools['bend'], 'bend2/PROOF.bend'], cwd=ROOT, env=environment, check=True)
        subprocess.run([tools['bend'], 'bend2/PROOF.bend', '--verdict'], cwd=ROOT, env=environment, check=True)
    if not options.skip_reference:
        subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'input_router_oracle', '--features', 'input-router-fixtures', '--', str(TARGET / 'input-router-oracle.json')], cwd=ROOT, env=environment, check=True)
    reference = TARGET / 'input-router-oracle.json'
    if reference.stat().st_size > 64 * 1024 * 1024:
        raise RuntimeError('source router corpus exceeds 64 MiB')
    oracle = json.loads(reference.read_text())
    if len(oracle['traces']) > 256 or any(len(trace['steps']) > 512 for trace in oracle['traces']):
        raise RuntimeError('source router corpus exceeds trace limits')
    operations = sum(len(trace['steps']) for trace in oracle['traces'])
    probe = TARGET / 'input-router-conformance'
    worker = options.worker.resolve() if options.worker else TARGET / 'zz-bend-input'
    fault_probe = TARGET / 'input-router-fault'
    if not options.skip_build:
        if not options.worker:
            subprocess.run([tools['bend'], 'bend2/input_router_bridge.bend', '-o', str(worker)], cwd=ROOT, env=environment, check=True)
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/input_router_conformance_probe.mojo'), '-o', str(probe)], cwd=ROOT, env=environment, check=True)
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/input_router_fault_probe.mojo'), '-o', str(fault_probe)], cwd=ROOT, env=environment, check=True)
    reports = []
    with tempfile.TemporaryDirectory(prefix='zz-router-') as directory:
        directory = Path(directory)
        (directory / 'traces.bin').write_bytes(packed([len(oracle['traces']), operations]))
        (directory / 'payloads.bin').write_bytes(packed(payload_arena(oracle['references'])))
        for index, trace in enumerate(oracle['traces']):
            (directory / f'trace-{index}.bin').write_bytes(packed(trace_packet(trace, index, oracle['references'])))
        for phase in ['model', 'native']:
            started = time.monotonic()
            result = subprocess.run([str(probe), phase, str(directory), str(worker)], cwd=ROOT, env=environment, check=True, capture_output=True, timeout=300)
            operations, compared, native, fallback = map(int, result.stdout.split())
            reports.append({'phase': phase, 'operations': operations, 'bytes_compared': compared, 'native': native, 'fallback': fallback, 'seconds': round(time.monotonic() - started, 3)})
            print(json.dumps(reports[-1]), flush=True)
        first = oracle['traces'][0]['steps'][0]
        faults = {}
        for kind in ['map', 'key', 'action', 'payload', 'metadata', 'semantic']:
            value = copy.deepcopy(first)
            state = value['after']
            if kind == 'map':
                state['keymap']['high'] ^= 1
            elif kind == 'key':
                state['claims']['local'].append({'$':'KeyToken','id':{'$':'Id','high':123,'low':456}})
            elif kind == 'action':
                state['effects'] = [{'$':'Chrome','action':{'$':'ChromeAction','id':{'$':'Id','high':123,'low':456}}}]
            elif kind in ('payload', 'metadata'):
                payload = oracle['references']['inputs'][0]
                key = oracle['references']['keys'][0]
                typed = {'$':'Input','key':{'$':'KeyToken','id':key['id']},'unshifted':{'$':'None'},'action':{'$':'KeyPressed' if kind == 'payload' else 'Repeat'},'payload':{'$':'Payload','id':payload['id'] if kind == 'metadata' else {'$':'Id','high':123,'low':456}}}
                state['effects'] = [{'$':'Forward','pane':{'$':'Id','high':0,'low':0},'input':typed}]
            else:
                state['capture'] = {'$':'Some','value':{'$':'Id','high':0,'low':99}}
            faults[kind] = packed(outcome(value))
        faults['truncated'] = b'partial'
        faults['oversized'] = None
        faults['exited'] = None
        faults['timeout'] = None
        for kind, value in faults.items():
            path = directory / ('worker-' + kind)
            script = '#!/usr/bin/python3\nimport sys,os,time,signal,base64\n'
            if kind == 'exited':
                script += 'raise SystemExit(0)\n'
            else:
                script += 'line=sys.stdin.readline()\n'
                if kind == 'timeout':
                    script += 'signal.signal(signal.SIGTERM,signal.SIG_IGN)\ntime.sleep(60)\n'
                elif kind == 'oversized':
                    script += 'p=line.split()[2]\nwith open(p,"wb") as f:f.truncate(65540)\nprint("0",flush=True)\ntime.sleep(60)\n'
                else:
                    data = base64.b64encode(value).decode()
                    script += f'open(line.split()[2],"wb").write(base64.b64decode("{data}"))\nprint("0",flush=True)\ntime.sleep(60)\n'
            path.write_text(script)
            path.chmod(0o700)
            started = time.monotonic()
            result = subprocess.run([str(probe), 'failed', str(directory), str(path)], cwd=ROOT, env=environment, check=True, capture_output=True, timeout=300)
            values = list(map(int, result.stdout.split()))
            assert values[2:] == [0, operations], (kind, values)
            reports.append({'phase': 'failed-' + kind, 'operations': values[0], 'bytes_compared': values[1], 'native': values[2], 'fallback': values[3], 'seconds': round(time.monotonic() - started, 3)})
            print(json.dumps(reports[-1]), flush=True)
        profiles = []
        for mode in [[], [str(worker)]]:
            command = [str(fault_probe), *mode]
            if sys.platform == 'linux' and Path('/usr/bin/time').is_file():
                command = ['/usr/bin/time', '-f', 'ROUTER_MAX_RSS_KIB %M', *command]
            started = time.monotonic()
            result = subprocess.run(command, cwd=ROOT, env=environment, check=True, capture_output=True, timeout=60)
            profile = {'mode': 'native' if mode else 'typed', 'seconds': round(time.monotonic() - started, 3)}
            for line in result.stderr.decode().splitlines():
                if line.startswith('ROUTER_MAX_RSS_KIB '):
                    profile['maximum_rss_kib'] = int(line.split()[1])
            profiles.append(profile)
            print(result.stdout.decode().strip(), flush=True)
        sentinel = b'untouched-output'
        current, output = directory / 'request.bin', directory / 'result.bin'
        base = request(first)
        massive = list(base)
        massive[10] = 0xFFFFFFFF
        invalid = [b'123', packed([0, *base[1:]]), packed(base + [0]), packed(massive), b'\0' * 65540]
        for raw in invalid:
            current.write_bytes(raw)
            output.write_bytes(sentinel)
            result = subprocess.run([str(worker), 'input', str(current), str(output)], cwd=ROOT, env=environment, capture_output=True, timeout=10)
            assert result.returncode == 65, result.stderr.decode()[-1000:]
            assert output.read_bytes() == sentinel
    report = {'passed': True, 'worker': str(worker), 'traces': len(oracle['traces']), 'phases': reports, 'native_atomic_refusals': len(invalid), 'focused_runtime_profiles': profiles}
    (TARGET / 'input-router-boundary.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
