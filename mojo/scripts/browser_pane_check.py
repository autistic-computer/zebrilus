from pathlib import Path
import runpy
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
KIT = ROOT.parent / 'mjui'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    host = runpy.run_path(str(KIT / 'scripts/web/host_check.py'))
    env = dict(__import__('os').environ)
    env.update(MJ_CEF_DIR=str(host['engine']()), MJ_CC='/usr/bin/clang', MJ_CXX='/usr/bin/clang++', CC='/usr/bin/clang', CXX='/usr/bin/clang++')
    subprocess.run(['cargo', 'build', '--release', '--locked', '--offline', '--manifest-path', str(ROOT / 'mojo/native/browser-url/Cargo.toml')], cwd=ROOT, check=True)
    for name in ['build_harfbuzz', 'build_native', 'build_runtime', 'build_vg', 'build_mjcef']:
        subprocess.run(['bash', f'scripts/{name}.sh'], cwd=KIT, env=env, check=True)
    (ROOT / 'target/port').mkdir(parents=True, exist_ok=True)
    binary = ROOT / 'target/port/browser-pane-host-probe'
    library = ROOT / 'mojo/native/browser-url/target/release/libzzbrowserurl.a'
    command = f'source scripts/runtime_env.sh; mojo build --fp-mode=contract=off -I src -I {ROOT / "mojo"} -Xlinker build/lib/libmjcef.a -Xlinker {library} $MJ_LINK -Xlinker -ldl -Xlinker -lm -Xlinker -lpthread {ROOT / "mojo/tests/browser_pane_host_probe.mojo"} -o {binary}'
    subprocess.run(['pixi', 'run', 'bash', '-c', command], cwd=KIT, env=env, check=True)
    with tempfile.TemporaryDirectory(prefix='zz-browser-pane-') as directory:
        fixture = Path(directory)
        (fixture / 'cache').mkdir(mode=0o700)
        one = fixture / 'one.html'
        two = fixture / 'two.html'
        popup = fixture / 'popup.html'
        one.write_text("<!doctype html><title>one</title><body style='margin:0'><a href='popup.html' target='_blank' style='display:block;width:320px;height:80px'>popup</a><p>select me</p>")
        two.write_text("<!doctype html><title>two</title><body>two")
        popup.write_text("<!doctype html><body><script>requestAnimationFrame(()=>document.title='popup:'+devicePixelRatio)</script>")
        stdout = host['run'](env, binary, [fixture, one.as_uri(), two.as_uri(), (fixture / 'missing.html').as_uri()])
    print(stdout.strip(), flush=True)


if __name__ == '__main__':
    main()
