#!/usr/bin/env python3
"""For screens in both rounds that no fixer named, lists those whose pixels changed, most changed first. Prints only, and exits 0 whatever it finds.

Usage: pixel-same.py <old shots> <new shots> <manifest> [--workers n] [--names-from text...]

"Named" means the screen has `arrows` in the new manifest, or its name appears as a whole word in a text given to --names-from.
A shot's `--arrows` copy is left out. No image library is on this Mac, so this reads the PNGs itself with zlib: 8-bit RGB or RGBA,
not interlaced, which is what Chrome writes. A shot it can't read, or two of different sizes, is listed as such and never counted same.
Read the small changes last: the same build captured twice differs on about a quarter of its screens, by at most 543 px on a sample of 56
(a clock's minute, a caret, antialiasing), so a line is drawn at 600 px. Below it, a change is only worth a look if its box is somewhere odd.
"""
import hashlib
import json
import multiprocessing
import os
import re
import struct
import sys
import zlib

NOISE = 600


def read_png(path):
    """(width, height, bytes per pixel, rows of raw pixels)."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos, width, height, bpp, idat = 8, 0, 0, 0, []
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos : pos + 8])
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", data[pos + 8 : pos + 21])
            if depth != 8 or colour not in (2, 6) or interlace:
                raise ValueError(f"depth {depth}, colour {colour}, interlace {interlace}")
            bpp = 3 if colour == 2 else 4
        elif kind == b"IDAT":
            idat.append(data[pos + 8 : pos + 8 + n])
        elif kind == b"IEND":
            break
        pos += 12 + n
    raw = zlib.decompress(b"".join(idat))
    stride = width * bpp
    rows, prev, at = [], bytes(stride), 0
    for _ in range(height):
        kind, line = raw[at], raw[at + 1 : at + 1 + stride]
        at += stride + 1
        if kind == 0:
            row = line
        elif kind == 2:
            row = add_up(line, prev)
        else:
            out = bytearray(line)
            if kind == 1:
                for i in range(bpp, stride):
                    out[i] = (out[i] + out[i - bpp]) & 255
            elif kind == 3:
                for i in range(stride):
                    out[i] = (out[i] + (((out[i - bpp] if i >= bpp else 0) + prev[i]) >> 1)) & 255
            elif kind == 4:
                for i in range(stride):
                    a = out[i - bpp] if i >= bpp else 0
                    b = prev[i]
                    c = prev[i - bpp] if i >= bpp else 0
                    p = a + b - c
                    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                    out[i] = (out[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
            else:
                raise ValueError(f"filter {kind}")
            row = bytes(out)
        rows.append(row)
        prev = row
    return width, height, bpp, rows


def add_up(line, prev):
    """Filter 2, whole-row: add bytes mod 256 without a Python loop, by holding the row in one big integer."""
    n = len(line)
    low = int.from_bytes(b"\x7f" * n, "little")
    a, b = int.from_bytes(line, "little"), int.from_bytes(prev, "little")
    total = (a & low) + (b & low)
    total ^= (a ^ b) & (low ^ int.from_bytes(b"\xff" * n, "little"))
    return total.to_bytes(n, "little")


def compare(old, new):
    """Pixels changed, of how many, the box round them, and the largest colour step. None for a shot that can't be compared."""
    w, h, bpp, a = read_png(old)
    w2, h2, bpp2, b = read_png(new)
    if (w, h) != (w2, h2):
        return {"size": f"{w}x{h} then {w2}x{h2}"}
    changed, left, top, right, bottom, step = 0, w, h, -1, -1, 0
    for y in range(h):
        if a[y] == b[y] and bpp == bpp2:
            continue
        ra, rb = a[y], b[y]
        for x in range(w):
            pa, pb = ra[x * bpp : x * bpp + 3], rb[x * bpp2 : x * bpp2 + 3]
            if pa != pb:
                changed += 1
                left, right = min(left, x), max(right, x)
                top, bottom = min(top, y), max(bottom, y)
                step = max(step, abs(pa[0] - pb[0]), abs(pa[1] - pb[1]), abs(pa[2] - pb[2]))
    return {"changed": changed, "total": w * h, "box": (left, top, right, bottom), "step": step}


def judge(pair):
    """One screen's result: "same", or its comparison, or a reason it can't be compared."""
    a, b = pair
    if hashlib.sha1(open(a, "rb").read()).digest() == hashlib.sha1(open(b, "rb").read()).digest():
        return "same"
    try:
        r = compare(a, b)
    except (ValueError, zlib.error, struct.error, IndexError, OSError) as err:
        return {"size": str(err)}
    return "same" if r.get("changed") == 0 else r


def main():
    argv = sys.argv[1:]
    workers = 2
    if "--workers" in argv:
        i = argv.index("--workers")
        workers = int(argv[i + 1])
        argv = argv[:i] + argv[i + 2 :]
    texts = []
    if "--names-from" in argv:
        i = argv.index("--names-from")
        texts, argv = argv[i + 1 :], argv[:i]
    if len(argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        return
    old_dir, new_dir, manifest = argv
    shots = lambda d: {f[:-4] for f in os.listdir(d) if f.endswith(".png") and not f.endswith("--arrows.png")}
    old, new = shots(old_dir), shots(new_dir)
    entries = json.load(open(manifest, encoding="utf-8"))
    named = {e["name"] for e in entries if e.get("arrows")}
    words = "\n".join(open(t, encoding="utf-8", errors="replace").read() for t in texts)
    named_by_text = {n for n in old & new if re.search(rf"(?<![\w-]){re.escape(n)}(?![\w-])", words)} if words else set()
    both = old & new
    pool = sorted(both - named - named_by_text)
    pairs = [(os.path.join(old_dir, n + ".png"), os.path.join(new_dir, n + ".png")) for n in pool]
    with multiprocessing.Pool(workers) as procs:
        results = procs.map(judge, pairs, chunksize=4)
    same = sum(r == "same" for r in results)
    changed = [(n, r) for n, r in zip(pool, results) if r != "same" and "size" not in r]
    broken = [(n, r["size"]) for n, r in zip(pool, results) if r != "same" and "size" in r]
    changed.sort(key=lambda c: -c[1]["changed"])
    routes = {e["name"]: e.get("route", "") for e in entries}
    print(f"{len(both)} screens in both rounds, {len(both & named)} named by arrows, {len(named_by_text - named)} more by text, {len(pool)} compared.")
    print(f"{same} the same, {len(changed)} changed, {len(broken)} couldn't be compared.")
    print(f"Only in the old round: {len(old - new)}. Only in the new: {len(new - old)}.")
    if changed:
        print("\nChanged, most first: pixels, share, box left,top to right,bottom, largest colour step, name, route")
    drawn = False
    for n, r in changed:
        if r["changed"] <= NOISE and not drawn:
            print(f"--- {NOISE} px and under, {sum(c[1]['changed'] <= NOISE for c in changed)} screens: what the same build captured twice differs by ---")
            drawn = True
        l, t, x, y = r["box"]
        print(f"{r['changed']:>8} {100 * r['changed'] / r['total']:6.2f}%  {l},{t} to {x},{y}  step {r['step']:<3}  {n}  {routes.get(n, '')}")
    for n, why in broken:
        print(f"couldn't compare {n}: {why}")


if __name__ == "__main__":
    main()
