#!/usr/bin/env python3
"""Probe a world pack (type 4) against the runtime's World struct (World.h).

The world root object sits at blob offset 0.  Fields the loader follows:
collision 0x00, volumeHolder 0x04, numTileCars 0x08/tileCars 0x0C,
numQuads 0x20/quads 0x24, speakers 0x28/0x2C, teleporters 0x30/0x34,
lights 0x38/0x3C, creatures 0x40/0x44, variables 0x48/0x4C, timers 0x50/0x54,
objectives 0x58/0x5C, lights 0x64, volumes 0x68/0x6C, nav 0x78,
collisionPackName 0x7C + collisionPackID 0x80, quadPackName 0x84 +
quadPackID 0x88, name 0x8C (the quad packs are in <name>.xgc), id 0x90,
gameData 0x9C.  A WorldQuad record is 0x50 bytes with numGeoms at +0x0C and
packId at +0x38 (beta v179 records are 0x40 bytes, no packId).

Usage: python3 world_probe.py <extracted pack dir> [...]
"""
import json
import struct
import sys


def load(d):
    man = json.load(open(f"{d.rsplit('/', 1)[0]}/manifest.json"))
    data = open(f"{d}/data.bin", "rb").read()
    e = ">" if man["platform"] == "gc" else "<"
    praw = open(f"{d}/pointers.bin", "rb").read() if __import__("os").path.exists(f"{d}/pointers.bin") else b""
    ptrs = {struct.unpack_from(e + "I", praw, i)[0] for i in range(0, len(praw) - 3, 4)}
    return man, data, e, ptrs


def cstr(data, off, n=16):
    raw = data[off:off + n]
    nul = raw.find(b"\0")
    return raw[:nul if nul >= 0 else n].decode("latin1")


def probe(d):
    man, data, e, ptrs = load(d)
    n = len(data)
    w = lambda o: struct.unpack_from(e + "I", data, o)[0] if o + 4 <= n else None
    s = lambda o: struct.unpack_from(e + "i", data, o)[0] if o + 4 <= n else None
    f = lambda o: struct.unpack_from(e + "f", data, o)[0] if o + 4 <= n else None

    name_ = d.split("/")[-1]
    ent = next((x for x in man["entries"] if x["path"] == name_), {})
    print(f"== {name_} [{d}]")
    print(f"   platform={man['platform']} archive={man['archive_version']} pack={ent.get('version')} "
          f"type={ent.get('type')} id={ent.get('id')} sub_id={ent.get('sub_id')} blob={n:#x}")
    for nm, off in [("collision", 0x00), ("volumeHolder", 0x04), ("tileCars", 0x0C), ("tilePaths", 0x14),
                    ("quads", 0x24), ("speakers", 0x2C), ("teleporters", 0x34), ("tileLights", 0x3C),
                    ("tileCreatures", 0x44), ("variables", 0x4C), ("timers", 0x54), ("objectives", 0x5C),
                    ("lights", 0x64), ("volumes", 0x6C), ("nav", 0x78), ("collisionPackName", 0x7C),
                    ("quadPackName", 0x84), ("name", 0x8C), ("gameData", 0x9C)]:
        v = w(off)
        extra = ""
        if v and v + 4 <= n and all(32 <= b < 127 for b in data[v:v + 4]) and (data[v:v + 16].find(b"\0") > 0):
            extra = f" -> {cstr(data, v)!r}"
        print(f"   {nm:<18}@0x{off:02X} = {v:#010x}{' [ptr]' if off in ptrs else ''}{extra}")
    for nm, off in [("numTileCars", 0x08), ("numTilePaths", 0x10), ("numQuads", 0x20), ("numSpeakers", 0x28),
                    ("numTeleporters", 0x30), ("numTileLights", 0x38), ("numTileCreatures", 0x40),
                    ("numVariables", 0x48), ("numTimers", 0x50), ("numObjectives", 0x58), ("numVolumes", 0x68)]:
        print(f"   {nm:<18}@0x{off:02X} = {s(off)}")
    print(f"   collisionPackID@0x80 = {s(0x80)}  quadPackID@0x88 = {s(0x88)}  id@0x90 = {s(0x90)}")

    stride = 0x50 if man["platform"] in ("gc", "ps2") else 0x40
    nq, qp = w(0x20), w(0x24)
    print(f"   quad records: {nq} at {qp:#x}, stride 0x{stride:X}")
    for i in range(min(nq or 0, 40)):
        q = qp + i * stride
        cells = w(q + 0x10)
        row = [w(q + o) for o in (0x00, 0x0C, 0x10, 0x14, 0x18, 0x1C, 0x20, 0x24, 0x28, 0x2C, 0x30, 0x34, 0x38, 0x3C)]
        pk = w(q + 0x38) if stride == 0x50 else None
        print(f"   quad[{i:<2}] id={row[0]:>7} numGeoms={row[1]:<3} numCells={cells:<4} "
              f"min=({f(q+0x14):.0f},{f(q+0x18):.0f},{f(q+0x1C):.0f}) max=({f(q+0x20):.0f},{f(q+0x24):.0f},{f(q+0x28):.0f}) "
              + (f"packId={pk:#010x} (low16={pk & 0xFFFF}, high16={pk >> 16})" if pk is not None else f"words={[hex(v) for v in row[10:]]}"))
    print()


for d in sys.argv[1:]:
    probe(d)
