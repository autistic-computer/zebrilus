import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / 'mojo'
KIT = ROOT.parent / 'mjui'
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    host = runpy.run_path(str(KIT / 'scripts/web/host_check.py'))
    env = dict(os.environ)
    env.update(MJ_CEF_DIR=str(host['engine']()), MJ_CC='/usr/bin/clang', MJ_CXX='/usr/bin/clang++', CC='/usr/bin/clang', CXX='/usr/bin/clang++')
    subprocess.run(['cargo', 'build', '--release', '--locked', '--offline', '--manifest-path', str(PORT / 'native/browser-url/Cargo.toml')], cwd=ROOT, check=True)
    for name in ['build_harfbuzz', 'build_native', 'build_runtime', 'build_vg', 'build_swash', 'build_mjcef']:
        subprocess.run(['bash', f'scripts/{name}.sh'], cwd=KIT, env=env, check=True)
    TARGET.mkdir(parents=True, exist_ok=True)
    binary = TARGET / 'browser-workspace-smoke'
    library = PORT / 'native/browser-url/target/release/libzzbrowserurl.a'
    command = f'source scripts/runtime_env.sh; mojo build -j 1 --fp-mode=contract=off -I src -I {PORT} -D ZZ_BROWSER=1 -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a -Xlinker build/lib/libmjcef.a -Xlinker {library} $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil -Xlinker -lm -Xlinker -lpthread {PORT / "browser_workspace_smoke.mojo"} -o {binary}'
    subprocess.run(['pixi', 'run', 'bash', '-c', command], cwd=KIT, env=env, check=True)
    subprocess.run(['python3', str(PORT / 'scripts/resources.py'), str(binary)], cwd=ROOT, check=True)
    env['ZZ_BEND'] = str(TARGET / 'zz-bend')
    native = ROOT / 'target/debug/zz_cli'
    with tempfile.TemporaryDirectory(prefix='zz-browser-workspace-', dir='/tmp') as directory:
        root = Path(directory)
        env['XDG_DATA_HOME'] = str(root / 'data')
        page = root / 'one.html'
        page.write_text("<!doctype html><title>load</title><body style='margin:0'><input autofocus onmousedown=\"if(document.title=='one')document.title='clicked'\" oninput=\"document.title='typed:'+this.value\" style='width:300px;height:60px'><script>requestAnimationFrame(()=>requestAnimationFrame(()=>{document.title='one'}))</script>")
        socket = str(root / 's')
        try:
            subprocess.run([str(native), '-S', socket, '-f', '/dev/null', 'new-session', '-d', '-s', 'web', '-c', str(root), 'cat'], cwd=root, env=env, check=True, capture_output=True, timeout=15)
            subprocess.run([str(native), '-S', socket, 'split-window', '-h', '-t', '%0', '--kind', 'browser', page.as_uri()], cwd=root, env=env, check=True, capture_output=True, timeout=15)
            subprocess.run([str(native), '-S', socket, 'select-pane', '-t', '%0'], cwd=root, env=env, check=True, capture_output=True, timeout=15)
            stdout = host['run'](env, binary, [socket])
        finally:
            subprocess.run([str(native), '-S', socket, 'kill-server'], cwd=root, env=env, capture_output=True, timeout=10)
    print(stdout.strip(), flush=True)
    report = {'passed': True, 'checks': ['actual private daemon with a browser pane on the source CEF 156.0.15 host', 'the pane descriptor starts a BrowserPane and draws its page in the workspace', 'with the terminal pane active, a click on the page selects the browser pane, focuses the page and reaches it', 'a typed key goes to the daemon as a browser surface key and comes back as a browser command the page receives', 'the prefix key arms the prefix table from the focused page, Escape cancels it and typing reaches the page again', 'Ctrl+T opens a tab and publishes two tabs to the daemon', 'Ctrl+W closes that tab, then closes the pane when one tab is left']}
    (TARGET / 'browser-workspace.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
