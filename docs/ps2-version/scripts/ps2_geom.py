#!/usr/bin/env python3
"""Decode the PS2 release's geometry (VIF/VU packets) into the port's indexed layout.

    python3 -I ps2_geom.py PACK_FOLDER [--dump]
    python3 -I ps2_geom.py --tree EXTRACTED_PS2_TREE

PACK_FOLDER is one pack of `hwvx-x extract orig/ps2/XPS/ENGLISH/<file>.xps <dir>` (data.bin and
pointers.bin, little-endian). The rules are in ../README.md §1. In short:

* a geometry: u16 0x1005, u16 numLevels, u16 skinned, pointer slots at 0x8 (levels) and 0xC
  (LOD distances), the GameCube's bounding box at 0x10..0x3F;
* a level (0x18): u16 kind (1 drawn, 2 skinned, 3 shadow), u16 numGroups, u32 vertex count,
  groups at 0x8 (pointer slot), 0xC/0x10 stale export-time addresses, names at 0x14;
* a group (0x20): u16 0, u16 flags, u8 colour[4], u32 0, packet at 0xC (pointer slot),
  triangle count at 0x10, blend 0x14, sort 0x16, texture 0x18 (relocated), stale address 0x1C;
* a packet: 16 zero bytes, then (drawn) one DMA `ret` tag and its VIF stream, or (skinned)
  records {u8 bone[3], u8 0, ptr subpacket} up to a null pointer, each subpacket a DMA `ret`
  tag and its VIF stream;
* a VIF stream: batches of STCYCL 4/4, UNPACK V4-32 x1 to VU address 0 (a GIF tag),
  STCYCL CL=3 WL=1, UNPACK V4-32 positions (address 1), V2-32 UVs (2), and V3-32 normals or
  V4-8 colours (3), then MSCNT. Vertex i of a batch is (pos[i], uv[i], attr[i]);
* the GIF tag's PRIM says strip (4) or list (3). In a strip the position's w bit 15 (ADC) is set
  on vertices that start a new strip and do not draw; any other vertex draws the triangle of the
  two before it, odd triangles of a run swapped. In a skinned list, w is a signed slot -1/0/1:
  the vertex's bone is record.bone[w + 1].

Only the Python standard library is used.
"""

import collections
import math
import struct
import sys
from pathlib import Path

PS2_GEOM = 0x1005
LEVEL_DRAWN, LEVEL_SKINNED, LEVEL_SHADOW = 1, 2, 3


class Pack:
    """One extracted pack: data.bin and its pointer slots (little-endian)."""

    def __init__(self, folder):
        self.folder = Path(folder)
        self.d = (self.folder / "data.bin").read_bytes()
        ptrs = self.folder / "pointers.bin"
        raw = ptrs.read_bytes() if ptrs.exists() else b""
        self.slots = set(struct.unpack("<%dI" % (len(raw) // 4), raw))

    def u16(self, o):
        return struct.unpack_from("<H", self.d, o)[0] if o + 2 <= len(self.d) else 0

    def u32(self, o):
        return struct.unpack_from("<I", self.d, o)[0] if o + 4 <= len(self.d) else 0

    def target(self, o):
        """The blob offset the pointer slot at `o` holds, or None."""
        if o not in self.slots:
            return None
        t = self.u32(o)
        return t if t < len(self.d) else None


def find_geometries(pk):
    """Geometry headers, found the way Port_Geometry.c finds GameCube ones (a scan)."""
    out = []
    for off in range(0, len(pk.d) - 0x40, 4):
        if pk.u16(off) != PS2_GEOM or (off + 8) not in pk.slots or (off + 0xC) not in pk.slots:
            continue
        n = pk.u16(off + 2)
        if 1 <= n <= 8 and pk.target(off + 8) is not None:
            out.append(off)
    return out


# ---------------------------------------------------------------------------------------------
# VIF


def vif_codes(d, start, end):
    """Yield (offset, cmd, num, imm, payload offset, payload size) for a VIF1 stream."""
    p = start
    while p + 4 <= end:
        w = struct.unpack_from("<I", d, p)[0]
        cmd, num, imm = (w >> 24) & 0x7F, (w >> 16) & 0xFF, w & 0xFFFF
        q, n = p + 4, 0
        if cmd & 0x60 == 0x60:                        # UNPACK
            vn, vl = (cmd >> 2) & 3, cmd & 3
            size = 2 if vl == 3 else (vn + 1) * (4 >> vl)
            n = ((num or 256) * size + 3) & ~3
        elif cmd == 0x20:                             # STMASK
            n = 4
        elif cmd in (0x30, 0x31):                     # STROW, STCOL
            n = 16
        elif cmd == 0x4A:                             # MPG
            n = (num or 256) * 8
        elif cmd in (0x50, 0x51):                     # DIRECT
            n = (imm or 65536) * 16
        yield p, cmd, num, imm, q, n
        p = q + n


class FormatError(ValueError):
    pass


def read_batches(pk, dma):
    """The batches of one DMA `ret` packet: [{prim, nloop, abe, pos, uv, nrm, col}]."""
    tag = pk.u32(dma)
    if (tag >> 28) & 7 != 6:
        raise FormatError("DMA tag id %d, not ret" % ((tag >> 28) & 7))
    end = dma + 16 + (tag & 0xFFFF) * 16
    if end > len(pk.d):
        raise FormatError("packet runs past the blob")
    batches, cur, cycle = [], None, None
    for off, cmd, num, imm, q, n in vif_codes(pk.d, dma + 8, end):
        if cmd == 0x01:
            cycle = imm
        elif cmd & 0x60 == 0x60:
            addr, count = imm & 0x3FF, num or 256
            if addr == 0:
                if cmd != 0x6C or count != 1 or cycle != 0x0404:
                    raise FormatError("GIF tag unpack 0x%02X x%d" % (cmd, count))
                lo, hi = struct.unpack_from("<QQ", pk.d, q)
                prim = (lo >> 47) & 0x7FF
                cur = {"nloop": lo & 0x7FFF, "prim": prim & 7, "abe": bool(prim & 0x40),
                       "nreg": (lo >> 60) & 0xF, "regs": hi, "n": 0}
                batches.append(cur)
                continue
            if cur is None or cycle != 0x0103:
                raise FormatError("vertex unpack outside a batch")
            if addr == 1 and cmd == 0x6C:     # V4-32 position, w = ADC flag or matrix slot
                cur["pos"] = [struct.unpack_from("<3fi", pk.d, q + 16 * i) for i in range(count)]
            elif addr == 2 and cmd == 0x64:   # V2-32 s, t
                cur["uv"] = [struct.unpack_from("<2f", pk.d, q + 8 * i) for i in range(count)]
            elif addr == 3 and cmd == 0x68:   # V3-32 normal
                cur["nrm"] = [struct.unpack_from("<3f", pk.d, q + 12 * i) for i in range(count)]
            elif addr == 3 and cmd == 0x6E:   # V4-8 colour (GS scale, 0x80 = 1.0)
                cur["col"] = [bytes(pk.d[q + 4 * i:q + 4 * i + 4]) for i in range(count)]
            else:
                raise FormatError("unpack 0x%02X to VU address %d" % (cmd, addr))
            cur["n"] = max(cur["n"], count)
        elif cmd in (0x00, 0x02, 0x03, 0x17):         # NOP, OFFSET, BASE, MSCNT
            pass
        else:
            raise FormatError("VIF command 0x%02X" % cmd)
    for b in batches:
        if "pos" not in b or "uv" not in b or ("nrm" in b) == ("col" in b):
            raise FormatError("batch without positions, UVs and exactly one of normals/colours")
        if not (len(b["pos"]) == len(b["uv"]) == len(b.get("nrm", b.get("col"))) == b["nloop"] * (b["nreg"] // 3)):
            raise FormatError("attribute counts differ from the GIF tag")
    return batches


# ---------------------------------------------------------------------------------------------
# Colours: the PS2 stores GS-scaled values; the GameCube's are 0..255.


def colour_to_gc(c):
    """PS2 vertex colour (R, G, B scaled 0x80 = 1.0; A = gc >> 2) -> GameCube u8 RGBA."""
    r, g, b, a = c
    return bytes((min(255, x * 2 + (x >> 6)) for x in (r, g, b))) + bytes((min(255, a * 4 + (a >> 4)),))


# ---------------------------------------------------------------------------------------------
# Triangles


def batch_triangles(b, bones=None):
    """Triangles of one batch as (v0, v1, v2, strip_id).

    A vertex is (pos, uv, nrm or None, col or None, bone). Strips: a vertex with ADC (w bit 15)
    starts a new strip and draws nothing; any other vertex draws (i-2, i-1, i), the odd
    triangles of a run swapped so all keep the run's winding. Lists: every three vertices."""
    nrm, col = b.get("nrm"), b.get("col")
    verts = []
    for i, (x, y, z, w) in enumerate(b["pos"]):
        bone = 0
        if bones is not None:
            if not -1 <= w <= 1 or bones[w + 1] == 0xFF:
                raise FormatError("matrix slot %d" % w)
            bone = bones[w + 1]
        verts.append(((x, y, z), b["uv"][i], nrm[i] if nrm else None, col[i] if col else None, bone))
    tris = []
    if b["prim"] == 4:                       # triangle strip
        run, strip = -1, -1
        for i, (_, _, _, w) in enumerate(b["pos"]):
            if w & 0x8000:                   # ADC: no triangle; the next drawing vertex starts a strip
                run = -1
                continue
            if i < 2:
                raise FormatError("strip vertex draws without two before it")
            if run < 0:
                run, strip = 0, strip + 1
            t = (verts[i - 2], verts[i - 1], verts[i]) if run % 2 == 0 else (verts[i - 1], verts[i - 2], verts[i])
            tris.append((t[0], t[1], t[2], strip))
            run += 1
    elif b["prim"] == 3:                     # triangle list
        if len(verts) % 3:
            raise FormatError("list of %d vertices" % len(verts))
        for i in range(0, len(verts), 3):
            tris.append((verts[i], verts[i + 1], verts[i + 2], i // 3))
    else:
        raise FormatError("GIF primitive %d" % b["prim"])
    return tris


def group_triangles(pk, grp, kind):
    """All triangles of a group, [(v0, v1, v2, strip_key)], in packet order."""
    pkt = pk.target(grp + 0xC)
    if pkt is None:
        raise FormatError("group packet is not a pointer")
    out = []
    if kind == LEVEL_SKINNED:
        r, rec = pkt + 0x10, 0
        while pk.u32(r + 4):
            sub = pk.target(r + 4)
            if sub is None:
                raise FormatError("skin record pointer is not a slot")
            bones = pk.d[r:r + 3]
            for bi, b in enumerate(read_batches(pk, sub)):
                out += [(a, b_, c, (rec, bi, s)) for a, b_, c, s in batch_triangles(b, bones)]
            r += 8
            rec += 1
    else:
        for bi, b in enumerate(read_batches(pk, pkt + 0x10)):
            out += [(a, b_, c, (bi, s)) for a, b_, c, s in batch_triangles(b)]
    return out


# ---------------------------------------------------------------------------------------------
# Winding. The PS2 does not cull, and 1-2% of its strips run the other way round; GameCube
# (and the port) cull back faces, so each strip is turned to agree with its neighbours.


def _sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def orient(tris):
    """Return (triangles with the GameCube's winding as far as it can be told, strips flipped).

    A consistently wound mesh walks an edge shared by two triangles in opposite directions.
    For each strip, count the directed edges it shares with the group's other strips: walked
    the same way ("same") or the opposite way ("opp"). A strip is reversed when same > opp,
    unless its face normals agree with its vertex normals (lit groups: the sum over its
    triangles of cos(face normal, n0 + n1 + n2) is positive). Normals alone decide nothing:
    the GameCube's own normals point inwards on about 1% of strips."""
    by_strip = collections.OrderedDict()
    for t in tris:
        by_strip.setdefault(t[3], []).append(t)
    edges = collections.defaultdict(set)
    for s, ts in by_strip.items():
        for a, b, c, _ in ts:
            for p, q in ((a, b), (b, c), (c, a)):
                edges[(p[0], q[0])].add(s)
    flipped = set()
    for s, ts in by_strip.items():
        same = opp = 0
        for a, b, c, _ in ts:
            for p, q in ((a, b), (b, c), (c, a)):
                same += len(edges.get((p[0], q[0]), set()) - {s})
                opp += len(edges.get((q[0], p[0]), set()) - {s})
        if same <= opp:
            continue
        vote = 0.0
        for a, b, c, _ in ts:
            if a[2] is None:
                continue
            n = _cross(_sub(b[0], a[0]), _sub(c[0], a[0]))
            m = (a[2][0] + b[2][0] + c[2][0], a[2][1] + b[2][1] + c[2][1], a[2][2] + b[2][2] + c[2][2])
            ln = math.sqrt(_dot(n, n) * _dot(m, m))
            if ln > 0:
                vote += _dot(n, m) / ln
        if vote <= 0:
            flipped.add(s)
    out = [(b, a, c, s) if s in flipped else (a, b, c, s) for a, b, c, s in tris]
    return out, len(flipped)


# ---------------------------------------------------------------------------------------------
# Geometry and the indexed layout


def gc_flags(ps2_flags, level_class):
    """GameCube group flags: the PS2's without the 0x16 bits on alpha groups, plus the class."""
    f = ps2_flags
    if f & 0x4000:
        return f
    if f & 0x1:
        f &= ~0x16
    if f & 0x8000:
        f = (f & ~0x8000) | 0x9000
        return f
    return f | level_class


def read_geometry(pk, off, fix_winding=True):
    """A geometry as {header fields, levels: [{kind, groups: [{flags, colour, ..., tris}]}]}."""
    nlod, skinned = pk.u16(off + 2), pk.u16(off + 4)
    levels_at, dist_at = pk.target(off + 8), pk.target(off + 0xC)
    geom = {"offset": off, "skinned": skinned, "bbox": pk.d[off + 0x10:off + 0x40],
            "lod_distances": [struct.unpack_from("<f", pk.d, dist_at + 4 * i)[0] for i in range(nlod)]
            if dist_at is not None else [], "levels": []}
    for i in range(nlod):
        L = levels_at + 0x18 * i
        kind, ng, groups = pk.u16(L), pk.u16(L + 2), pk.target(L + 8)
        lvl = {"kind": kind, "vertex_count": pk.u32(L + 4), "groups": [], "names": pk.target(L + 0x14)}
        geom["levels"].append(lvl)
        if groups is None:
            raise FormatError("level groups not a pointer")
        for k in range(ng):
            G = groups + 0x20 * k
            grp = {"offset": G, "flags": pk.u16(G + 2), "colour": pk.d[G + 4:G + 8],
                   "triangle_count": pk.u32(G + 0x10), "blend": pk.u16(G + 0x14),
                   "sort": pk.u16(G + 0x16), "texture_slot": pk.u32(G + 0x18), "tris": []}
            lvl["groups"].append(grp)
            if kind == LEVEL_SHADOW or grp["flags"] & 0x4000:
                continue
            tris = group_triangles(pk, G, kind)
            if fix_winding:
                tris, grp["strips_flipped"] = orient(tris)
            grp["tris"] = tris
        # level class: which arrays the GameCube level had (0x800 nrm, 0x1000 col, 0x2000 both)
        has_n = any(t[0][2] is not None for g in lvl["groups"] for t in g["tris"][:1])
        has_c = any(t[0][3] is not None for g in lvl["groups"] for t in g["tris"][:1])
        lvl["class"] = 0x2000 if has_n and has_c else 0x800 if has_n else 0x1000
        for g in lvl["groups"]:
            g["gc_flags"] = gc_flags(g["flags"], lvl["class"])
    return geom


def to_indexed(level, skinned):
    """The port's indexed layout for one level: arrays, per-group u16 triangle lists, bone counts.

    One vertex per distinct (pos, uv, normal, colour, bone) in first-use order; colours are
    converted to the GameCube's scale; a level with both arrays gives lit vertices white and
    unlit vertices a zero normal (neither is read when drawing them). Skinned: sorted by bone."""
    has_n = any(t[0][2] is not None for g in level["groups"] for t in g["tris"])
    has_c = any(t[0][3] is not None for g in level["groups"] for t in g["tris"])
    verts, index = [], {}
    lists = []
    for g in level["groups"]:
        idx = []
        for a, b, c, _ in g["tris"]:
            for v in (a, b, c):
                n = v[2] if v[2] is not None else ((0.0, 0.0, 0.0) if has_n else None)
                col = colour_to_gc(v[3]) if v[3] is not None else (b"\xff\xff\xff\xff" if has_c else None)
                key = (v[0], v[1], n, col, v[4])
                if key not in index:
                    index[key] = len(verts)
                    verts.append(key)
                idx.append(index[key])
        lists.append(idx)
    counts = [0] * 8
    if skinned:
        order = sorted(range(len(verts)), key=lambda i: verts[i][4])
        remap = {old: new for new, old in enumerate(order)}
        verts = [verts[i] for i in order]
        lists = [[remap[i] for i in l] for l in lists]
        for v in verts:
            counts[v[4]] += 1
    if len(verts) > 0x7FFF:
        raise FormatError("more than 0x7FFF vertices in a level")
    return {"positions": [v[0] for v in verts], "uvs": [v[1] for v in verts],
            "normals": [v[2] for v in verts] if has_n else None,
            "colours": [v[3] for v in verts] if has_c else None,
            "indices": lists, "bone_counts": counts if skinned else None}


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    if sys.argv[1] == "--tree":
        st = collections.Counter()
        for data in sorted(Path(sys.argv[2]).rglob("data.bin")):
            pk = Pack(data.parent)
            for off in find_geometries(pk):
                st["geometries"] += 1
                try:
                    g = read_geometry(pk, off)
                except FormatError as e:
                    st["error: %s" % e] += 1
                    continue
                for lvl in g["levels"]:
                    st["levels kind %d" % lvl["kind"]] += 1
                    if lvl["kind"] == LEVEL_SHADOW:
                        continue
                    ix = to_indexed(lvl, g["skinned"] != 0)
                    st["triangles"] += sum(len(l) for l in ix["indices"]) // 3
                    st["max vertices in a level"] = max(st["max vertices in a level"], len(ix["positions"]))
                    for grp in lvl["groups"]:
                        st["strips flipped"] += grp.get("strips_flipped", 0)
                        if grp["tris"] and len(grp["tris"]) != grp["triangle_count"]:
                            st["group triangle count mismatch"] += 1
        for k, v in st.items():
            print(f"  {k}: {v}")
        return 0
    pk = Pack(sys.argv[1])
    for off in find_geometries(pk):
        g = read_geometry(pk, off)
        print(f"geometry @0x{off:X} levels {len(g['levels'])} skinned {g['skinned']}")
        for i, lvl in enumerate(g["levels"]):
            ix = to_indexed(lvl, g["skinned"] != 0) if lvl["kind"] != LEVEL_SHADOW else None
            print(f"  level {i} kind {lvl['kind']} groups {len(lvl['groups'])} class 0x{lvl['class']:X}"
                  + (f" vertices {len(ix['positions'])} (header {lvl['vertex_count']})" if ix else ""))
            if "--dump" in sys.argv:
                for g_ in lvl["groups"]:
                    print(f"    flags 0x{g_['flags']:04X} -> GC 0x{g_['gc_flags']:04X} tris {len(g_['tris'])}"
                          f"/{g_['triangle_count']} texture 0x{g_['texture_slot']:08X}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
