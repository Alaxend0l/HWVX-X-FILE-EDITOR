#!/usr/bin/env python3
"""Pair every texture across the GameCube, PC and PS2 releases and measure which copy is best.

    python3 tex_survey.py SURVEY_ROOT [--jobs N]      writes SURVEY_ROOT/tex_survey.jsonl
    python3 tex_survey.py SURVEY_ROOT --summary       prints the tables of ../ultimate.md §1

SURVEY_ROOT holds `hwvx-x extract` trees `gc/`, `pc/`, `ps2/` (survey_common.py). Needs numpy.

Per pack present in two or more releases, the top level of every texture is decoded into the
PC/PS2 colour space (GameCube RGB -> sqrt(255 * v), the inverse of its squaring) and textures are
paired by content: an 8x8 box thumbnail of premultiplied RGB, greedy on mean RGB distance (index distance breaks ties),
accepted under 24. GameCube is the hub (GC<->PC, GC<->PS2), leftover PC<->PS2 are paired after.

Per group the record keeps each release's format, size, mip count, unique colours, and:
* `ref`: a lossless copy if any release has one: PS2 PSMCT32 or GameCube RGBA8 (lossless in the
  GameCube's own squared space, so errors against it are taken there: others are squared first);
* `err`: RGB RMSE (texels with alpha >= 128 in the reference) and alpha MAE of every lossy copy
  against the reference, at equal size;
* `cons`: without a reference and with three equal-size copies, each copy's RGB RMSE to the
  per-texel median of the three;
* `pair_rmse` / `pair_rmse_gc`: RGB RMSE between every two copies, in the PC space and in the
  GameCube's squared space; `--summary` solves them per texture for each copy's own error
  (three-cornered hat: d(a,b)^2 = s_a^2 + s_b^2 when the copies' errors are independent);
* `pair_alpha`: alpha RMSE between every two copies (all texels);
* `grad`: mean |luma gradient| inside 4x4 blocks (block seams excluded, so DXT/CMPR block edges
  don't count as detail), per copy at equal size.
"""

import collections
import json
import sys
from multiprocessing import Pool
from pathlib import Path

import numpy as np

import survey_common as S
import ps2_tex as T

# ---------------------------------------------------------------------------------------------
# vectorised decoders: return uint8 [h, w, 4]


def _565(c):
    r = ((c >> 11) & 31) * 255 // 31
    g = ((c >> 5) & 63) * 255 // 63
    b = (c & 31) * 255 // 31
    return np.stack([r, g, b], -1).astype(np.int32)


def _palette4(c0, c1, three_mode_alpha):
    """c0, c1 u16 arrays [...]; -> palette [..., 4, 4] int (RGBA)."""
    a, b = _565(c0), _565(c1)
    p2 = (2 * a + b) // 3
    p3 = (a + 2 * b) // 3
    m = (c0 <= c1)[..., None]
    p2 = np.where(m, (a + b) // 2, p2)
    p3 = np.where(m, 0, p3)
    alpha = np.full(c0.shape + (4,), 255, np.int32)
    if three_mode_alpha:
        alpha[..., 3] = np.where(c0 <= c1, 0, 255)
    pal = np.stack([a, b, p2, p3], -2)
    return np.concatenate([pal, alpha[..., None]], -1)


def decode_dxt(d, ptr, w, h, fmt):
    bw, bh = (w + 3) // 4, (h + 3) // 4
    size = 16 if fmt == 5 else 8
    blk = np.frombuffer(d, np.uint8, bw * bh * size, ptr).reshape(bh, bw, size).astype(np.int64)
    if fmt == 5:
        a0, a1 = blk[..., 0], blk[..., 1]
        bits = sum(blk[..., 2 + k] << (8 * k) for k in range(6))
        idx = np.stack([(bits >> (3 * k)) & 7 for k in range(16)], -1)  # [bh, bw, 16]
        k = np.arange(8)
        a0e, a1e = a0[..., None], a1[..., None]
        eight = np.where(k == 0, a0e, np.where(k == 1, a1e, ((8 - k) * a0e + (k - 1) * a1e) // 7))
        six = np.where(k == 0, a0e, np.where(k == 1, a1e, ((6 - k) * a0e + (k - 1) * a1e) // 5))
        six = np.where(k == 6, 0, np.where(k == 7, 255, six))
        al = np.where((a0 > a1)[..., None], eight, six)
        alpha = np.take_along_axis(al, idx, -1)
        blk = blk[..., 8:]
    c0 = blk[..., 0] | (blk[..., 1] << 8)
    c1 = blk[..., 2] | (blk[..., 3] << 8)
    pal = _palette4(c0, c1, False)
    if fmt == 5:   # DXT5 colour block is always 4-colour
        a, b = _565(c0), _565(c1)
        pal[..., 2, :3] = (2 * a + b) // 3
        pal[..., 3, :3] = (a + 2 * b) // 3
    idx = np.stack([(blk[..., 4 + y] >> (2 * x)) & 3 for y in range(4) for x in range(4)], -1)
    px = np.take_along_axis(pal, idx[..., None].repeat(4, -1), -2)  # [bh, bw, 16, 4]
    if fmt == 5:
        px[..., 3] = alpha
    img = px.reshape(bh, bw, 4, 4, 4).transpose(0, 2, 1, 3, 4).reshape(bh * 4, bw * 4, 4)
    return img[:h, :w].astype(np.uint8)


def decode_cmpr(d, ptr, w, h):
    tw, th = (w + 7) // 8, (h + 7) // 8
    blk = np.frombuffer(d, np.uint8, tw * th * 32, ptr).reshape(th, tw, 2, 2, 8).astype(np.int64)
    c0 = (blk[..., 0] << 8) | blk[..., 1]
    c1 = (blk[..., 2] << 8) | blk[..., 3]
    pal = _palette4(c0, c1, True)
    idx = np.stack([(blk[..., 4 + y] >> (6 - 2 * x)) & 3 for y in range(4) for x in range(4)], -1)
    px = np.take_along_axis(pal, idx[..., None].repeat(4, -1), -2)  # [th, tw, sy, sx, 16, 4]
    img = px.reshape(th, tw, 2, 2, 4, 4, 4).transpose(0, 2, 4, 1, 3, 5, 6).reshape(th * 8, tw * 8, 4)
    return img[:h, :w].astype(np.uint8)


def decode_gc_rgba8(d, ptr, w, h):
    tw, th = (w + 3) // 4, (h + 3) // 4
    blk = np.frombuffer(d, np.uint8, tw * th * 64, ptr).reshape(th, tw, 2, 16, 2)
    a, r = blk[:, :, 0, :, 0], blk[:, :, 0, :, 1]
    g, b = blk[:, :, 1, :, 0], blk[:, :, 1, :, 1]
    px = np.stack([r, g, b, a], -1)  # [th, tw, 16, 4]
    img = px.reshape(th, tw, 4, 4, 4).transpose(0, 2, 1, 3, 4).reshape(th * 4, tw * 4, 4)
    return img[:h, :w]


def decode_ps2(h, d, t):
    w, ht = t["w"], t["h"]
    if t["fmt"] == T.FMT_PSMCT32:
        return np.frombuffer(d, np.uint8, w * ht * 4, t["ptr"]).reshape(ht, w, 4)
    o = h["palette_offset"] + 0x400 * t["clut"]
    clut = np.frombuffer(d, np.uint8, 0x400, o).reshape(256, 4)
    perm = np.array([T.csm1(i) for i in range(256)])
    pal = clut[perm]
    idx = np.frombuffer(d, np.uint8, w * ht, t["ptr"]).reshape(ht, w)
    return pal[idx]


GC_TO_PC = np.round(np.sqrt(255.0 * np.arange(256))).astype(np.uint8)


def to_pc_space(img):
    out = img.copy()
    out[..., :3] = GC_TO_PC[img[..., :3]]
    return out


FMT_NAMES = {("gc", 1): "RGBA8", ("gc", 4): "CMPR", ("pc", 4): "DXT1", ("pc", 5): "DXT5",
             ("pc", 0): "RGB", ("pc", 1): "RGBA", ("ps2", 1): "PSMCT32", ("ps2", 2): "PSMT8"}
LOSSLESS = {"PSMCT32", "RGBA8", "RGB", "RGBA"}


def load_release(rel, folder):
    h, d = T.load(folder)
    if not h.get("num_textures"):
        return []
    out = []
    hdrs = {"gc": T.gc_headers, "pc": T.pc_headers, "ps2": T.ps2_headers}[rel](h, d)
    for t in hdrs:
        try:
            if rel == "gc":
                img = decode_cmpr(d, t["ptr"], t["w"], t["h"]) if t["fmt"] == 4 else \
                    decode_gc_rgba8(d, t["ptr"], t["w"], t["h"])
                native = img
                img = to_pc_space(img)
            elif rel == "pc":
                img = decode_dxt(d, t["ptr"], t["w"], t["h"], t["fmt"])
                native = img
            else:
                img = decode_ps2(h, d, t)
                native = img
        except Exception as e:  # noqa: BLE001
            print(f"{folder} t{t['i']}: {e}", file=sys.stderr)
            continue
        lv = t.get("mips", t.get("levels", 0))
        out.append({"i": t["i"], "fmt": FMT_NAMES.get((rel, t["fmt"]), str(t["fmt"])), "w": t["w"],
                    "h": t["h"], "levels": lv if lv else 1, "img": img, "native": native})
    return out


def thumb(img, n=8):
    h, w = img.shape[:2]
    ys = (np.arange(n + 1) * h) // n
    xs = (np.arange(n + 1) * w) // n
    f = img.astype(np.float32)
    f[..., :3] *= f[..., 3:] / 255     # premultiplied: the colour of transparent texels is noise
    out = np.zeros((n, n, 4), np.float32)
    for j in range(n):
        for i in range(n):
            out[j, i] = f[ys[j]:max(ys[j + 1], ys[j] + 1), xs[i]:max(xs[i + 1], xs[i] + 1)].reshape(-1, 4).mean(0)
    return out


def pair(A, B, limit=24.0):
    """Greedy content pairing of two texture lists; -> {index in A: (index in B, distance)}."""
    if not A or not B:
        return {}
    ta = np.stack([a["thumb"][..., :3].ravel() for a in A])
    tb = np.stack([b["thumb"][..., :3].ravel() for b in B])
    dist = np.abs(ta[:, None, :] - tb[None, :, :]).mean(-1)
    ia = np.array([a["i"] for a in A])[:, None]
    ib = np.array([b["i"] for b in B])[None, :]
    cost = dist + 0.01 * np.minimum(np.abs(ia - ib), 50)
    order = np.argsort(cost, axis=None)
    ua, ub, out = set(), set(), {}
    for flat in order:
        i, j = divmod(int(flat), len(B))
        if dist[i, j] >= limit:
            break
        if i in ua or j in ub:
            continue
        ua.add(i)
        ub.add(j)
        out[i] = (j, float(dist[i, j]))
    return out


def grad_energy(img):
    """Mean |luma gradient| between neighbours inside the same 4x4 block, opaque texels."""
    f = img.astype(np.float32)
    y = 0.299 * f[..., 0] + 0.587 * f[..., 1] + 0.114 * f[..., 2]
    h, w = y.shape
    dx = np.abs(np.diff(y, axis=1))
    dy = np.abs(np.diff(y, axis=0))
    mx = (np.arange(w - 1) % 4) != 3
    my = (np.arange(h - 1) % 4) != 3
    vals = np.concatenate([dx[:, mx].ravel(), dy[my, :].ravel()])
    return float(vals.mean()) if vals.size else 0.0


def rgb_rmse(a, ref, gc_space=False):
    fa, fr = a.astype(np.float64), ref.astype(np.float64)
    if gc_space:
        fa[..., :3] = fa[..., :3] ** 2 / 255
        fr[..., :3] = fr[..., :3] ** 2 / 255
    m = ref[..., 3] >= 128
    if not m.any():
        m = np.ones(ref.shape[:2], bool)
    e = float(np.sqrt(((fa[..., :3] - fr[..., :3])[m] ** 2).mean()))
    ea = float(np.abs(fa[..., 3] - fr[..., 3]).mean())
    return e, ea


def ncolours(img):
    v = img.reshape(-1, 4).astype(np.uint32)
    return int(np.unique((v[:, 0] << 24) | (v[:, 1] << 16) | (v[:, 2] << 8) | v[:, 3]).size)


def survey_pack(job):
    key, folders = job
    texs = {}
    for rel, f in folders.items():
        lst = load_release(rel, f)
        for t in lst:
            t["thumb"] = thumb(t["img"])
        texs[rel] = lst
    if not any(texs.values()):
        return []
    groups = []   # dicts rel -> texture
    used = {r: set() for r in texs}
    gc = texs.get("gc", [])
    for r in ("pc", "ps2"):
        if r not in texs:
            continue
        m = pair(gc, texs[r])
        for i, (j, dist) in m.items():
            g = next((g for g in groups if g.get("gc") is gc[i]), None)
            if g is None:
                g = {"gc": gc[i]}
                groups.append(g)
                used["gc"].add(i)
            g[r] = texs[r][j]
            g["d_" + r] = dist
            used[r].add(j)
    if "pc" in texs and "ps2" in texs:
        la = [t for k, t in enumerate(texs["pc"]) if k not in used["pc"]]
        lb = [t for k, t in enumerate(texs["ps2"]) if k not in used["ps2"]]
        for i, (j, dist) in pair(la, lb).items():
            groups.append({"pc": la[i], "ps2": lb[j], "d_ps2pc": dist})
            used["pc"].add(texs["pc"].index(la[i]))
            used["ps2"].add(texs["ps2"].index(lb[j]))
    for r, lst in texs.items():
        for k, t in enumerate(lst):
            if k not in used[r]:
                groups.append({r: t})
    out = []
    for g in groups:
        rels = [r for r in S.RELEASES if r in g]
        rec = {"archive": key[0], "pack": key[1], "cat": S.category(key[0]),
               "present": [r for r in S.RELEASES if r in folders]}
        for r in rels:
            t = g[r]
            rec[r] = {"i": t["i"], "fmt": t["fmt"], "w": t["w"], "h": t["h"], "levels": t["levels"],
                      "colours": ncolours(t["img"]),
                      "alpha": int(t["img"][..., 3].min()) < 255}
        for k in ("d_pc", "d_ps2", "d_ps2pc"):
            if k in g:
                rec[k] = round(g[k], 2)
        same = [r for r in rels if (g[r]["w"], g[r]["h"]) == max((g[x]["w"], g[x]["h"]) for x in rels)]
        rec["largest"] = same
        eq = len({(g[r]["w"], g[r]["h"]) for r in rels}) == 1
        rec["same_size"] = eq
        if len(rels) > 1 and eq:
            rec["grad"] = {r: round(grad_energy(g[r]["img"]), 3) for r in rels}
            ref = None
            if "ps2" in g and g["ps2"]["fmt"] == "PSMCT32":
                ref = "ps2"
            elif "gc" in g and g["gc"]["fmt"] == "RGBA8":
                ref = "gc"
            if ref:
                rec["ref"] = ref
                rec["err"] = {}
                for r in rels:
                    if r == ref:
                        continue
                    if ref == "gc":
                        e, ea = rgb_rmse(g[r]["img"], g["gc"]["img"], gc_space=True)
                    else:
                        e, ea = rgb_rmse(g[r]["img"], g[ref]["img"])
                    rec["err"][r] = [round(e, 2), round(ea, 2)]
            elif len(rels) == 3:
                stack = np.stack([g[r]["img"].astype(np.float32) for r in rels])
                med = np.median(stack, 0)
                m = med[..., 3] >= 128
                if not m.any():
                    m = np.ones(med.shape[:2], bool)
                rec["cons"] = {r: round(float(np.sqrt(((g[r]["img"][..., :3].astype(np.float32) - med[..., :3])[m] ** 2).mean())), 2)
                               for r in rels}
            if len(rels) >= 2:
                # pairwise RGB RMSE, a size-independent sanity check of the pairing
                rec["pair_rmse"], rec["pair_rmse_gc"], rec["pair_alpha"] = {}, {}, {}
                for a in rels:
                    for b in rels:
                        if a < b:
                            rec["pair_rmse"][a + "-" + b] = round(rgb_rmse(g[a]["img"], g[b]["img"])[0], 2)
                            rec["pair_rmse_gc"][a + "-" + b] = round(rgb_rmse(g[a]["img"], g[b]["img"], True)[0], 2)
                            da = g[a]["img"][..., 3].astype(np.float32) - g[b]["img"][..., 3]
                            rec["pair_alpha"][a + "-" + b] = round(float(np.sqrt((da ** 2).mean())), 2)
        out.append(rec)
    return out


def jobs(root):
    arcs = {r: S.archives(root / r) for r in S.RELEASES}
    names = sorted(set().union(*arcs.values()))
    out = []
    for a in names:
        per = {}
        for r in S.RELEASES:
            if a in arcs[r]:
                for name, folder, _ in S.packs(arcs[r][a]):
                    per.setdefault(name, {})[r] = folder
        for name, folders in per.items():
            out.append(((a, name), folders))
    return out


# ---------------------------------------------------------------------------------------------
# summary


def hat(d2):
    """Three-cornered hat: squared pair distances (gc-pc, gc-ps2, pc-ps2) -> each copy's s^2."""
    gp, gs, ps = d2
    return np.array([(gp + gs - ps) / 2, (gp + ps - gs) / 2, (gs + ps - gp) / 2])


def rule(r, look="pc"):
    """The recommended source of a texture group (../ultimate.md §1.6)."""
    rels = [x for x in S.RELEASES if x in r]
    if len(rels) == 1:
        return rels[0]
    if not r["same_size"]:
        return max(rels, key=lambda x: r[x]["w"] * r[x]["h"])
    if "ps2" in r and r["ps2"]["fmt"] == "PSMCT32":
        return "ps2"
    if "gc" in r and r["gc"]["fmt"] == "RGBA8":
        return "gc" if look == "gc" else "ps2" if "ps2" in r else "gc"
    alpha = any(r[x]["alpha"] for x in rels) or ("pc" in r and r["pc"]["fmt"] == "DXT5")
    if alpha:
        if "ps2" in r and not ("pc" in r and r["pc"]["colours"] > 1024):
            return "ps2"
        return "pc" if "pc" in r else rels[0]
    if look == "gc" and "gc" in r:
        return "gc"
    return "pc" if "pc" in r else "gc" if "gc" in r else "ps2"


def summary(root):
    recs = [json.loads(line) for line in (root / "tex_survey.jsonl").read_text().splitlines()]
    C = collections.Counter
    print(f"texture groups: {len(recs)}")
    by = C()
    for r in recs:
        rels = tuple(x for x in S.RELEASES if x in r)
        by[(r["cat"], "+".join(rels))] += 1
    print("\n## presence (category, releases holding the texture)")
    for k, v in sorted(by.items()):
        print(f"  {k[0]:12s} {k[1]:12s} {v}")
    # only-in-one where the pack exists in more than one release
    print("\n## single-release textures in packs that exist in 2+ releases")
    one = C()
    for r in recs:
        rels = [x for x in S.RELEASES if x in r]
        if len(rels) == 1 and len(r["present"]) > 1:
            one[(r["cat"], rels[0])] += 1
    for k, v in sorted(one.items()):
        print(f"  {k}: {v}")
    print("\n## formats in full triples (gc, pc, ps2)")
    trip = [r for r in recs if all(x in r for x in S.RELEASES)]
    print(f"  triples: {len(trip)}, same size: {sum(r['same_size'] for r in trip)}")
    fm = C((r["gc"]["fmt"], r["pc"]["fmt"], r["ps2"]["fmt"]) for r in trip)
    for k, v in fm.most_common():
        print(f"  GC {k[0]:6s} PC {k[1]:5s} PS2 {k[2]:8s} {v}")
    print("\n## size differences (all paired groups)")
    sd = C()
    for r in recs:
        rels = [x for x in S.RELEASES if x in r]
        if len(rels) > 1 and not r["same_size"]:
            sizes = {x: r[x]["w"] * r[x]["h"] for x in rels}
            big = max(sizes.values())
            winners = "+".join(x for x in rels if sizes[x] == big)
            sd[(r["cat"], winners)] += 1
    for k, v in sorted(sd.items()):
        print(f"  {k[0]:12s} largest: {k[1]:10s} {v}")
    print("\n## mip levels (triples, same size): level-count pattern gc/pc/ps2")
    lv = C()
    for r in trip:
        if r["same_size"]:
            lv["equal" if r["gc"]["levels"] == r["pc"]["levels"] == r["ps2"]["levels"] else
               f"gc<{'pc' if r['gc']['levels'] < r['pc']['levels'] else '='}" if r["pc"]["levels"] == r["ps2"]["levels"] else "other"] += 1
    for k, v in lv.most_common():
        print(f"  {k}: {v}")
    print("\n## errors against a lossless copy (RGB RMSE, PC space; GC-RGBA8 refs: GC space)")
    E = collections.defaultdict(list)
    for r in recs:
        if "err" in r:
            for x, (e, ea) in r["err"].items():
                E[(r["ref"], r["cat"], x, r[x]["fmt"])].append((e, ea))
    for k in sorted(E):
        v = np.array(E[k])
        print(f"  ref {k[0]:3s} {k[1]:12s} {k[2]:3s} {k[3]:6s} n={len(v):5d} RGB median {np.median(v[:, 0]):6.2f} "
              f"mean {v[:, 0].mean():6.2f} p90 {np.percentile(v[:, 0], 90):6.2f}  alpha mean {v[:, 1].mean():5.2f}")
    print("\n## head-to-head against the lossless copy (both lossy copies measured on the same texture)")
    h2h = C()
    for r in recs:
        if "err" in r and len(r["err"]) == 2:
            (a, (ea, _)), (b, (eb, _)) = sorted(r["err"].items())
            w = a if ea < eb - 0.5 else b if eb < ea - 0.5 else "tie"
            h2h[(r["ref"], r["cat"], f"{a}:{r[a]['fmt']} vs {b}:{r[b]['fmt']}", w)] += 1
    for k, v in sorted(h2h.items()):
        print(f"  ref {k[0]} {k[1]:12s} {k[2]:28s} better: {k[3]:4s} {v}")
    print("\n## all-lossy triples: RMSE to the per-texel median of the three")
    cons = collections.defaultdict(lambda: collections.defaultdict(list))
    best = C()
    for r in recs:
        if "cons" in r:
            key = (r["cat"], f"{r['gc']['fmt']}/{r['pc']['fmt']}/{r['ps2']['fmt']}")
            for x, e in r["cons"].items():
                cons[key][x].append(e)
            b = min(r["cons"], key=r["cons"].get)
            best[(r["cat"], b)] += 1
    for k in sorted(cons):
        line = "  ".join(f"{x} median {np.median(v):5.2f} mean {np.mean(v):5.2f}" for x, v in cons[k].items())
        print(f"  {k[0]:12s} {k[1]:20s} n={len(next(iter(cons[k].values()))):5d}  {line}")
    for k, v in sorted(best.items()):
        print(f"  closest to the median: {k}: {v}")
    print("\n## detail: in-block gradient energy relative to the copies' median (same-size groups)")
    G = collections.defaultdict(list)
    for r in recs:
        if "grad" in r and len(r["grad"]) >= 2:
            vals = r["grad"]
            med = float(np.median(list(vals.values())))
            if med < 0.5:
                continue
            for x, v in vals.items():
                G[(r["cat"], x, r[x]["fmt"])].append(v / med)
    for k in sorted(G):
        v = np.array(G[k])
        print(f"  {k[0]:12s} {k[1]:3s} {k[2]:8s} n={len(v):5d} median {np.median(v):.3f} mean {v.mean():.3f}")
    print("\n## unique colours (top level), same-size triples")
    U = collections.defaultdict(list)
    for r in trip:
        if r["same_size"]:
            for x in S.RELEASES:
                U[(r["cat"], x, r[x]["fmt"])].append(r[x]["colours"])
    for k in sorted(U):
        v = np.array(U[k])
        print(f"  {k[0]:12s} {k[1]:3s} {k[2]:8s} n={len(v):5d} median {np.median(v):7.0f} mean {v.mean():8.0f}")
    print("\n## PSMT8 banding: PS2 palette textures whose PC/GC copy has more than 256 colours")
    band = C()
    for r in trip:
        if r["same_size"] and r["ps2"]["fmt"] == "PSMT8":
            band[(r["cat"], "pc>256" if r["pc"]["colours"] > 256 else "pc<=256")] += 1
    for k, v in sorted(band.items()):
        print(f"  {k}: {v}")
    for space in ("pair_rmse", "pair_rmse_gc", "pair_alpha"):
        print(f"\n## three-cornered hat ({ {'pair_rmse': 'RGB, PC', 'pair_rmse_gc': 'RGB, GameCube squared', 'pair_alpha': 'alpha'}[space]} space): "
              "each copy's own error, sqrt of mean s^2; triples with every pair < 30 RMSE")
        H = collections.defaultdict(list)
        for r in trip:
            p = r.get(space, {})
            if len(p) != 3:
                continue
            a = np.array([p["gc-pc"], p["gc-ps2"], p["pc-ps2"]]) ** 2
            if (a >= 900).any():
                continue
            s2 = hat(a)
            fm = f"{r['gc']['fmt']}/{r['pc']['fmt']}/{r['ps2']['fmt']}"
            sub = "brief" if r["archive"] == "hw_brief" else r["cat"]
            H[(sub, fm)].append(s2)
            H[("all", fm)].append(s2)
        for k in sorted(H):
            v = np.array(H[k])
            m = np.sqrt(np.maximum(v.mean(0), 0))
            srt = np.sort(v, 1)
            win = C(np.where(srt[:, 1] - srt[:, 0] >= 2.0, np.array(S.RELEASES)[np.argmin(v, 1)], "tie"))
            print(f"  {k[0]:12s} {k[1]:20s} n={len(v):5d} sigma gc {m[0]:5.2f} pc {m[1]:5.2f} ps2 {m[2]:5.2f}"
                  f"  lowest per texture (by s^2 >= 2): " + ", ".join(f"{x} {win[x]}" for x in S.RELEASES + ("tie",)))
    for look, space in (("pc", "pair_rmse"), ("gc", "pair_rmse_gc")):
        print(f"\n## the rule, {look.upper()} colour look: chosen source per category (all groups),"
              " and on triples its agreement with the per-texture hat")
        ch = C((r["cat"], rule(r, look)) for r in recs)
        for cat in sorted({c for c, _ in ch}):
            print(f"  {cat:12s} " + "  ".join(f"{x} {ch[(cat, x)]:5d}" for x in S.RELEASES))
        agree = C()
        for r in trip:
            p = r.get(space, {})
            if len(p) != 3:
                continue
            a = np.array([p["gc-pc"], p["gc-ps2"], p["pc-ps2"]]) ** 2
            if (a >= 900).any():
                continue
            s2 = hat(a)
            pick = S.RELEASES.index(rule(r, look))
            agree["rule's pick within s^2 2 of the lowest" if s2[pick] - s2.min() < 2 else "hat prefers another"] += 1
            agree["sum s^2: rule"] += s2[pick]
            agree["sum s^2: per-texture lowest"] += s2.min()
            agree["sum s^2: all pc" if look == "pc" else "sum s^2: all gc"] += s2[1 if look == "pc" else 0]
            agree["n"] += 1
        n = agree.pop("n")
        for k, v in agree.items():
            print(f"  {k}: " + (f"{v}" if not k.startswith("sum") else f"RMSE {np.sqrt(max(v, 0) / n):.2f}"))
    print("\n## pairing quality (thumbnail distance)")
    for k in ("d_pc", "d_ps2", "d_ps2pc"):
        v = np.array([r[k] for r in recs if k in r])
        if v.size:
            print(f"  {k}: n={len(v)} median {np.median(v):.2f} p90 {np.percentile(v, 90):.2f} >=12: {(v >= 12).sum()}")


def main():
    root = S.survey_root(sys.argv[1:])
    if "--summary" in sys.argv:
        summary(root)
        return 0
    n = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 8
    js = jobs(root)
    with Pool(n) as p, open(root / "tex_survey.jsonl", "w") as f:
        for recs in p.imap_unordered(survey_pack, js, chunksize=4):
            for r in recs:
                f.write(json.dumps(r) + "\n")
    print(f"{len(js)} packs surveyed -> {root / 'tex_survey.jsonl'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
