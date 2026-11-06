import argparse
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / "mjui"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
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
