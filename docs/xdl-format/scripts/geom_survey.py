#!/usr/bin/env python3
"""Compare every geometry of the GameCube, PC and PS2 releases: levels, triangles, vertices,
attributes, skinning.

    python3 geom_survey.py SURVEY_ROOT [--jobs N]     writes SURVEY_ROOT/geom_survey.jsonl
    python3 geom_survey.py SURVEY_ROOT --summary      prints the tables of ../ultimate.md §2

SURVEY_ROOT holds `hwvx-x extract` trees gc/, pc/, ps2/ (survey_common.py).

Geometries are found by the releases' header scans (GameCube 0x2003, PC 0x0002 with levels at
header + 0x40, PS2 0x1005; ../../port-format, ../../ps2-version) and paired by index within a
pack (the scans find them in the same order). Per drawn level: groups, triangles, distinct
positions (bit-exact f32 triples: comparable across releases whatever each exporter's vertex
sharing), distinct vertices as each release stores them, and whether the level carries normals
and colours. Shadow levels are counted, not decoded.
"""

import collections
import json
import struct
import sys
from multiprocessing import Pool

import survey_common as S
import ps2_geom as P
import ps2_geom_check as PC_GC  # GameCube display-list walker (gc_group_tris)
from geom_check import arrays, Blob


def gc_survey(folder):
    b = PC_GC.GC(folder)
    out = []
    for off in PC_GC.gc_geometries(b):
        nlod, skinned, lods = b.u16(off + 2), b.u16(off + 4) != 0, b.target(off + 8)
        g = {"levels": [], "skinned": skinned,     # bbox in the PC/PS2 byte order, to compare
             "bbox": struct.pack("<12I", *struct.unpack_from(">12I", b.d, off + 0x10)).hex()}
        for i in range(nlod):
            L = lods + 0x18 * i
            ng, groups, vb = b.u16(L + 2), b.target(L + 8), b.target(L + 0xC)
            if groups is None:
                g["levels"].append({"kind": "bad"})
                continue
            if b.u16(groups + 2) & 0x4000:
                g["levels"].append({"kind": "shadow", "groups": ng})
                continue
            a = arrays(Blob(b.d, b.slots), vb) if vb is not None else None
            pos, verts, tris = set(), set(), 0
            try:
                for k in range(ng):
                    for t in PC_GC.gc_group_tris(b, L, k, skinned):
                        tris += 1
                        for v in t:
                            pos.add(v[0])
                            verts.add(v)
            except (struct.error, TypeError, IndexError):
                g["levels"].append({"kind": "bad"})
                continue
            g["levels"].append({"kind": "drawn", "groups": ng, "tris": tris, "pos": len(pos), "verts": len(verts),
                                "nrm": bool(a and a["has_nrm"]), "clr": bool(a and a["has_clr"])})
        out.append(g)
    return out


def pc_survey(folder):
    d = (folder / "data.bin").read_bytes()
    raw = (folder / "pointers.bin").read_bytes() if (folder / "pointers.bin").exists() else b""
    slots = set(struct.unpack("<%dI" % (len(raw) // 4), raw))
    u16 = lambda o: struct.unpack_from("<H", d, o)[0] if o + 2 <= len(d) else 0  # noqa: E731
    u32 = lambda o: struct.unpack_from("<I", d, o)[0] if o + 4 <= len(d) else 0  # noqa: E731

    def target(o):
        if o not in slots:
            return None
        t = u32(o)
        return t if t < len(d) else None
    out = []
    for off in range(0, len(d) - 0x40, 4):
        if u16(off) != 0x0002 or (off + 8) not in slots or (off + 0xC) not in slots:
            continue
        n, lods = u16(off + 2), target(off + 8)
        if not 1 <= n <= 8 or lods != off + 0x40:
            continue
        g = {"levels": [], "skinned": u16(off + 4) != 0, "bbox": d[off + 0x10:off + 0x40].hex()}
        for i in range(n):
            L = lods + 0x18 * i
            ng, groups, vb = u16(L + 2), target(L + 8), target(L + 0xC)
            if groups is None or vb is None:
                g["levels"].append({"kind": "bad"})
                continue
            if u16(groups + 2) & 0x4000:
                g["levels"].append({"kind": "shadow", "groups": ng})
                continue
            pp, cp, up, npp = (target(vb + 4 * k) for k in range(4))
            idx, tris = set(), 0
            for k in range(ng):
                G = groups + 0x20 * k
                cnt, il = u32(G + 0x10), target(G + 0x1C)
                if il is None:
                    continue
                tris += cnt
                idx.update(struct.unpack_from("<%dH" % (3 * cnt), d, il))
            pos = {d[pp + 16 * j:pp + 16 * j + 12] for j in idx} if pp is not None else set()
            g["levels"].append({"kind": "drawn", "groups": ng, "tris": tris, "pos": len(pos), "verts": len(idx),
                                "nrm": npp is not None, "clr": cp is not None})
        out.append(g)
    return out


def ps2_survey(folder):
    pk = P.Pack(folder)
    out = []
    for off in P.find_geometries(pk):
        try:
            geo = P.read_geometry(pk, off, fix_winding=False)
        except Exception:  # noqa: BLE001
            out.append({"levels": [], "bad": True})
            continue
        g = {"levels": [], "skinned": geo["skinned"] != 0, "bbox": geo["bbox"].hex()}
        for lvl in geo["levels"]:
            if lvl["kind"] == P.LEVEL_SHADOW:
                g["levels"].append({"kind": "shadow", "groups": len(lvl["groups"])})
                continue
            pos, verts, tris, nrm, clr = set(), set(), 0, False, False
            for grp in lvl["groups"]:
                for t in grp["tris"]:
                    tris += 1
                    for v in t[:3]:
                        pos.add(struct.pack("<3f", *v[0]))
                        verts.add(v[:5])
                        nrm |= v[2] is not None
                        clr |= v[3] is not None
            g["levels"].append({"kind": "drawn", "groups": len(lvl["groups"]), "tris": tris, "pos": len(pos),
                                "verts": len(verts), "nrm": nrm, "clr": clr})
        out.append(g)
    return out


def survey_pack(job):
    key, folders = job
    rec = {"archive": key[0], "pack": key[1], "cat": S.category(key[0])}
    for rel, f in folders.items():
        if not (f / "data.bin").exists():
            continue
        rec[rel] = {"gc": gc_survey, "pc": pc_survey, "ps2": ps2_survey}[rel](f)
    return rec


def jobs(root):
    arcs = {r: S.archives(root / r) for r in S.RELEASES}
    out = []
    for a in sorted(set().union(*arcs.values())):
        per = {}
        for r in S.RELEASES:
            if a in arcs[r]:
                for name, folder, _ in S.packs(arcs[r][a]):
                    per.setdefault(name, {})[r] = folder
        out += [((a, n), f) for n, f in per.items()]
    return out


def align(A, B):
    """Pair two releases' geometry lists of a pack: by bounding box (bit-equal across releases
    unless edited) in order, falling back to position for equal-length unmatched runs."""
    import difflib
    sm = difflib.SequenceMatcher(None, [g.get("bbox") for g in A], [g.get("bbox") for g in B], autojunk=False)
    out = []
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag == "equal" or (tag == "replace" and i2 - i1 == j2 - j1):
            out += list(zip(A[i1:i2], B[j1:j2]))
    return out


def summary(root):
    C = collections.Counter
    recs = [json.loads(line) for line in (root / "geom_survey.jsonl").read_text().splitlines()]
    st = collections.defaultdict(C)
    tot = collections.defaultdict(C)
    where = collections.defaultdict(list)
    for r in recs:
        rels = [x for x in S.RELEASES if x in r]
        for x in rels:
            for g in r[x]:
                tot[x]["geometries"] += 1
                tot[x]["skinned"] += bool(g.get("skinned"))
                for lv in g["levels"]:
                    tot[x]["levels " + lv["kind"]] += 1
                    if lv["kind"] == "drawn":
                        tot[x]["triangles"] += lv["tris"]
                        tot[x]["positions"] += lv["pos"]
                        tot[x]["levels with normals"] += lv["nrm"]
                        tot[x]["levels with colours"] += lv["clr"]
        if len(rels) < 2:
            continue
        cat = r["cat"]
        n = {x: len(r[x]) for x in rels}
        if len(set(n.values())) > 1:
            st[cat]["packs with different geometry counts: " + " ".join(f"{x}={n[x]}" for x in rels)] += 1
        for a in rels:
            for b in rels:
                if a >= b:
                    continue
                pair = f"{a}-{b}"
                pairs = align(r[a], r[b])
                for x, n_ in ((a, len(r[a])), (b, len(r[b]))):
                    if n_ > len(pairs):
                        st[cat][f"{pair} geometries without a partner in {x}"] += n_ - len(pairs)
                        where[f"{pair}: {x}"].append(f"{r['archive']}/{r['pack']} {n_ - len(pairs)}")
                for ga, gb in pairs:
                    st[cat][f"{pair} geometries paired"] += 1
                    if ga["levels"] and gb["levels"] and (len(ga["levels"]) != len(gb["levels"]) or any(
                            la.get("tris") != lb.get("tris") for la, lb in zip(ga["levels"], gb["levels"]))):
                        where[f"{pair}: changed"].append(f"{r['archive']}/{r['pack']}")
                    if len(ga["levels"]) != len(gb["levels"]):
                        st[cat][f"{pair} level count differs ({a} {len(ga['levels'])}, {b} {len(gb['levels'])})"] += 1
                        continue
                    if ga.get("skinned") != gb.get("skinned"):
                        st[cat][f"{pair} skinned differs"] += 1
                    for la, lb in zip(ga["levels"], gb["levels"]):
                        if la["kind"] != lb["kind"]:
                            st[cat][f"{pair} level kind differs"] += 1
                            continue
                        if la["kind"] != "drawn":
                            continue
                        st[cat][f"{pair} drawn levels"] += 1
                        for f in ("tris", "pos", "groups"):
                            if la[f] != lb[f]:
                                st[cat][f"{pair} {f} differ"] += 1
                                st[cat][f"{pair} {f} more in {a if la[f] > lb[f] else b}"] += 1
                                st[cat][f"{pair} {f} net {a}-{b}"] += la[f] - lb[f]
                        for f in ("nrm", "clr"):
                            if la[f] != lb[f]:
                                st[cat][f"{pair} {f} only in {a if la[f] else b}"] += 1
    print("## totals")
    keys = sorted(set().union(*tot.values()))
    for k in keys:
        print(f"  {k:28s} " + "  ".join(f"{x} {tot[x][k]:9d}" for x in S.RELEASES))
    for cat in sorted(st):
        print(f"\n## {cat}")
        for k in sorted(st[cat]):
            print(f"  {k}: {st[cat][k]}")
    print("\n## where (packs with unpaired or changed geometries)")
    for k in sorted(where):
        c = C(where[k])
        print(f"  {k}: " + ", ".join(f"{p} x{n}" if n > 1 else p for p, n in sorted(c.items())))


def main():
    root = S.survey_root(sys.argv[1:])
    if "--summary" in sys.argv:
        summary(root)
        return 0
    n = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 8
    js = jobs(root)
    with Pool(n) as p, open(root / "geom_survey.jsonl", "w") as f:
        for rec in p.imap_unordered(survey_pack, js):
            f.write(json.dumps(rec) + "\n")
    print(f"{len(js)} packs -> {root / 'geom_survey.jsonl'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
