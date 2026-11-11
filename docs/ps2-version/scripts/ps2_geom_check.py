#!/usr/bin/env python3
"""Pair every PS2 geometry with the GameCube's and compare what ps2_geom.py decodes.

    python3 -I ps2_geom_check.py EXTRACTED_PS2_TREE EXTRACTED_GC_TREE [--no-fix] [--verbose] [PACK_SUBSTRING ...]

Both trees are `hwvx-x extract` output with the same pack folder names. Geometries pair by
index (the PS2 scan for 0x1005 and the GameCube scan for 0x2003 find them in the same order).
Per pair: header (bounding box, LOD distances, level and group counts), group flags and
colours, and per group the triangle multiset:

* positions bit-exact, UVs equal up to a whole-number shift per triangle, normals bit-exact,
  vertex colours through the documented scale (ps2 = gc >> 1, alpha ps2 = gc >> 2);
* winding: a triangle is "same" when its vertex cycle equals the GameCube's, "reversed" when
  it equals the reversed cycle. --no-fix compares the packet's own strip order.
* skinned: each vertex's bone (record.bone[w + 1]) against the GameCube's pnmtx / 3.
"""

import collections
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ps2_geom as P  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "port-format" / "scripts"))


class GC:
    def __init__(self, folder):
        f = Path(folder)
        self.d = (f / "data.bin").read_bytes()
        raw = (f / "pointers.bin").read_bytes() if (f / "pointers.bin").exists() else b""
        self.slots = set(struct.unpack(">%dI" % (len(raw) // 4), raw))

    def u16(self, o):
        return struct.unpack_from(">H", self.d, o)[0]

    def u32(self, o):
        return struct.unpack_from(">I", self.d, o)[0]

    def target(self, o):
        if o not in self.slots:
            return None
        t = self.u32(o)
        return t if t < len(self.d) else None


def gc_geometries(b):
    out = []
    for off in range(0, len(b.d) - 0x40, 4):
        if b.u16(off) != 0x2003 or (off + 8) not in b.slots or (off + 0xC) not in b.slots:
            continue
        if 1 <= b.u16(off + 2) <= 8 and b.target(off + 8) is not None:
            out.append(off)
    return out


def gc_group_tris(b, lod, k, skinned):
    """GameCube triangles of group k: [(v0, v1, v2)], v = (pos, uv, nrm|None, col|None, bone)."""
    groups, vb = b.target(lod + 8), b.target(lod + 0xC)
    uv, clr, nrm, _ = (b.u32(vb + i) for i in (0, 4, 8, 12))
    data = vb + 0x10
    grp = groups + 0x20 * k
    flags, size, dl = b.u16(grp + 2), b.u32(grp + 0x10), b.target(grp + 0x1C)
    lit = bool(flags & 0x16) and not (flags & 0x81)
    mtx = skinned and lit
    p, e, tris = dl, dl + size, []
    while p < e:
        cmd = b.d[p]
        p += 1
        if cmd == 0:
            continue
        prim, count = cmd & 0xF8, b.u16(p)
        p += 2
        vs = []
        for _ in range(count):
            m = 0
            if mtx:
                m = b.d[p]
                p += 1
            A, B, C = struct.unpack_from(">HHH", b.d, p)
            p += 6
            vs.append((struct.unpack_from(">3f", b.d, data + 12 * A),
                       struct.unpack_from(">2f", b.d, data + uv + 8 * C),
                       struct.unpack_from(">3f", b.d, data + nrm + 12 * B) if lit and nrm else None,
                       bytes(b.d[data + clr + 4 * B:data + clr + 4 * B + 4]) if not lit and clr else None,
                       m // 3))
        if prim == 0x90:
            tris += [(vs[i], vs[i + 1], vs[i + 2]) for i in range(0, count - 2, 3)]
        elif prim == 0x98:
            tris += [(vs[i], vs[i + 1], vs[i + 2]) if i % 2 == 0 else (vs[i + 1], vs[i], vs[i + 2])
                     for i in range(count - 2)]
        elif prim == 0xA0:
            tris += [(vs[0], vs[i], vs[i + 1]) for i in range(1, count - 1)]
    return tris


def canon(cyc):
    i = cyc.index(min(cyc))
    return cyc[i:] + cyc[:i]


def vkey(v, uvshift=(0, 0)):
    return v[0]


def compare_group(pt, gt, st, colours):
    """Match PS2 triangles to GameCube ones by position cycle; check attributes and winding."""
    gc_by = collections.defaultdict(list)
    for t in gt:
        gc_by[frozenset(v[0] for v in t)].append(t)
    for a, b, c, _ in pt:
        cand = gc_by.get(frozenset((a[0], b[0], c[0])))
        if not cand:
            st["tri: no GameCube triangle with these positions"] += 1
            continue
        mine = canon((a[0], b[0], c[0]))
        # double-sided surfaces have the same triangle both ways round: take the same-winding one
        g = next((x for x in cand if canon(tuple(v[0] for v in x)) == mine), cand[-1])
        cand.remove(g)
        theirs = canon(tuple(v[0] for v in g))
        if len({a[0], b[0], c[0]}) < 3:
            st["tri: degenerate (winding not comparable)"] += 1
        elif mine == theirs:
            st["tri winding same"] += 1
        elif mine == canon(tuple(v[0] for v in g[::-1])):
            st["tri winding reversed"] += 1
        gv = {v[0]: v for v in g}
        shifts, ok_uv, ok_attr, ok_bone = set(), True, True, True
        for v in (a, b, c):
            w = gv[v[0]]
            du, dv = v[1][0] - w[1][0], v[1][1] - w[1][1]
            ru, rv = round(du), round(dv)
            shifts.add((ru, rv))
            if abs(du - ru) > 1e-4 or abs(dv - rv) > 1e-4:
                ok_uv = False
            if w[2] is not None and v[2] != w[2]:
                ok_attr = False
            if w[3] is not None:
                if v[3] is None:
                    ok_attr = False
                else:
                    colours[tuple(zip(w[3], v[3]))] += 1
                    if tuple(v[3]) != (w[3][0] >> 1, w[3][1] >> 1, w[3][2] >> 1, w[3][3] >> 2):
                        ok_attr = False
            if v[4] != w[4]:
                ok_bone = False
        st["tri uv equal" if shifts == {(0, 0)} and ok_uv else
           "tri uv whole-number shift" if ok_uv and len(shifts) == 1 else "tri uv differ"] += 1
        st["tri attr (normal/colour) match" if ok_attr else "tri attr differ"] += 1
        if not ok_bone:
            st["tri bone differ"] += 1
    left = sum(len(v) for v in gc_by.values())
    if left:
        st["tri: GameCube triangle without a PS2 one"] += left


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) < 2:
        print(__doc__)
        return 2
    fix = "--no-fix" not in sys.argv
    ps2_root, gc_root, filt = Path(args[0]), Path(args[1]), args[2:]
    st = collections.Counter()
    colours = collections.Counter()
    where = []
    for data in sorted(ps2_root.rglob("data.bin")):
        rel = data.parent.relative_to(ps2_root)
        if filt and not any(f in str(rel) for f in filt):
            continue
        if not (gc_root / rel / "data.bin").exists():
            st["packs without a GameCube pair"] += 1
            continue
        pk, gb = P.Pack(data.parent), GC(gc_root / rel)
        pg, gg = P.find_geometries(pk), gc_geometries(gb)
        st["packs"] += 1
        if len(pg) != len(gg):
            st["packs with a different geometry count"] += 1
            where.append(f"{rel}: {len(pg)} PS2 vs {len(gg)} GC geometries")
            continue
        for po, go in zip(pg, gg):
            st["geometries"] += 1
            if pk.d[po + 0x10:po + 0x40] != b"".join(
                    struct.pack("<I", gb.u32(go + i)) for i in range(0x10, 0x40, 4)):
                st["geometry bbox differs"] += 1
            try:
                geo = P.read_geometry(pk, po, fix_winding=fix)
            except P.FormatError as e:
                st["geometry not decodable"] += 1
                where.append(f"{rel} @0x{po:X}: {e}")
                continue
            gskin = gb.u16(go + 4) != 0
            if len(geo["levels"]) != gb.u16(go + 2):
                st["level count differs"] += 1
                continue
            gl = gb.target(go + 8)
            for i, lvl in enumerate(geo["levels"]):
                GL = gl + 0x18 * i
                st["levels"] += 1
                if len(lvl["groups"]) != gb.u16(GL + 2):
                    st["level group count differs"] += 1
                    where.append(f"{rel} @0x{po:X} level {i}: group count")
                    continue
                ggr = gb.target(GL + 8)
                gclass = gb.u16(ggr + 2) & 0x3800
                if lvl["kind"] == P.LEVEL_SHADOW:
                    st["shadow levels"] += 1
                    continue
                if (lvl["class"] if gclass != 0x1000 or lvl["class"] != 0x2000 else 0x1000) != gclass and gclass:
                    st["level class differs (0x%X vs GC 0x%X)" % (lvl["class"], gclass)] += 1
                for k, grp in enumerate(lvl["groups"]):
                    G = ggr + 0x20 * k
                    gflags = gb.u16(G + 2)
                    st["groups"] += 1
                    if grp["gc_flags"] == gflags:
                        st["group flags reproduced"] += 1
                    elif (grp["gc_flags"] & ~0x3800) == (gflags & ~0x3800):
                        st["group flags reproduced but class bits"] += 1
                    else:
                        st["group flags differ"] += 1
                        where.append(f"{rel} @0x{po:X} level {i} group {k}: flags 0x{grp['flags']:X} -> 0x{grp['gc_flags']:X}, GC 0x{gflags:X}")
                    if grp["colour"] != gb.d[G + 4:G + 8]:
                        st["group colour differs"] += 1
                    if gflags & 0x4000:
                        continue
                    gt = gc_group_tris(gb, GL, k, gskin)
                    st["triangles GC"] += len(gt)
                    st["triangles PS2"] += len(grp["tris"])
                    if len(gt) != grp["triangle_count"]:
                        st["group triangle count (0x10) differs from GC"] += 1
                    compare_group(grp["tris"], gt, st, colours)
                    st["strips flipped"] += grp.get("strips_flipped", 0)
    print(f"PS2 {ps2_root} vs GC {gc_root}, winding fix {'on' if fix else 'off'}")
    for k in sorted(st):
        print(f"  {k}: {st[k]}")
    print("  colour channel pairs (GC -> PS2), most common:")
    ch = [collections.Counter() for _ in range(4)]
    for pairs, n in colours.items():
        for c, (g, p) in enumerate(pairs):
            ch[c][(g, p)] += n
    for c in range(4):
        bad = sum(n for (g, p), n in ch[c].items() if p != (g >> (2 if c == 3 else 1)))
        off = collections.Counter({k: n for k, n in ch[c].items() if k[1] != (k[0] >> (2 if c == 3 else 1))})
        print(f"    channel {'RGBA'[c]}: {sum(ch[c].values())} values, {bad} off the rule; {ch[c].most_common(6)}; off: {off.most_common(5)}")
    if "--verbose" in sys.argv:
        for w in where:
            print("   ", w)
    return 0


if __name__ == "__main__":
    sys.exit(main())
