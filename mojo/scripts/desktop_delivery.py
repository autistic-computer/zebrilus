import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-build', action='store_true')
    options = parser.parse_args()
    probe = TARGET / 'desktop-delivery-probe'
    worker = TARGET / 'zz-bend-input'
    if not options.skip_build:
        kit = runpy.run_path(str(ROOT / 'bend2/toolchain.py'))['ensure']()
        environment = os.environ.copy()
        environment['PATH'] = '/usr/bin:' + environment['PATH']
        temporary = TARGET / 'zz-bend-input.next'
        subprocess.run([kit['bend'], 'bend2/input_router_bridge.bend', '-o', str(temporary)], cwd=ROOT, env=environment, check=True)
        temporary.replace(worker)
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_delivery_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    reports = []
    with tempfile.TemporaryDirectory(prefix='zz-desktop-delivery-') as directory:
        directory = Path(directory)
        bad = directory / 'wrong-worker'
        bad.write_text('#!/usr/bin/python3\nimport struct,subprocess,sys\nline=sys.stdin.readline().split()\nsubprocess.run([' + repr(str(worker)) + ',*line],check=True,stdout=subprocess.DEVNULL)\np=line[2]\nb=open(p,"rb").read()\nw=list(struct.unpack("<"+str(len(b)//4)+"I",b))\ni=4\ndef owner():\n global i\n tag=w[i];i+=4 if tag==1 else 1\nowner()\npresent=w[i];i+=1\nif present:owner()\nowner();owner()\npresent=w[i];i+=1+3*present\nif w[i]:w[i+2]^=1\nelse:w[i:i+1]=[1,0,99]\nopen(p,"wb").write(struct.pack("<"+str(len(w))+"I",*w))\nprint("0",flush=True)\nsys.stdin.read()\n')
        bad.chmod(0o700)
        for name, mode in [('typed', []), ('native', [str(worker)]), ('semantic-fallback', [str(bad)]), ('missing-fallback', [str(directory / 'missing')])]:
            started = time.monotonic()
            command = [str(probe), *mode]
            if Path('/usr/bin/time').is_file():
                command = ['/usr/bin/time', '-f', 'DESKTOP_RSS_KIB %M', *command]
            result = subprocess.run(command, cwd=ROOT, timeout=120, capture_output=True, text=True)
            if result.returncode:
                raise RuntimeError(result.stdout[-4000:] + result.stderr[-8000:])
            counts = list(map(int, result.stdout.split()[-2:]))
            if name == 'native':
                assert counts[0] > 0
            if name.endswith('fallback'):
                assert counts[0] == 0
            report = {'mode': name, 'native': counts[0], 'fallback': counts[1], 'seconds': round(time.monotonic() - started, 3)}
            for line in result.stderr.splitlines():
                if line.startswith('DESKTOP_RSS_KIB '):
                    report['maximum_rss_kib'] = int(line.split()[1])
            reports.append(report)
            print(json.dumps(report), flush=True)
    (TARGET / 'desktop-delivery-verification.json').write_text(json.dumps({'passed': True, 'profiles': reports}, indent=2) + '\n')


if __name__ == '__main__':
    main()
