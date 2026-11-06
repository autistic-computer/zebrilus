[doc("Verify the Bend core, Mojo app, appearance port, or all three.")]
check front="all":
    python3 scripts/port/check.py {{ front }}

[doc("Install the pinned Bend compiler and proof kernel in the user cache.")]
setup:
    python3 bend2/toolchain.py
    pixi install --manifest-path mojo/pixi.toml --locked

[doc("Build the native Mojo command client and Bend runtime.")]
build:
    python3 scripts/port/check.py core
    pixi run --manifest-path mojo/pixi.toml build

[doc("Run the native mjui workspace against a daemon socket.")]
run socket:
    ZZ_BEND="$PWD/target/port/zz-bend" pixi run --manifest-path mojo/pixi.toml run --socket {{ quote(socket) }}

[doc("Read the current migration boundaries and verification commands.")]
status:
    cat docs/mojo-bend-port.md
import "settings.just"

set working-directory := "../.."
