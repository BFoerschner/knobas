#!/usr/bin/env python3
"""Pack the Firefox extension into an unsigned `.xpi` (#2534, ADR-0061).

An `.xpi` is a zip with `manifest.json` at its root. This one holds only
what Firefox loads (the manifest, the background page, `src/`, `icons/`),
never the tests or the build tooling, and is byte-for-byte the same for the
same sources: entries sorted, every timestamp 1980-01-01, one file mode. So a
rebuild shows whether anything changed, and the file sent to AMO for
unlisted signing is the file the tests ran against.

Usage: python3 -I extension/firefox/build.py [--out DIR]
  (default DIR: target/extension; prints the path and its SHA-256)
"""
import argparse
import hashlib
import json
import sys
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
SHIPPED = ["manifest.json", "background.html", "src", "icons"]
EPOCH = (1980, 1, 1, 0, 0, 0)


def files():
    out = []
    for name in SHIPPED:
        path = HERE / name
        if path.is_dir():
            out += [p for p in path.rglob("*") if p.is_file() and not p.name.startswith(".")]
        else:
            out.append(path)
    return sorted(out, key=lambda p: p.relative_to(HERE).as_posix())


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, default=ROOT / "target" / "extension")
    args = parser.parse_args()
    manifest = json.loads((HERE / "manifest.json").read_text())
    version = manifest["version"]
    args.out.mkdir(parents=True, exist_ok=True)
    xpi = args.out / f"knobas-link-handoff-{version}.xpi"
    with zipfile.ZipFile(xpi, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for path in files():
            info = zipfile.ZipInfo(path.relative_to(HERE).as_posix(), date_time=EPOCH)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            info.create_system = 3
            z.writestr(info, path.read_bytes(), compresslevel=9)
    digest = hashlib.sha256(xpi.read_bytes()).hexdigest()
    print(f"{xpi}\nsha256 {digest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
