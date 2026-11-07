import argparse
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / "mjui"


def install_application(binary):
    source = ROOT / "target/port/zz-mojo-app"
    if not source.is_file():
        return
    destination = binary.parent.parent / "share/frontend"
    destination.mkdir(parents=True, exist_ok=True)
    link = destination / "zz-app"
    if link.is_symlink():
        if link.resolve() == source.resolve():
            return
        raise SystemExit("desktop application resource already points elsewhere: " + str(link))
    if link.exists():
        raise SystemExit("desktop application resource already exists: " + str(link))
    link.symlink_to(source.resolve())


def install_backend(binary):
    source = ROOT / "target/debug/zz_cli"
    if not source.is_file():
        return
    destination = binary.parent.parent / "share/backend"
    destination.mkdir(parents=True, exist_ok=True)
    link = destination / "zz"
    if link.is_symlink():
        if link.resolve() == source.resolve():
            return
        raise SystemExit("daemon backend resource already points elsewhere: " + str(link))
    if link.exists():
        raise SystemExit("daemon backend resource already exists: " + str(link))
    link.symlink_to(source.resolve())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--backend-only", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    install_backend(binary)
    install_application(binary)
    if args.backend_only:
        return
    destination = binary.parent.parent / "share" / binary.name
    sources = {
        "fonts": MJUI / "bench/assets/fonts",
        "shaders": MJUI / "build/shaders",
        "icons": MJUI / "bench/assets/icons",
        "data": MJUI / "bench/data",
    }
    for name in ("fonts", "shaders"):
        if not sources[name].is_dir():
            raise SystemExit("missing runtime resources: " + str(sources[name]))
    destination.mkdir(parents=True, exist_ok=True)
    for name, source in sources.items():
        if not source.is_dir():
            continue
        link = destination / name
        if link.is_symlink():
            if link.resolve() == source.resolve():
                continue
            raise SystemExit("runtime resource link already points elsewhere: " + str(link))
        if link.exists():
            raise SystemExit("runtime resource path already exists: " + str(link))
        link.symlink_to(source.resolve(), target_is_directory=True)
    print("Runtime resources ready: " + str(destination))


if __name__ == "__main__":
    main()
