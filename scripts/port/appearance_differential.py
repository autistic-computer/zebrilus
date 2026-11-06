#!/usr/bin/env python3
import argparse
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]


class ResultParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.collect = False
        self.parts = []

    def handle_starttag(self, tag, attrs):
        if tag == "pre" and ("id", "result") in attrs:
            self.collect = True

    def handle_endtag(self, tag):
        if tag == "pre":
            self.collect = False

    def handle_data(self, data):
        if self.collect:
            self.parts.append(data)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mjui", type=Path, default=ROOT.parent / "mjui")
    parser.add_argument("--chrome", default=shutil.which("google-chrome") or shutil.which("chromium"))
    args = parser.parse_args()
    if not args.chrome:
        raise SystemExit("Chrome or Chromium is required for the independent CSS comparison")
    importer = runpy.run_path(str(args.mjui / "scripts/penna/import.py"))
    expected = json.loads((args.mjui / "docs/penna/values.json").read_text())
    contract = json.loads((args.mjui / "docs/penna/contract.json").read_text())
    mismatches = []
    checked = 0
    for theme in contract["themes"]:
        css = args.mjui / "build/penna-tokens" / (theme + ".css")
        if hashlib.sha256(css.read_bytes()).hexdigest() != contract["sources"][theme]["sha256"]:
            raise SystemExit(f"appearance contract does not match fresh CSS: {css}")
        keys = [key for key in expected if key.startswith(theme + "|")]
        descriptors = [key.split("|")[1:] for key in keys]
        properties = list(importer["BASE"])
        page = "<!doctype html><meta charset=utf-8><style>" + css.read_text() + "</style><pre id=result></pre><script>"
        page += "const descriptors=" + json.dumps(descriptors) + ";const properties=" + json.dumps(properties) + ";"
        page += """const results=descriptors.map(([role,tone,prominent,state])=>{
const element=document.createElement('div');
element.dataset.penna=role;
if(tone)element.dataset.tone=tone;
if(prominent==='1')element.dataset.prominent='';
if(state!=='rest')element.dataset.state=state;
document.body.append(element);
const computed=getComputedStyle(element);
const values=Object.fromEntries(properties.map(name=>[name,computed.getPropertyValue('--penna-'+name).trim()]));
element.remove();
return values;
});document.getElementById('result').textContent=JSON.stringify(results);</script>"""
        with tempfile.TemporaryDirectory(prefix="zz-css-") as temporary:
            path = Path(temporary) / "contract.html"
            path.write_text(page)
            result = subprocess.run([args.chrome, "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check", "--user-data-dir=" + str(Path(temporary) / "profile"), "--virtual-time-budget=1000", "--dump-dom", path.as_uri()], capture_output=True, text=True, timeout=60, check=True)
        document = ResultParser()
        document.feed(result.stdout)
        actual = json.loads("".join(document.parts))
        if len(actual) != len(keys):
            raise SystemExit(f"browser returned {len(actual)} of {len(keys)} recipes for {theme}")
        for key, values in zip(keys, actual, strict=True):
            normalized = importer["record"](values)
            checked += 1
            if normalized != expected[key]:
                mismatches.append({"key": key, "expected": expected[key], "actual": normalized})
        print(f"{theme}: {len(keys)} browser-computed recipes checked", flush=True)
    report = {"reference": "penna CSS computed by Chromium", "records": checked, "mismatches": mismatches, "scope": "direct role/tone/prominence selectors on the root ground; contextual and transparency overrides excluded"}
    target = ROOT / "target/port/appearance-differential.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(report, indent=2) + "\n")
    if mismatches:
        for mismatch in mismatches[:10]:
            print(json.dumps(mismatch))
        raise SystemExit(f"{len(mismatches)} of {checked} appearance records disagree with the browser")
    print(f"Appearance differential: {checked} records agree with the browser")


if __name__ == "__main__":
    main()
