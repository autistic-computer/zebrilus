#!/usr/bin/env bash
set -euo pipefail
launch_dir=${ZZ_APP_STARTUP_DIRECTORY:-$PWD}
port_dir=$(cd "$(dirname "$0")/.." && pwd)
zz_dir=$(dirname "$port_dir")
if [ -z "${MJ_CC:-}" ] && [ -x /usr/bin/clang ]; then export MJ_CC=/usr/bin/clang; fi
if [ -z "${MJ_CXX:-}" ] && [ -x /usr/bin/clang++ ]; then export MJ_CXX=/usr/bin/clang++; fi
action=${1:-run}
shift || true
if [ "$action" != run ] && [ "${ZZ_PORT_GUARDED:-0}" != 1 ]; then
    exec python3 "$zz_dir/scripts/port/guard.py" bash "$0" "$action" "$@"
fi
mjui_dir=$(dirname "$zz_dir")/mjui
mkdir -p "$zz_dir/target/port"
export ZZ_BEND=${ZZ_BEND:-$zz_dir/target/port/zz-bend}
cd "$mjui_dir"
case "$action" in
  build-app)
    if [ "$(uname -s)" = Linux ]; then export MJ_NATIVE_RASTER=${MJ_NATIVE_RASTER:-1}; fi
    MJ_A11Y=0 MJUI_EXAMPLE_BUILD_ONLY=1 MJUI_EXAMPLE_OUT="$zz_dir/target/port/zz-mojo-app.next" bash scripts/run_example.sh "$port_dir/window.mojo"
    mv "$zz_dir/target/port/zz-mojo-app.next" "$zz_dir/target/port/zz-mojo-app"
    python3 "$port_dir/scripts/resources.py" "$zz_dir/target/port/zz-mojo-app"
    ;;
  build-browser-app)
    MJ_CEF_DIR=${MJ_CEF_DIR:-$(python3 -c 'import runpy, sys; print(runpy.run_path(sys.argv[1])["engine"]())' "$mjui_dir/scripts/web/host_check.py")}
    export MJ_CEF_DIR MJ_CC=${MJ_CC:-/usr/bin/clang} MJ_CXX=${MJ_CXX:-/usr/bin/clang++}
    (cd "$zz_dir" && cargo build --release --locked --offline --manifest-path "$port_dir/native/browser-url/Cargo.toml")
    for step in build_harfbuzz build_native build_runtime build_vg build_swash build_sdl build_shaders build_mjcef; do bash "scripts/$step.sh"; done
    pixi run bash -c 'source scripts/runtime_env.sh; mojo build -j 1 --fp-mode=contract=off -I src -I "$1" -D ASSERT=none -D ZZ_BROWSER=1 -D MJ_RASTER=swash -D MJ_SDL=1 -Xlinker build/lib/libmjswash.a -Xlinker build/lib/libmjcef.a -Xlinker "$1/native/browser-url/target/release/libzzbrowserurl.a" $MJ_LINK -Xlinker build/lib/libSDL3.a -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil -Xlinker -lm -Xlinker -lpthread "$1/window.mojo" -o "$2.next"' _ "$port_dir" "$zz_dir/target/port/zz-mojo-browser-app"
    mv "$zz_dir/target/port/zz-mojo-browser-app.next" "$zz_dir/target/port/zz-mojo-browser-app"
    python3 "$port_dir/scripts/resources.py" "$zz_dir/target/port/zz-mojo-browser-app"
    ;;
  build-output-smoke)
    pixi run bash -c 'mojo build -j 1 --fp-mode=contract=off -I src -I "$1" $MJ_LINK -o "$2.next" "$1/command_output_smoke.mojo"' _ "$port_dir" "$zz_dir/target/port/command-output-smoke"
    mv "$zz_dir/target/port/command-output-smoke.next" "$zz_dir/target/port/command-output-smoke"
    python3 "$port_dir/scripts/resources.py" "$zz_dir/target/port/command-output-smoke"
    ;;
  build-clipboard-smoke)
    pixi run bash -c 'mojo build -j 1 --fp-mode=contract=off -I src -I "$1" $MJ_LINK -D MJ_SDL=1 -Xlinker "$2/build/lib/libSDL3.a" -Xlinker -lm -Xlinker -ldl "$1/clipboard_workspace_smoke.mojo" -o "$3.next"' _ "$port_dir" "$mjui_dir" "$zz_dir/target/port/clipboard-workspace-smoke"
    mv "$zz_dir/target/port/clipboard-workspace-smoke.next" "$zz_dir/target/port/clipboard-workspace-smoke"
    python3 "$port_dir/scripts/resources.py" "$zz_dir/target/port/clipboard-workspace-smoke"
    ;;
  build-history-smoke)
    pixi run bash -c 'mojo build -j 1 --fp-mode=contract=off -I src -I "$1" $MJ_LINK -o "$2.next" "$1/history_workspace_smoke.mojo"' _ "$port_dir" "$zz_dir/target/port/history-workspace-smoke"
    mv "$zz_dir/target/port/history-workspace-smoke.next" "$zz_dir/target/port/history-workspace-smoke"
    python3 "$port_dir/scripts/resources.py" "$zz_dir/target/port/history-workspace-smoke"
    ;;
  run)
    python3 "$zz_dir/scripts/port/guard.py" bash "$port_dir/scripts/ui.sh" build-app
    cd "$launch_dir"
    exec env -u ZZ_PORT_GUARDED -u ZZ_PORT_BUILD_LOCK_FD "$zz_dir/target/port/zz-mojo-app" "$@"
    ;;
  build-shot|shot)
    pixi run bash -c 'mojo build -j 1 --fp-mode=contract=off -I src -I "$1" $MJ_LINK -o "$2.next" "$1/screenshot.mojo"' _ "$port_dir" "$zz_dir/target/port/zz-mojo-shot"
    mv "$zz_dir/target/port/zz-mojo-shot.next" "$zz_dir/target/port/zz-mojo-shot"
    python3 "$port_dir/scripts/resources.py" "$zz_dir/target/port/zz-mojo-shot"
    if [ "$action" = build-shot ]; then
      exit 0
    fi
    if [ "$#" -lt 2 ]; then
      echo "usage: ui.sh shot SOCKET OUTPUT.png" >&2
      exit 2
    fi
    exec "$zz_dir/target/port/zz-mojo-shot" "$@"
    ;;
  test-painter|test-readiness|test-command-output|test-history-painter)
    probe=terminal_painter
    if [ "$action" = test-readiness ]; then probe=readiness; fi
    if [ "$action" = test-command-output ]; then probe=command_output; fi
    if [ "$action" = test-history-painter ]; then probe=terminal_history_painter; fi
    pixi run bash -c 'mojo build -j 1 --fp-mode=contract=off -I src -I "$1" $MJ_LINK -o "$2" "$1/tests/test_$3.mojo"' _ "$port_dir" "$zz_dir/target/port/$probe-probe" "$probe"
    exec "$zz_dir/target/port/$probe-probe"
    ;;
  *) exit 2 ;;
esac
