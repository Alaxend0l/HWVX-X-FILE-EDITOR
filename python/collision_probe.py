#!/usr/bin/env python3
"""Probe a collision pack (type 5) object blob against the runtime's claims.

The GC runtime swaps and reads collision packs in Port_Swap.c:Claim_Collision,
which roots at the blob base: TerrainGrid { origin f32[3] 0x00, cells ptr 0x20,
size 0x28..0x30, numX/Y/Z 0x34/0x38/0x3C, cell index 0x40 (s32 per grid cell),
polys 0x4C, edges 0x54, mode 0x58 (1: BSP), materials 0x5C }.  A TerrainCell is
0x50 bytes { min 0x00, max 0x10, nodes 0x30, polys 0x38, numPolys 0x3C,
numEdges 0x40, edges 0x44 }; TerrainBSPNode 0x20; TerrainPoly 0x20;
TerrainEdge 0x30; materials is an array of u32 pointers to surfaces
{ u8 type, u16 skidTexture, f32 grip 0x14, f32 restitution 0x18 }.

Usage: python3 collision_probe.py <extracted pack dir> [...]
"""
import json
import struct
import sys


def load(d):
    j = json.load(open(f"{d}/pack.json"))
    man = json.load(open(f"{d.rsplit('/', 1)[0]}/manifest.json"))
    data = open(f"{d}/data.bin", "rb").read()
    e = ">" if man["platform"] == "gc" else "<"
    try:
        praw = open(f"{d}/pointers.bin", "rb").read()
    except FileNotFoundError:
        praw = b""
    ptrs = {struct.unpack_from(e + "I", praw, i)[0] for i in range(0, len(praw) - 3, 4)}
    return j, data, e, ptrs


def probe(d):
    j, data, e, ptrs = load(d)
    n = len(data)
    w = lambda o: struct.unpack_from(e + "I", data, o)[0] if o + 4 <= n else None
    h = lambda o: struct.unpack_from(e + "H", data, o)[0] if o + 2 <= n else None
    f = lambda o: struct.unpack_from(e + "f", data, o)[0] if o + 4 <= n else None
    P = lambda o: w(o)

    name = d.split("/")[-1]
    man = json.load(open(f"{d.rsplit('/', 1)[0]}/manifest.json"))
    ent = next((x for x in man["entries"] if x["path"] == name), {})
    print(f"== {name} [{d}]")
    print(f"   platform={man['platform']} archive={man['archive_version']} pack={ent.get('version')} "
          f"type={ent.get('type')} id={ent.get('id')} sub_id={ent.get('sub_id')}")
    print(f"   json keys: {sorted(j.keys())}")
    print(f"   blob={n:#x}  pointer slots={len(ptrs)}")
    nx, ny, nz = (w(o) or 0 for o in (0x34, 0x38, 0x3C))
    print(f"   origin=({f(0):.3f},{f(4):.3f},{f(8):.3f})  mode@0x58={w(0x58)}")
    print(f"   size=({f(0x28):.3f},{f(0x2C):.3f},{f(0x30):.3f})  num=({nx},{ny},{nz})")
    fields = [("cells", 0x20), ("index", 0x40), ("polys", 0x4C), ("edges", 0x54), ("materials", 0x5C)]
    for nm, off in fields:
        v = P(off)
        print(f"   {nm:<9}@0x{off:02X} = {v:#010x} {'in range' if v is not None and v < n else 'BAD'}"
              f" {'[ptr slot]' if off in ptrs else ''}")
    cells, index = P(0x20), P(0x40)
    polys, edges, mats = P(0x4C), P(0x54), P(0x5C)
    total = nx * ny * nz if 0 < nx * ny * nz < 0x400000 else 0
    if index not in (None, ~0) and total:
        vals = [w(index + i * 4) for i in range(total)]
        ok = [v for v in vals if v is not None and v < 0x1000000]
        print(f"   index: {total} entries, -1 count={sum(1 for v in vals if v == ~0)}, "
              f"min={min(ok, default=0):#x} max={max(ok, default=0):#x}")
        if ok and cells:
            print(f"   -> cell array from {cells:#x}: (max-cells)/0x50+1 = {(max(ok) - cells) // 0x50 + 1} records")
    if cells:
        end = min(x for x in (polys, edges, mats, n) if x)
        print(f"   cell region {cells:#x}..{end:#x} = {(end - cells) // 0x50} records of 0x50 "
              f"({(end - cells) % 0x50} left over)")
        c = cells
        for i in (0, 1):
            c = cells + i * 0x50
            print(f"   cell{i} min=({f(c):.2f},{f(c+4):.2f},{f(c+8):.2f}) max=({f(c+0x10):.2f},{f(c+0x14):.2f},{f(c+0x18):.2f})")
            row = []
            for o in range(0x20, 0x50, 4):
                row.append(f"+0x{o:02X}={w(c + o):#010x}{'*' if c + o in ptrs else ' '}")
            print("      " + " ".join(row))
    for nm, addr, size, fmt in (("poly", polys, 0x20, "poly"), ("edge", edges, 0x30, "edge")):
        if not addr:
            continue
        a = addr
        if fmt == "poly":
            print(f"   poly0 plane=({f(a):.4g},{f(a+4):.4g},{f(a+8):.4g},{f(a+0xC):.4g}) verts={P(a+0x10):#x} "
                  f"numVerts={w(a+0x14)} sweep={w(a+0x18)} material={w(a+0x1C)}")
        else:
            print(f"   edge0 a=({f(a):.4g},{f(a+4):.4g},{f(a+8):.4g}) b=({f(a+0x10):.4g},{f(a+0x14):.4g},{f(a+0x18):.4g}) "
                  f"sweep={w(a+0x20)} material={w(a+0x24)} pad={data[a+0x28:a+0x30].hex()}")
    if mats:
        m0 = P(mats)
        if m0 and m0 + 0x1C <= n:
            print(f"   mat0 -> {m0:#x} type={data[m0]} skidTex={h(m0+2)} grip={f(m0+0x14)} rest={f(m0+0x18)}")
            print(f"        bytes={data[m0:m0+0x1C].hex()}")
        slots = (n - mats) // 4
        vals = [P(mats + i * 4) for i in range(slots)]
        print(f"   materials @ {mats:#x}: {slots} slots, {sum(1 for v in vals if v == ~0)} nulls, "
              f"first={[hex(v) for v in vals[:6]]}")
    print()


for d in sys.argv[1:]:
    probe(d)
