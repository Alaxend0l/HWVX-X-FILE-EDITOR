#!/usr/bin/env python3
"""Decode the PS2 release's pack textures, and compare them with the GameCube's and the PC's.

    python3 -I ps2_tex.py PS2_PACK [--png OUT_DIR]             list (and decode) the textures
    python3 -I ps2_tex.py --compare PS2_PACK GC_PACK [PC_PACK] [--png OUT_DIR]
    python3 -I ps2_tex.py --survey EXTRACTED_PS2_TREE
    python3 -I ps2_tex.py --check PS2_TREE GC_TREE PC_TREE     pair every texture by content

Rules (../README.md §2):
* header table at texture_data_offset, num_textures x 0x10 bytes, packed, little-endian:
  u16 format (2 = PSMT8: 8-bit index + CLUT, 1 = PSMCT32: R, G, B, A bytes), u16 width,
  u16 height, u16 levels (0 = one level, else the stored level count; chains go to 1x1),
  ptr pixels (pointer slot), u32 CLUT index;
* CLUTs at palette_offset, num_palettes x 0x400 bytes, 256 entries of R, G, B, A bytes, in
  the GS's CSM1 order (logical index i is stored at i with bits 3 and 4 swapped);
* pixels linear, row-major (no GS swizzle); levels back to back, each w*h*bpp/8, 16-aligned
  at the end of the chain;
* RGBA are 0..255 (alpha not GS-scaled), the PC's colours: GameCube = PS2^2 / 255 per RGB.

Only the standard library is used (PNG writing with zlib).
"""

import collections
import json
import struct
import sys
import zlib
from pathlib import Path

FMT_PSMCT32, FMT_PSMT8 = 1, 2


def load(folder):
    f = Path(folder)
    return json.loads((f / "pack.json").read_text())["header"], (f / "data.bin").read_bytes()


def csm1(i):
    """Logical CLUT index -> stored position (swap bits 3 and 4); its own inverse."""
    return (i & ~0x18) | ((i & 0x08) << 1) | ((i & 0x10) >> 1)


def ps2_headers(h, d):
    out = []
    for i in range(h.get("num_textures", 0)):
        o = h["texture_data_offset"] + 0x10 * i
        fmt, w, ht, levels, ptr, clut = struct.unpack_from("<4H2I", d, o)
        out.append({"i": i, "fmt": fmt, "w": w, "h": ht, "levels": levels, "ptr": ptr, "clut": clut})
    return out


def ps2_level_offset(t, level):
    bpp = 4 if t["fmt"] == FMT_PSMCT32 else 1
    off, w, h = 0, t["w"], t["h"]
    for _ in range(level):
        off += w * h * bpp
        w, h = max(1, w // 2), max(1, h // 2)
    return off, w, h


def ps2_palette(h, d, idx):
    o = h["palette_offset"] + 0x400 * idx
    return [tuple(d[o + 4 * csm1(k):o + 4 * csm1(k) + 4]) for k in range(256)]


def alpha_to_255(a):
    """Alpha is stored 0..255 (not the GS's 0x80 = 1.0): kept as it is."""
    return a


def ps2_decode(h, d, t, level=0, raw_alpha=False):
    """RGBA rows (list of bytearrays) of one level."""
    off, w, ht = ps2_level_offset(t, level)
    p = t["ptr"] + off
    rows = []
    if t["fmt"] == FMT_PSMT8:
        pal = ps2_palette(h, d, t["clut"])
        pal = [bytes((r, g, b, a if raw_alpha else alpha_to_255(a))) for r, g, b, a in pal]
        for y in range(ht):
            rows.append(bytearray(b"".join(pal[i] for i in d[p + y * w:p + y * w + w])))
    elif t["fmt"] == FMT_PSMCT32:
        for y in range(ht):
            row = bytearray(d[p + 4 * y * w:p + 4 * (y + 1) * w])
            if not raw_alpha:
                for x in range(w):
                    row[4 * x + 3] = alpha_to_255(row[4 * x + 3])
            rows.append(row)
    else:
        raise ValueError("PS2 texture format %d" % t["fmt"])
    return w, ht, rows


# ---------------------------------------------------------------------------------------------
# GameCube and PC decoders (for comparison)


def gc_headers(h, d):
    out, o = [], h.get("texture_data_offset", 0)
    for i in range(h.get("num_textures", 0)):
        fmt, w, ht, mips = struct.unpack_from("<4H", d, o)
        out.append({"i": i, "fmt": fmt, "w": w, "h": ht, "mips": mips, "ptr": struct.unpack_from(">I", d, o + 8)[0]})
        o = (o + 0x10 + 0x1F) & ~0x1F
    return out


def _565(c):
    return (((c >> 11) & 31) * 255 // 31, ((c >> 5) & 63) * 255 // 63, (c & 31) * 255 // 31)


def gc_decode(d, t):
    w, ht, p = t["w"], t["h"], t["ptr"]
    rows = [bytearray(4 * w) for _ in range(ht)]
    if t["fmt"] == 4:                                    # CMPR
        for by in range(0, ht, 8):
            for bx in range(0, w, 8):
                for sb in range(4):
                    sx, sy = bx + (sb & 1) * 4, by + (sb >> 1) * 4
                    c0, c1, bits = struct.unpack_from(">HHI", d, p)
                    p += 8
                    a, b = _565(c0), _565(c1)
                    if c0 > c1:
                        pal = [a + (255,), b + (255,), tuple((2 * a[k] + b[k]) // 3 for k in range(3)) + (255,),
                               tuple((a[k] + 2 * b[k]) // 3 for k in range(3)) + (255,)]
                    else:
                        pal = [a + (255,), b + (255,), tuple((a[k] + b[k]) // 2 for k in range(3)) + (255,), (0, 0, 0, 0)]
                    for y in range(4):
                        for x in range(4):
                            if sx + x < w and sy + y < ht:
                                rows[sy + y][4 * (sx + x):4 * (sx + x) + 4] = bytes(pal[(bits >> (30 - 2 * (y * 4 + x))) & 3])
    elif t["fmt"] == 1:                                  # RGBA8
        for by in range(0, ht, 4):
            for bx in range(0, w, 4):
                ar, gb = d[p:p + 32], d[p + 32:p + 64]
                p += 64
                for k in range(16):
                    x, y = bx + (k & 3), by + (k >> 2)
                    if x < w and y < ht:
                        rows[y][4 * x:4 * x + 4] = bytes((ar[2 * k + 1], gb[2 * k], gb[2 * k + 1], ar[2 * k]))
    else:
        raise ValueError("GameCube texture format %d" % t["fmt"])
    return w, ht, rows


def pc_headers(h, d):
    out = []
    for i in range(h.get("num_textures", 0)):
        fmt, w, ht, mips, ptr = struct.unpack_from("<4HI", d, h["texture_data_offset"] + 0x10 * i)
        out.append({"i": i, "fmt": fmt, "w": w, "h": ht, "mips": mips, "ptr": ptr})
    return out


def pc_decode(d, t):
    w, ht, p, fmt = t["w"], t["h"], t["ptr"], t["fmt"]
    rows = [bytearray(4 * w) for _ in range(ht)]
    if fmt in (4, 5):
        for by in range(0, ht, 4):
            for bx in range(0, w, 4):
                alpha = [255] * 16
                if fmt == 5:
                    a0, a1 = d[p], d[p + 1]
                    bits = int.from_bytes(d[p + 2:p + 8], "little")
                    al = [a0, a1] + ([((8 - k) * a0 + (k - 1) * a1) // 7 for k in range(2, 8)] if a0 > a1 else
                                     [((6 - k) * a0 + (k - 1) * a1) // 5 for k in range(2, 6)] + [0, 255])
                    alpha = [al[(bits >> (3 * k)) & 7] for k in range(16)]
                    p += 8
                c0, c1, bits = struct.unpack_from("<HHI", d, p)
                p += 8
                a, b = _565(c0), _565(c1)
                pal = [a, b, tuple((2 * a[k] + b[k]) // 3 for k in range(3)), tuple((a[k] + 2 * b[k]) // 3 for k in range(3))]
                if fmt == 4 and c0 <= c1:
                    pal[2] = tuple((a[k] + b[k]) // 2 for k in range(3))
                    pal[3] = (0, 0, 0)
                for k in range(16):
                    x, y = bx + (k & 3), by + (k >> 2)
                    if x < w and y < ht:
                        rows[y][4 * x:4 * x + 4] = bytes(pal[(bits >> (2 * k)) & 3] + (alpha[k],))
    elif fmt in (0, 1):
        bpp = 3 if fmt == 0 else 4
        for y in range(ht):
            for x in range(w):
                px = d[p + bpp * (y * w + x):p + bpp * (y * w + x) + bpp]
                rows[y][4 * x:4 * x + 4] = bytes(px) + (b"\xff" if bpp == 3 else b"")
    else:
        raise ValueError("PC texture format %d" % fmt)
    return w, ht, rows


# ---------------------------------------------------------------------------------------------


def write_png(path, w, h, rows):
    raw = b"".join(b"\x00" + bytes(r) for r in rows)

    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    Path(path).write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
                           + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def thumb(w, h, rows, n=8):
    """n x n box-averaged RGB(A) thumbnail as a flat list."""
    out = []
    for ty in range(n):
        for tx in range(n):
            acc, cnt = [0, 0, 0, 0], 0
            for y in range(ty * h // n, max(ty * h // n + 1, (ty + 1) * h // n)):
                r = rows[y]
                for x in range(tx * w // n, max(tx * w // n + 1, (tx + 1) * w // n)):
                    for c in range(4):
                        acc[c] += r[4 * x + c]
                    cnt += 1
            out += [a / cnt for a in acc]
    return out


def resample(w, h, rows, W, H):
    """Nearest-neighbour resize so two releases' textures can be compared texel by texel."""
    return [bytearray(b"".join(bytes(rows[y * h // H][4 * (x * w // W):4 * (x * w // W) + 4]) for x in range(W)))
            for y in range(H)]


def compare(a, b, fn=lambda v: v):
    """Mean |fn(a) - b| over RGB of opaque texels, and mean |alpha a - alpha b|."""
    (w, h, ra), (_, _, rb) = a, b
    e = ea = n = 0
    for y in range(h):
        for x in range(w):
            pa, pb = ra[y][4 * x:4 * x + 4], rb[y][4 * x:4 * x + 4]
            ea += abs(pa[3] - pb[3])
            if pb[3] >= 128:
                e += sum(abs(fn(pa[c]) - pb[c]) for c in range(3)) / 3
                n += 1
    return (e / n if n else 0.0), ea / (w * h)


def squared(v):
    return v * v / 255


def match_textures(A, B):
    """Greedy best pairing of two lists of (key, thumbnail) by RGB thumbnail distance."""
    cands = []
    for i, ta in A:
        for j, tb in B:
            dist = sum(abs(x - y) for k, (x, y) in enumerate(zip(ta, tb)) if k % 4 != 3) / (len(ta) * 3 / 4)
            cands.append((dist, i, j))
    cands.sort()
    used_a, used_b, out = set(), set(), {}
    for dist, i, j in cands:
        if i in used_a or j in used_b:
            continue
        used_a.add(i)
        used_b.add(j)
        out[i] = (j, dist)
    return out



# ---------------------------------------------------------------------------------------------
# Sampling one texel at a time (fast enough to compare every texture of every pack)


def ps2_texel(h, d, t, x, y):
    if t["fmt"] == FMT_PSMT8:
        i = d[t["ptr"] + y * t["w"] + x]
        o = h["palette_offset"] + 0x400 * t["clut"] + 4 * csm1(i)
        return tuple(d[o:o + 4])
    o = t["ptr"] + 4 * (y * t["w"] + x)
    return tuple(d[o:o + 4])


def gc_texel(d, t, x, y):
    w = t["w"]
    if t["fmt"] == 4:
        tile = (y // 8) * ((w + 7) // 8) + x // 8
        o = t["ptr"] + 32 * tile + 8 * (((y % 8) // 4) * 2 + (x % 8) // 4)
        c0, c1, bits = struct.unpack_from(">HHI", d, o)
        k = (bits >> (30 - 2 * ((y % 4) * 4 + x % 4))) & 3
        a, b = _565(c0), _565(c1)
        if k < 2:
            return (a, b)[k] + (255,)
        if c0 > c1:
            return tuple(((2 * a[i] + b[i]) if k == 2 else (a[i] + 2 * b[i])) // 3 for i in range(3)) + (255,)
        return tuple((a[i] + b[i]) // 2 for i in range(3)) + (255,) if k == 2 else (0, 0, 0, 0)
    if t["fmt"] == 1:
        o = t["ptr"] + 64 * ((y // 4) * ((w + 3) // 4) + x // 4)
        k = (y % 4) * 4 + x % 4
        return (d[o + 2 * k + 1], d[o + 32 + 2 * k], d[o + 33 + 2 * k], d[o + 2 * k])
    raise ValueError("GameCube texture format %d" % t["fmt"])


def pc_texel(d, t, x, y):
    w, fmt = t["w"], t["fmt"]
    if fmt in (0, 1):
        bpp = 3 if fmt == 0 else 4
        o = t["ptr"] + bpp * (y * w + x)
        return tuple(d[o:o + bpp]) + ((255,) if bpp == 3 else ())
    size = 8 if fmt == 4 else 16
    o = t["ptr"] + size * ((y // 4) * ((w + 3) // 4) + x // 4)
    k = (y % 4) * 4 + x % 4
    alpha = 255
    if fmt == 5:
        a0, a1 = d[o], d[o + 1]
        j = (int.from_bytes(d[o + 2:o + 8], "little") >> (3 * k)) & 7
        al = [a0, a1] + ([((8 - i) * a0 + (i - 1) * a1) // 7 for i in range(2, 8)] if a0 > a1 else
                         [((6 - i) * a0 + (i - 1) * a1) // 5 for i in range(2, 6)] + [0, 255])
        alpha = al[j]
        o += 8
    c0, c1, bits = struct.unpack_from("<HHI", d, o)
    j = (bits >> (2 * k)) & 3
    a, b = _565(c0), _565(c1)
    if j < 2:
        c = (a, b)[j]
    elif fmt == 5 or c0 > c1:
        c = tuple(((2 * a[i] + b[i]) if j == 2 else (a[i] + 2 * b[i])) // 3 for i in range(3))
    else:
        c = tuple((a[i] + b[i]) // 2 for i in range(3)) if j == 2 else (0, 0, 0)
    return c + (alpha,)


def samples(fn, t, n=16):
    """n x n texels at the same relative positions whatever the size."""
    return [fn(t, (2 * i + 1) * t["w"] // (2 * n), (2 * j + 1) * t["h"] // (2 * n)) for j in range(n) for i in range(n)]


def sample_err(a, b, f=lambda v: v):
    e = n = ea = 0
    for p, q in zip(a, b):
        ea += abs(p[3] - q[3])
        if q[3] >= 128:
            e += sum(abs(f(p[c]) - q[c]) for c in range(3)) / 3
            n += 1
    return (e / n if n else 0.0), ea / len(a)


def check_trees(ps2_root, gc_root, pc_root):
    """Pair every PS2 texture with the PC's and the GameCube's of the same pack, by content."""
    st = collections.Counter()
    hist = collections.Counter()
    for pj in sorted(Path(ps2_root).rglob("pack.json")):
        rel = pj.parent.relative_to(ps2_root)
        hp, dp = load(pj.parent)
        if not hp.get("num_textures"):
            continue
        P = ps2_headers(hp, dp)
        Ps = {t["i"]: samples(lambda t, x, y: ps2_texel(hp, dp, t, x, y), t) for t in P}
        for name, root, hdrs, texel, fn in (("PC", pc_root, pc_headers, pc_texel, lambda v: v),
                                             ("GC", gc_root, gc_headers, gc_texel, squared)):
            if root is None or not (Path(root) / rel / "data.bin").exists():
                continue
            ho, do = load(Path(root) / rel)
            O = [t for t in hdrs(ho, do) if t["fmt"] in ((0, 1, 4, 5) if name == "PC" else (1, 4))]
            Os = {t["i"]: samples(lambda t, x, y: texel(do, t, x, y), t) for t in O}
            st[f"{name} packs"] += 1
            st[f"{name} textures"] += len(O)
            cands = sorted((sample_err(Ps[a], Os[b], fn)[0], a, b) for a in Ps for b in Os)
            ua, ub = set(), set()
            for e, a, b in cands:
                if a in ua or b in ub:
                    continue
                ua.add(a)
                ub.add(b)
                pa, ob = P[a], next(t for t in O if t["i"] == b)
                ea = sample_err(Ps[a], Os[b], fn)[1]
                bucket = "<4" if e < 4 else "<8" if e < 8 else "<16" if e < 16 else ">=16"
                st[f"{name} paired, RGB err {bucket}"] += 1
                if bucket in ("<4", "<8"):
                    st[f"{name} paired (err<8): same size"] += (pa["w"], pa["h"]) == (ob["w"], ob["h"])
                    st[f"{name} paired (err<8): alpha err <8"] += ea < 8
                    if name == "PC":
                        lv = lambda m: m if m else 1
                        st["PC paired (err<8): same level count"] += lv(pa["levels"]) == lv(ob["mips"])
                        st[f"PC paired (err<8): PS2 fmt{pa['fmt']} -> PC fmt{ob['fmt']}"] += 1
                    else:
                        st[f"GC paired (err<8): PS2 fmt{pa['fmt']} -> GC fmt{ob['fmt']}"] += 1
                    if b - a not in (0, -1, 1):
                        hist[f"{name} index shift other"] += 1
                    else:
                        hist[f"{name} index = PS2 {b - a:+d}"] += 1
            st[f"{name}: PS2 textures without a partner"] += len(Ps) - len(ua)
            st[f"{name}: textures without a PS2 partner"] += len(Os) - len(ub)
        st["PS2 textures"] += len(P)
    for k in sorted(st):
        print(f"  {k}: {st[k]}")
    for k in sorted(hist):
        print(f"  {k}: {hist[k]}")

def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    png = None
    if "--png" in sys.argv:
        png = Path(sys.argv[sys.argv.index("--png") + 1])
        args.remove(str(png))
        png.mkdir(parents=True, exist_ok=True)
    if "--check" in sys.argv:
        check_trees(Path(args[0]), Path(args[1]) if len(args) > 1 else None, Path(args[2]) if len(args) > 2 else None)
        return 0
    if "--survey" in sys.argv:
        st = collections.Counter()
        for pj in sorted(Path(args[0]).rglob("pack.json")):
            h, d = load(pj.parent)
            if not h.get("num_textures"):
                continue
            for t in ps2_headers(h, d):
                st["format %d" % t["fmt"]] += 1
                st["levels field %d" % t["levels"]] += 1
                if t["fmt"] == FMT_PSMT8:
                    o = h["palette_offset"] + 0x400 * t["clut"]
                    st["CLUT index in range" if t["clut"] < h["num_palettes"] else "CLUT index out of range"] += 1
                    alphas = d[o + 3:o + 0x400:4]
                    st["CLUT alpha max %s" % ("0x80" if max(alphas) == 0x80 else "<0x80" if max(alphas) < 0x80 else ">0x80")] += 1
                else:
                    alphas = d[t["ptr"] + 3:t["ptr"] + 4 * t["w"] * t["h"]:4]
                    st["RGBA32 alpha max %s" % ("0x80" if max(alphas) == 0x80 else "<0x80" if max(alphas) < 0x80 else ">0x80")] += 1
                if t["levels"]:
                    full = max(t["w"], t["h"]).bit_length()
                    st["levels == log2(max)+1 (to 1x1)" if t["levels"] == full else "levels other"] += 1
        for k in sorted(st):
            print(f"  {k}: {st[k]}")
        return 0
    if "--compare" in sys.argv:
        hp, dp = load(args[0])
        hg, dg = load(args[1])
        P = [(t, ps2_decode(hp, dp, t)) for t in ps2_headers(hp, dp)]
        G = [(t, gc_decode(dg, t)) for t in gc_headers(hg, dg)]
        C = []
        if len(args) > 2:
            hc, dc = load(args[2])
            C = [(t, pc_decode(dc, t)) for t in pc_headers(hc, dc)]
        def sq(th):
            return [squared(v) if k % 4 != 3 else v for k, v in enumerate(th)]
        m = match_textures([(t["i"], sq(thumb(*img))) for t, img in P], [(t["i"], thumb(*img)) for t, img in G])
        mc = match_textures([(t["i"], thumb(*img)) for t, img in P], [(t["i"], thumb(*img)) for t, img in C]) if C else {}
        for t, img in P:
            line = f"PS2 t{t['i']} fmt{t['fmt']} {t['w']}x{t['h']} levels {t['levels']} clut {t['clut']}"
            if t["i"] in m:
                j, dist = m[t["i"]]
                g, gimg = G[j]
                a = resample(*img, gimg[0], gimg[1])
                e1, ea = compare((gimg[0], gimg[1], a), gimg)
                e2, _ = compare((gimg[0], gimg[1], a), gimg, squared)
                line += f" | GC t{j} fmt{g['fmt']} {g['w']}x{g['h']} mips {g['mips']} thumb {dist:.1f}: RGB err {e1:.1f}, squared {e2:.1f}, alpha {ea:.1f}"
                if png:
                    write_png(png / f"t{t['i']}_ps2.png", *img)
                    write_png(png / f"t{t['i']}_gc{j}.png", *gimg)
            if t["i"] in mc:
                j, dist = mc[t["i"]]
                c, cimg = C[j]
                a = resample(*img, cimg[0], cimg[1])
                e1, ea = compare((cimg[0], cimg[1], a), cimg)
                line += f" | PC t{j} fmt{c['fmt']} thumb {dist:.1f}: RGB err {e1:.1f}, alpha {ea:.1f}"
            print(line)
        return 0
    h, d = load(args[0])
    for t in ps2_headers(h, d):
        print(f"t{t['i']} fmt{t['fmt']} {t['w']}x{t['h']} levels {t['levels']} ptr 0x{t['ptr']:X} clut {t['clut']}")
        if png:
            write_png(png / f"t{t['i']}.png", *ps2_decode(h, d, t))
    return 0


if __name__ == "__main__":
    sys.exit(main())
