#!/usr/bin/env python3
"""Check every GameCube geometry of an extracted tree against the indexed-layout conversion.

    python3 geom_check.py EXTRACTED_GC_TREE [--verbose]
    python3 geom_check.py --pc EXTRACTED_PC_TREE [--verbose]

EXTRACTED_GC_TREE is `hwvx-x extract orig/gc/files <dir>`. With --pc, the tree is the PC
release's (`hwvx-x extract orig/pc <dir>`) and the check is the one HWVX-SDL's Port_Data.c
PC_MarkGeometry makes before drawing PC geometry as it is: every level of a header 0x0002 with
groups whose index lists and vertex buffer (positions, UVs) are pointers. For each pack folder (data.bin and
pointers.bin, GameCube byte order) this finds the geometry headers the way HWVX-SDL's
src/game/Port_Geometry.c does (u16 0x2003 with pointer slots at 0x8 and 0xC), walks every level's
display lists with the same rules, and reports what converts, what would stay on display lists
and why, and the vertex counts the indexed layout needs. It is the reference for that converter
and for a future offline .x* compiler: same rules, no game code.

Rules (docs/port-format/README.md):
* a level: u16 0, u16 numGroups, groups at 0x8 (0x20 each), vertex buffer at 0xC;
  a shadow level has groups flagged 0x4000 and is left alone;
* a vertex buffer: u32 offsets {uv, colours or 0, normals or 0, end}, arrays from +0x10
  (positions and normals 12 bytes, UVs 8, colours 4);
* a display list: commands 0x90 (triangles), 0x98 (strip), 0xA0 (fan) | vertex format,
  u16 count, then per vertex three u16 indices A, B, C; 0x00 is padding;
* in skinned geometry (header u16 at 0x4 nonzero) a lit or env-mapped, non-alpha group's
  vertices start with a u8 position-matrix index (GX PNMTXIDX, bone = index / 3);
* a lit or env-mapped group (flags & 0x16, not 0x81): position A, normal B, UV C;
  any other group: position A, colour B, UV C.
"""

import collections
import json
import struct
import sys
from pathlib import Path

GC_HEADER = 0x2003
MAX_VERTS = 0x7FFF


class Blob:
    def __init__(self, data: bytes, slots: set[int]):
        self.d = data
        self.slots = slots

    def u16(self, off):
        return struct.unpack_from(">H", self.d, off)[0] if off + 2 <= len(self.d) else 0

    def u32(self, off):
        return struct.unpack_from(">I", self.d, off)[0] if off + 4 <= len(self.d) else 0

    def target(self, off):
        """The blob offset the pointer slot at `off` holds, or None."""
        if off not in self.slots:
            return None
        t = self.u32(off)
        return t if t < len(self.d) else None


def arrays(b: Blob, vb: int):
    uv, clr, nrm, end = (b.u32(vb + i) for i in (0, 4, 8, 12))
    data = vb + 0x10
    if data + end > len(b.d) or uv > end:
        return None

    def bound(start):
        return min([o for o in (uv, clr, nrm) if o > start] + [end])
    return {
        "data": data,
        "pos": uv // 12,
        "uv": (bound(uv) - uv) // 8,
        "clr": (bound(clr) - clr) // 4 if clr else 0,
        "nrm": (bound(nrm) - nrm) // 12 if nrm else 0,
        "has_clr": clr != 0,
        "has_nrm": nrm != 0,
    }


def convert_level(b: Blob, lod: int, skinned: bool):
    """(vertex count, triangle count, bones used) or raises ValueError(reason)."""
    ng, groups, vb = b.u16(lod + 2), b.target(lod + 8), b.target(lod + 0xC)
    if groups is None or vb is None:
        raise ValueError("groups or vertex buffer not a pointer")
    a = arrays(b, vb)
    if a is None:
        raise ValueError("vertex buffer outside the blob")
    verts, tris, bones = {}, 0, collections.Counter()
    for g in range(ng):
        grp = groups + g * 0x20
        flags, size, dl = b.u16(grp + 2), b.u32(grp + 0x10), b.target(grp + 0x1C)
        if dl is None or dl + size > len(b.d):
            raise ValueError("display list outside the blob")
        lit = bool(flags & 0x16) and not (flags & 0x81)
        mtx = skinned and lit
        p, end = dl, dl + size
        while p < end:
            cmd = b.d[p]
            p += 1
            if cmd == 0:
                continue
            prim = cmd & 0xF8
            if prim not in (0x90, 0x98, 0xA0):
                raise ValueError(f"display-list command 0x{cmd:02X}")
            count = b.u16(p)
            p += 2
            stride = 7 if mtx else 6
            if p + count * stride > end:
                raise ValueError("display list runs past its size")
            for _ in range(count):
                m = 0
                if mtx:
                    m = b.d[p]
                    p += 1
                A, B, C = struct.unpack_from(">HHH", b.d, p)
                p += 6
                if A >= a["pos"] or C >= a["uv"] or (lit and a["has_nrm"] and B >= a["nrm"]) or \
                        (not lit and a["has_clr"] and B >= a["clr"]):
                    raise ValueError("index out of its array")
                if mtx and m // 3 >= 8:
                    raise ValueError(f"matrix index {m} (bone {m // 3}) past 8 bones")
                key = (lit, m // 3, A, B, C)
                if key not in verts:
                    verts[key] = len(verts)
                    bones[m // 3] += 1
            tris += count // 3 if prim == 0x90 else max(count - 2, 0)
    if len(verts) > MAX_VERTS:
        raise ValueError("more than 0x7FFF vertices")
    return len(verts), tris, bones


def check_pack(folder: Path, report):
    data = (folder / "data.bin").read_bytes()
    raw = (folder / "pointers.bin").read_bytes() if (folder / "pointers.bin").exists() else b""
    slots = {struct.unpack_from(">I", raw, i)[0] for i in range(0, len(raw) - 3, 4)}
    b = Blob(data, slots)
    for off in range(0, len(data) - 0x40, 4):
        if b.u16(off) != GC_HEADER or (off + 8) not in slots or (off + 0xC) not in slots:
            continue
        nlod, skinned, lods = b.u16(off + 2), b.u16(off + 4) != 0, b.target(off + 8)
        if not 1 <= nlod <= 8 or lods is None:
            continue
        report["geometries"] += 1
        report["skinned geometries"] += skinned
        for i in range(nlod):
            lod = lods + i * 0x18
            groups = b.target(lod + 8)
            if b.u16(lod) != 0 or groups is None or not 1 <= b.u16(lod + 2) <= 0x400:
                report["levels not drawable"] += 1
                continue
            if b.u16(groups + 2) & 0x4000:
                report["shadow levels"] += 1
                continue
            try:
                nv, nt, bones = convert_level(b, lod, skinned)
            except ValueError as e:
                report["levels left to display lists"] += 1
                report["why"][str(e)] += 1
                report["where"].append(f"{folder} @0x{off:X} level {i}: {e}")
                continue
            report["levels converted"] += 1
            report["triangles"] += nt
            report["max vertices in a level"] = max(report["max vertices in a level"], nv)
            if skinned:
                report["skinned levels converted"] += 1
                report["bones"].update(bones)


def check_pc_pack(folder: Path, report):
    data = (folder / "data.bin").read_bytes()
    raw = (folder / "pointers.bin").read_bytes() if (folder / "pointers.bin").exists() else b""
    slots = {struct.unpack_from("<I", raw, i)[0] for i in range(0, len(raw) - 3, 4)}

    def u16(o):
        return struct.unpack_from("<H", data, o)[0] if o + 2 <= len(data) else 0

    def target(o):
        if o not in slots or o + 4 > len(data):
            return None
        t = struct.unpack_from("<I", data, o)[0]
        return t if t < len(data) else None

    for off in range(0, len(data) - 0x40, 4):
        if u16(off) != 0x0002 or (off + 8) not in slots or (off + 0xC) not in slots:
            continue
        n, lods = u16(off + 2), target(off + 8)
        if not 1 <= n <= 8 or lods != off + 0x40:   # the PC's levels always follow the header
            continue
        report["geometries"] += 1
        report["skinned geometries"] += u16(off + 4) != 0
        bad = None
        for i in range(n):
            lod = lods + i * 0x18
            ng, groups, vb = u16(lod + 2), target(lod + 8), target(lod + 0xC)
            if u16(lod) != 0 or not 1 <= ng <= 0x400 or groups is None or vb is None:
                bad = "level header"
                break
            if u16(groups + 2) & 0x4000:
                report["shadow levels"] += 1
                continue
            if target(vb) is None or target(vb + 8) is None:
                bad = "vertex buffer without positions or UVs"
                break
            for g in range(ng):
                idx = target(groups + g * 0x20 + 0x1C)
                if idx is None:
                    bad = "group without an index list"
                    break
                if u16(idx) & 0x8000:
                    report["groups drawn as strips"] += 1
            if bad:
                break
            report["levels marked"] += 1
        if bad:
            report["geometries left unmarked"] += 1
            report["why"][bad] += 1
            report["where"].append(f"{folder} @0x{off:X}: {bad}")


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    pc = "--pc" in sys.argv
    root = Path(next(a for a in sys.argv[1:] if not a.startswith("--")))
    report = collections.Counter()
    report["why"] = collections.Counter()
    report["bones"] = collections.Counter()
    report["where"] = []
    report["max vertices in a level"] = 0
    packs = 0
    for data in sorted(root.rglob("data.bin")):
        folder = data.parent
        manifest = next((p / "manifest.json" for p in folder.parents if (p / "manifest.json").exists()), None)
        if manifest is not None and json.loads(manifest.read_text()).get("platform", "gc") != ("pc" if pc else "gc"):
            continue
        packs += 1
        (check_pc_pack if pc else check_pack)(folder, report)
    where = report.pop("where")
    print(f"packs {packs}")
    for k, v in report.items():
        print(f"  {k}: {dict(v) if isinstance(v, collections.Counter) else v}")
    if "--verbose" in sys.argv:
        for w in where:
            print("   ", w)
    return 0


if __name__ == "__main__":
    sys.exit(main())
