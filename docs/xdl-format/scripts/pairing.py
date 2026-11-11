#!/usr/bin/env python3
"""The cross-release pairing table: structural id -> per-release fingerprints (../pairing.md).

    python3 pairing.py SURVEY_ROOT [--out DIR] [--jobs N] [--fmv REL=DIR ...] [--only=ARCH,...]
    python3 pairing.py --summary [--out DIR]       coverage tables for ../pairing.md
    python3 pairing.py --verify  [--out DIR]       check the table holds only ids, hashes, sizes, counts

SURVEY_ROOT holds `hwvx-x extract` trees gc/, pc/, ps2/ and beta/ (the PS2 prototype, release
name `proto`), as survey_common.py describes. --out defaults to ../pairing. Movies are read from
the releases' own folders; the defaults are the repository layout (orig/gc/files/fmv_,
orig/ps2/XPS/ENGLISH/fmv_, orig/ps2-beta/XPS/ENGLISH, the HWVX-SDL import's pc/FMV_), each
overridable with --fmv gc=DIR etc. Needs numpy; run without -I so the user site-packages load.

Output: one JSON Lines file per archive, sorted, one asset per line; no game bytes (--verify).
"""

import bisect
import collections
import difflib
import hashlib
import json
import os
import re
import struct
import sys
from multiprocessing import Pool
from pathlib import Path

import numpy as np

import survey_common as S
import ps2_tex as T
import ps2_audio as A
import ps2_geom as P
import ps2_geom_check as PGC
import tex_survey as TS

RELS = ("gc", "pc", "ps2", "proto")
TREE = {"gc": "gc", "pc": "pc", "ps2": "ps2", "proto": "beta"}
FAMILY = {"gc": "gc", "pc": "pc", "ps2": "ps2", "proto": "ps2"}     # decoder per release

TEX_LIMIT = 24.0       # thumbnail distance under which two textures may pair (tex_survey.py)
TEX_MARGIN = 1.0       # an index pair must be within this of the best in its row and column
TEX_EQUIV = 1.0        # two candidates closer than this to each other are the same image
SND_MIN_CORR = 0.95    # lossy sounds pair at this correlation or more
SND_MARGIN = 0.02      # ... and a better one than any non-equivalent alternative by this
Q = 4096.0             # geometry quantisation: positions and uvs to 1/4096

REPO = Path(__file__).resolve().parents[3]
FMV_DEFAULT = {
    "gc": REPO / "orig/gc/files/fmv_",
    "pc": Path.home() / "Library/Application Support/HWVX-SDL/HotWheelsVelocityX/pc/FMV_",
    "ps2": REPO / "orig/ps2/XPS/ENGLISH/fmv_",
    "proto": REPO / "orig/ps2-beta/XPS/ENGLISH",
}


def H(*parts):
    """BLAKE2b-128, hex."""
    h = hashlib.blake2b(digest_size=16)
    for p in parts:
        h.update(p)
    return h.hexdigest()


# ---------------------------------------------------------------------------------------------
# pack index


def pack_key(e):
    """A pack's structural name: type_id, plus _qN for a quadrant geometry pack (sub_id low half;
    the high half is its group count, which changes when a world is edited)."""
    k = f"{e['type']}_{e['id']}"
    if e["sub_id"]:
        k += f"_q{e['sub_id'] & 0xFFFF}" if e["type"] == "geometry" else f"_s{e['sub_id']}"
    return k


def index(root):
    """{archive key: {"name": display name, "packs": {pack key: {rel: folder}}, "order": [...]}}."""
    out = {}
    for rel in RELS:
        base = root / TREE[rel]
        if not base.exists():
            continue
        for m in sorted(base.rglob("manifest.json")):
            man = json.loads(m.read_text())
            key = m.parent.name.lower()
            a = out.setdefault(key, {"name": None, "packs": {}, "order": []})
            if a["name"] is None:
                a["name"] = Path(man.get("source_file", m.parent.name)).stem
            for e in man["entries"]:
                k = pack_key(e)
                if k not in a["packs"]:
                    a["packs"][k] = {}
                    a["order"].append(k)
                a["packs"][k][rel] = m.parent / e["path"]
    return out


# ---------------------------------------------------------------------------------------------
# generic pairing helpers


def new_group(rel, item):
    return {"m": {rel: item}, "pair": {}, "flags": []}


def group_by_hub(groups):
    return {id(g["m"][next(iter(g["m"]))]): g for g in groups}


def assemble(lists, pair_fn):
    """Pair per-release item lists of one pack. GameCube is the hub; what it lacks is paired
    among the other releases in release order. pair_fn(a_items, b_items, rel_a, rel_b) ->
    (pairs [(ia, ib, method, distance)], ambiguous {('a'|'b', index): [candidate indices]})."""
    groups = []
    owner = {}           # (rel, list position) -> group
    rels = [r for r in RELS if lists.get(r)]
    for hub_i, hub in enumerate(rels):
        for k, it in enumerate(lists[hub]):
            if (hub, k) not in owner:
                g = new_group(hub, it)
                g["hub"] = hub
                groups.append(g)
                owner[(hub, k)] = g
        for r in rels[hub_i + 1:]:
            ka = [k for k in range(len(lists[hub])) if owner[(hub, k)]["hub"] == hub and r not in owner[(hub, k)]["m"]]
            kb = [k for k in range(len(lists[r])) if (r, k) not in owner]
            if not ka or not kb:
                continue
            pairs, amb = pair_fn([lists[hub][k] for k in ka], [lists[r][k] for k in kb], hub, r)
            for ia, ib, meth, d in pairs:
                g = owner[(hub, ka[ia])]
                g["m"][r] = lists[r][kb[ib]]
                g["pair"][r] = (meth, d)
                owner[(r, kb[ib])] = g
            for (side, i), cands in amb.items():
                if side == "a":
                    owner[(hub, ka[i])]["flags"].append(
                        f"ambiguous:{r}:" + ",".join(str(lists[r][kb[c]]["i"]) for c in cands))
                else:
                    # the other release's item: give it its own group now, flagged
                    if (r, kb[i]) not in owner:
                        g = new_group(r, lists[r][kb[i]])
                        g["hub"] = r
                        g["flags"].append(f"ambiguous:{hub}:" + ",".join(str(lists[hub][ka[c]]["i"]) for c in cands))
                        groups.append(g)
                        owner[(r, kb[i])] = g
    return groups


# ---------------------------------------------------------------------------------------------
# textures

def _ceil(a, b):
    return (a + b - 1) // b


def tex_level_bytes(fam, fmt, w, h):
    if fam == "gc":
        return _ceil(w, 8) * _ceil(h, 8) * 32 if fmt == 4 else _ceil(w, 4) * _ceil(h, 4) * 64
    if fam == "pc":
        if fmt in (4, 5):
            return max(1, _ceil(w, 4)) * max(1, _ceil(h, 4)) * (8 if fmt == 4 else 16)
        return w * h * (3 if fmt == 0 else 4)
    return w * h * (4 if fmt == T.FMT_PSMCT32 else 1)


def tex_exact(fam, h, d, t):
    """u16le fmt, w, h, levels as stored, then every level's pixels as stored (+ PS2 CLUT)."""
    lv = t.get("mips", t.get("levels", 0))
    n, w, ht = 0, t["w"], t["h"]
    for _ in range(max(1, lv)):
        n += tex_level_bytes(fam, t["fmt"], w, ht)
        w, ht = max(1, w // 2), max(1, ht // 2)
    px = d[t["ptr"]:t["ptr"] + n]
    parts = [struct.pack("<4H", t["fmt"], t["w"], t["h"], lv), px]
    if fam == "ps2" and t["fmt"] == T.FMT_PSMT8:
        o = h["palette_offset"] + 0x400 * t["clut"]
        parts.append(d[o:o + 0x400])
    return H(*parts), sum(len(p) for p in parts)


def thumb16(img):
    """16x16 box thumbnail of premultiplied RGB (PC colour space), float32 [768]."""
    f = img.astype(np.float32)
    rgb = f[..., :3] * (f[..., 3:] / 255.0)
    h, w = rgb.shape[:2]
    ry, rx = _ceil(16, h) if h < 16 else 1, _ceil(16, w) if w < 16 else 1
    if ry > 1 or rx > 1:
        rgb = np.repeat(np.repeat(rgb, ry, 0), rx, 1)
        h, w = rgb.shape[:2]
    ys, xs = (np.arange(16) * h) // 16, (np.arange(16) * w) // 16
    s = np.add.reduceat(np.add.reduceat(rgb, ys, 0), xs, 1)
    cy = np.diff(np.append(ys, h))[:, None, None]
    cx = np.diff(np.append(xs, w))[None, :, None]
    return (s / (cy * cx)).ravel()


def load_textures(rel, folder):
    fam = FAMILY[rel]
    try:
        h, d = T.load(folder)
    except FileNotFoundError:
        return []
    if not h.get("num_textures"):
        return []
    hdrs = {"gc": T.gc_headers, "pc": T.pc_headers, "ps2": T.ps2_headers}[fam](h, d)
    dec = {t["i"]: t for t in TS.load_release(fam, folder)}
    out = []
    for t in hdrs:
        x, n = tex_exact(fam, h, d, t)
        it = {"i": t["i"], "x": x, "s": n, "w": t["w"], "h": t["h"]}
        if t["i"] in dec:
            it["thumb"] = thumb16(dec[t["i"]]["img"])
        else:
            it["undecoded"] = True
        out.append(it)
    return out


def pair_textures(a, b, ra, rb):
    """Index pairs where both tables agree, then content pairs; ambiguous ones left unpaired."""
    retail = "proto" not in (ra, rb)
    ok_a = [k for k, t in enumerate(a) if "thumb" in t]
    ok_b = [k for k, t in enumerate(b) if "thumb" in t]
    if not ok_a or not ok_b:
        return [], {}
    ta = np.stack([a[k]["thumb"] for k in ok_a])
    tb = np.stack([b[k]["thumb"] for k in ok_b])
    dist = np.abs(ta[:, None, :] - tb[None, :, :]).mean(-1)
    if retail:
        sa = np.array([(a[k]["w"], a[k]["h"]) for k in ok_a])
        sb = np.array([(b[k]["w"], b[k]["h"]) for k in ok_b])
        dist = np.where((sa[:, None, :] == sb[None, :, :]).all(-1), dist, np.inf)
    bb = np.abs(tb[:, None, :] - tb[None, :, :]).mean(-1)      # b against b: equivalent images
    aa = np.abs(ta[:, None, :] - ta[None, :, :]).mean(-1)
    pairs, amb = [], {}
    ua, ub = set(), set()
    rowmin, colmin = dist.min(1), dist.min(0)
    pos_b = {b[k]["i"]: j for j, k in enumerate(ok_b)}
    for i, k in enumerate(ok_a):
        j = pos_b.get(a[k]["i"])
        if j is None:
            continue
        d = dist[i, j]
        if d < TEX_LIMIT and d <= rowmin[i] + TEX_MARGIN and d <= colmin[j] + TEX_MARGIN:
            pairs.append((k, ok_b[j], "index", float(d)))
            ua.add(i)
            ub.add(j)
    order = np.argsort(dist, axis=None, kind="stable")
    for flat in order:
        i, j = divmod(int(flat), len(ok_b))
        d = dist[i, j]
        if not d < TEX_LIMIT:
            break
        if i in ua or j in ub:
            continue
        m = max(TEX_MARGIN, 0.25 * d)
        alt_b = [j2 for j2 in range(len(ok_b)) if j2 != j and j2 not in ub and dist[i, j2] < d + m and bb[j, j2] >= TEX_EQUIV]
        alt_a = [i2 for i2 in range(len(ok_a)) if i2 != i and i2 not in ua and dist[i2, j] < d + m and aa[i, i2] >= TEX_EQUIV]
        ua.add(i)
        ub.add(j)
        if alt_b or alt_a:
            amb[("a", ok_a[i])] = [ok_b[x] for x in [j] + alt_b]
            amb[("b", ok_b[j])] = [ok_a[x] for x in [i] + alt_a]
            continue
        pairs.append((ok_a[i], ok_b[j], "content", float(d)))
    return pairs, amb


# ---------------------------------------------------------------------------------------------
# sounds and instrument banks


def pcm_of(fam, sd, ptr, n):
    """(stored bytes, PCM16 little-endian bytes or None, float samples)."""
    raw = sd[ptr:ptr + n]
    if fam == "gc":
        s = np.frombuffer(raw[:len(raw) // 2 * 2], ">i2")
    elif fam == "pc":
        s = np.frombuffer(raw[:len(raw) // 2 * 2], "<i2")
    else:
        v = A.vag_decode(sd, ptr, n)[0][28:]          # block 0 is a silent lead-in (SFX and banks)
        return raw, None, np.array(v, np.float64)
    return raw, s.astype("<i2").tobytes(), s.astype(np.float64)


def sound_item(fam, sd, i, ptr, n, rate):
    raw, pcm, f = pcm_of(fam, sd, ptr, n)
    it = {"i": i, "x": H(raw), "s": len(raw), "c": len(f), "rate": rate, "pcm": f}
    if pcm is not None:
        it["n"] = H(pcm)
    return it


def load_sounds(rel, folder):
    fam = FAMILY[rel]
    if not (Path(folder) / "pack.json").exists():
        return [], []
    h, sd = A.load(folder)
    snd, bank = [], []
    try:
        for s in A.sfx(h, sd, be=(fam == "gc")):
            if s["ptr"] + s["len"] <= len(sd):
                snd.append(sound_item(fam, sd, s["i"], s["ptr"], s["len"], s["rate"]))
    except struct.error:
        pass
    bo = h.get("song_bank_offset", 0)
    if bo and sd[bo:bo + 4] in (b"RIFF", b"RIFX") and sd[bo + 8:bo + 12] == b"sndb":
        try:
            b = A.bank(sd, bo)
            for e in b["entries"]:
                bank.append(sound_item(fam, sd, e["i"], b["data"] + e["off"], e["len"], e["rate"]))
        except (ValueError, struct.error):
            pass
    return snd, bank


_corr_cache = {}


def corr(x, y):
    """Best correlation of two sounds (y resampled to x's rate), lags -8..8."""
    key = (x["x"], y["x"])
    if key in _corr_cache:
        return _corr_cache[key]
    a, b = x["pcm"], y["pcm"]
    best = -1.0
    if len(a) >= 32 and len(b) >= 32 and x["rate"] and y["rate"]:
        if y["rate"] != x["rate"]:
            b = np.interp(np.arange(int(len(b) * x["rate"] / y["rate"])) * y["rate"] / x["rate"], np.arange(len(b)), b)
        for lag in range(-8, 9):
            xs = a[max(0, lag):len(a) + min(0, lag)]
            ys = b[max(0, -lag):len(b) + min(0, -lag)]
            m = min(len(xs), len(ys))
            if m >= 32:
                xs, ys = xs[:m], ys[:m]
                sx, sy = xs.std(), ys.std()
                if sx and sy:
                    best = max(best, float(((xs - xs.mean()) * (ys - ys.mean())).mean() / (sx * sy)))
                elif not sx and not sy and xs.mean() == ys.mean():
                    best = max(best, 1.0)       # both silent / constant
    _corr_cache[key] = best
    return best


def pair_sounds(a, b, ra, rb):
    pairs, amb = [], {}
    ua, ub = set(), set()
    lossless = "n" in (a[0] if a else {}) and "n" in (b[0] if b else {})
    if lossless:                                        # GameCube / PC: equal samples
        for i in range(min(len(a), len(b))):
            if a[i]["n"] == b[i]["n"]:
                pairs.append((i, i, "index", 0.0))
                ua.add(i)
                ub.add(i)
        byn = collections.defaultdict(list)
        for j, t in enumerate(b):
            if j not in ub:
                byn[t["n"]].append(j)
        for i, t in enumerate(a):
            if i not in ua and byn.get(t["n"]):
                j = byn[t["n"]].pop(0)
                pairs.append((i, j, "content", 0.0))
                ua.add(i)
                ub.add(j)
        return pairs, amb
    for i in range(min(len(a), len(b))):
        c = corr(a[i], b[i])
        if c >= SND_MIN_CORR:
            pairs.append((i, i, "index", round(1 - c, 4)))
            ua.add(i)
            ub.add(i)
    for i, t in enumerate(a):
        if i in ua:
            continue
        dur = t["c"] / t["rate"] if t["rate"] else 0
        cands = []
        for j, u in enumerate(b):
            if j in ub or not u["rate"]:
                continue
            du = u["c"] / u["rate"]
            if dur and abs(du / dur - 1) < 0.15:
                cands.append((corr(t, u), j))
        cands.sort(reverse=True)
        if not cands or cands[0][0] < SND_MIN_CORR:
            continue
        c, j = cands[0]
        alts = [j2 for c2, j2 in cands[1:] if c2 > c - SND_MARGIN and b[j2]["x"] != b[j]["x"]]
        if alts:
            amb[("a", i)] = [j] + alts
            ua.add(i)
            continue
        pairs.append((i, j, "content", round(1 - c, 4)))
        ua.add(i)
        ub.add(j)
    return pairs, amb


def pair_banks(a, b, ra, rb):
    """Instrument banks are one table in every release (same count, same instruments): entries
    pair by index; the distance says how close a lossy copy is (short looped waveforms resample
    badly, so it is a measure, not a test)."""
    if len(a) != len(b):
        return pair_sounds(a, b, ra, rb)
    if "n" in a[0] and "n" in b[0]:
        return [(i, i, "index", 0.0 if a[i]["n"] == b[i]["n"] else 1.0) for i in range(len(a))], {}
    return [(i, i, "index", round(max(0.0, 1 - corr(a[i], b[i])), 4)) for i in range(len(a))], {}


# ---------------------------------------------------------------------------------------------
# songs


def load_song(rel, folder):
    if not (Path(folder) / "pack.json").exists():
        return []
    h, sd = A.load(folder)
    o = h.get("song_offset", 0)
    tag = sd[o:o + 4]
    if tag not in (b"RIFX", b"RIFF") or sd[o + 8:o + 12] != b"MIDI":
        if h.get("song_offset") or tag == b"IMPM":
            # the prototype's Impulse Tracker modules: another format, not paired
            if tag == b"IMPM":
                return [{"i": 0, "x": H(sd[o:]), "s": len(sd) - o, "fmt": "IMPM"}]
        return []
    size = struct.unpack_from(">I" if tag == b"RIFX" else "<I", sd, o + 4)[0] + 8
    raw = sd[o:o + size]
    x = H(raw)
    return [{"i": 0, "x": x, "n": x, "s": len(raw)}]


def pair_songs(a, b, ra, rb):
    if a and b and a[0].get("fmt") == b[0].get("fmt"):
        return [(0, 0, "index", 0.0 if a[0]["x"] == b[0]["x"] else 1.0)], {}
    return [], {}


# ---------------------------------------------------------------------------------------------
# geometry


class Slots:
    def __init__(self, slots):
        self.s = sorted(slots)

    def blk(self, d, o, n, extra=()):
        """d[o:o+n] with every pointer slot (and the given extra ranges) zeroed."""
        b = bytearray(d[o:o + n])
        lo = bisect.bisect_left(self.s, o)
        hi = bisect.bisect_left(self.s, o + n)
        for s in self.s[lo:hi]:
            b[s - o:s - o + 4] = b"\0\0\0\0"
        for a, z in extra:
            b[a:a + z] = bytes(len(b[a:a + z]))
        return bytes(b)


def q(v):
    return tuple(int(round(c * Q)) + 0 for c in v)


def canon_tri(t):
    """A triangle of (qpos, quv) vertices rotated to start at its least vertex (winding kept)."""
    k = t.index(min(t))
    return t[k:] + t[:k]


def norm_geometry(levels):
    """levels: [(kind, [group tris or None for shadow groups])], tris [((pos, uv), x3)]."""
    h = hashlib.blake2b(digest_size=16)
    tset = collections.Counter()
    ntri = 0
    h.update(struct.pack("<I", len(levels)))
    for kind, groups in levels:
        h.update(kind.encode() + struct.pack("<I", len(groups)))
        if kind != "d":
            continue
        for tris in groups:
            if tris is None:
                h.update(b"S")
                continue
            ct = sorted(canon_tri(tuple((q(p), q(u)) for p, u in t)) for t in tris)
            h.update(struct.pack("<I", len(ct)))
            for t in ct:
                h.update(struct.pack("<15q", *(c for pos, uv in t for c in pos + uv)))
                tset[frozenset(pos for pos, _ in t)] += 1
            ntri += len(ct)
    return h.hexdigest(), tset, ntri


def gc_geometry_list(folder):
    b = PGC.GC(folder)
    sl = Slots(b.slots)
    out = []
    for gi, off in enumerate(PGC.gc_geometries(b)):
        nlod, skinned, lods, dist = b.u16(off + 2), b.u16(off + 4) != 0, b.target(off + 8), b.target(off + 0xC)
        parts = [sl.blk(b.d, off, 0x40)]
        if dist is not None:
            parts.append(sl.blk(b.d, dist, 4 * nlod))
        levels = []
        bad = False
        for i in range(nlod):
            L = lods + 0x18 * i
            parts.append(sl.blk(b.d, L, 0x18))
            ng, groups, vb = b.u16(L + 2), b.target(L + 8), b.target(L + 0xC)
            if groups is None:
                levels.append(("x", []))
                continue
            shadow = bool(b.u16(groups + 2) & 0x4000)
            gl = []
            for k in range(ng):
                G = groups + 0x20 * k
                parts.append(sl.blk(b.d, G, 0x20, ((0x18, 4),)))
                if shadow:
                    gl.append(None)
                    continue
                dl, size = b.target(G + 0x1C), b.u32(G + 0x10)
                if dl is not None:
                    parts.append(sl.blk(b.d, dl, size))
                try:
                    gl.append([tuple((v[0], v[1]) for v in tri) for tri in PGC.gc_group_tris(b, L, k, skinned)])
                except (struct.error, TypeError, IndexError):
                    bad = True
                    gl.append([])
            if vb is not None and not shadow:
                parts.append(sl.blk(b.d, vb, 0x10 + b.u32(vb + 12)))
            levels.append(("s" if shadow else "d", gl))
        out.append(geom_item(gi, parts, levels, b.d[off + 0x10:off + 0x40], ">", bad))
    return out


def geom_item(gi, parts, levels, bbox_raw, endian, bad):
    n, tset, ntri = norm_geometry(levels)
    bbox = struct.pack("<12I", *struct.unpack(endian + "12I", bbox_raw)).hex()
    it = {"i": gi, "x": H(*parts), "s": sum(len(p) for p in parts), "n": n, "c": ntri, "bbox": bbox, "tset": tset}
    if bad:
        it["bad"] = True
    return it


def pc_geometry_list(folder):
    d = (folder / "data.bin").read_bytes()
    raw = (folder / "pointers.bin").read_bytes() if (folder / "pointers.bin").exists() else b""
    slots = set(struct.unpack("<%dI" % (len(raw) // 4), raw))
    sl = Slots(slots)
    u16 = lambda o: struct.unpack_from("<H", d, o)[0] if o + 2 <= len(d) else 0  # noqa: E731
    u32 = lambda o: struct.unpack_from("<I", d, o)[0] if o + 4 <= len(d) else 0  # noqa: E731

    def target(o):
        if o not in slots:
            return None
        t = u32(o)
        return t if t < len(d) else None
    out = []
    gi = 0
    for off in range(0, len(d) - 0x40, 4):
        if u16(off) != 0x0002 or (off + 8) not in slots or (off + 0xC) not in slots:
            continue
        nlod, lods, dist = u16(off + 2), target(off + 8), target(off + 0xC)
        if not 1 <= nlod <= 8 or lods != off + 0x40:
            continue
        parts = [sl.blk(d, off, 0x40)]
        if dist is not None:
            parts.append(sl.blk(d, dist, 4 * nlod))
        levels, bad = [], False
        for i in range(nlod):
            L = lods + 0x18 * i
            parts.append(sl.blk(d, L, 0x18))
            ng, groups, vb = u16(L + 2), target(L + 8), target(L + 0xC)
            if groups is None:
                levels.append(("x", []))
                continue
            shadow = bool(u16(groups + 2) & 0x4000)
            gl, lists = [], []
            for k in range(ng):
                G = groups + 0x20 * k
                parts.append(sl.blk(d, G, 0x20, ((0x18, 4),)))
                if shadow:
                    gl.append(None)
                    continue
                cnt, il = u32(G + 0x10), target(G + 0x1C)
                if il is None or il + 6 * cnt > len(d):
                    bad = True
                    lists.append([])
                    continue
                parts.append(sl.blk(d, il, 6 * cnt))
                lists.append(struct.unpack_from("<%dH" % (3 * cnt), d, il))
            if not shadow and vb is not None:
                pp, cp, up, npp = (target(vb + 4 * k) for k in range(4))
                nv = max((max(x) + 1 for x in lists if x), default=0)
                parts.append(sl.blk(d, vb, 0x20))
                for p_, st in ((pp, 16), (cp, 4), (up, 8), (npp, 16)):
                    if p_ is not None:
                        parts.append(sl.blk(d, p_, st * nv))
                try:
                    pos = [struct.unpack_from("<3f", d, pp + 16 * j) for j in range(nv)]
                    uv = [struct.unpack_from("<2f", d, up + 8 * j) for j in range(nv)]
                    for x in lists:
                        gl.append([tuple((pos[x[3 * t + e]], uv[x[3 * t + e]]) for e in range(3)) for t in range(len(x) // 3)])
                except (struct.error, TypeError):
                    bad = True
            levels.append(("s" if shadow else "d", gl))
        out.append(geom_item(gi, parts, levels, d[off + 0x10:off + 0x40], "<", bad))
        gi += 1
    return out


def dma_extent(pk, dma):
    tag = pk.u32(dma)
    return 16 + (tag & 0xFFFF) * 16


def ps2_geometry_list(folder):
    pk = P.Pack(folder)
    sl = Slots(pk.slots)
    out = []
    for gi, off in enumerate(P.find_geometries(pk)):
        nlod, lods, dist = pk.u16(off + 2), pk.target(off + 8), pk.target(off + 0xC)
        parts = [sl.blk(pk.d, off, 0x40)]
        if dist is not None:
            parts.append(sl.blk(pk.d, dist, 4 * nlod))
        for i in range(nlod):
            L = lods + 0x18 * i
            parts.append(sl.blk(pk.d, L, 0x18))
            kind, ng, groups = pk.u16(L), pk.u16(L + 2), pk.target(L + 8)
            if groups is None:
                continue
            for k in range(ng):
                G = groups + 0x20 * k
                parts.append(sl.blk(pk.d, G, 0x20, ((0x18, 4),)))
                pkt = pk.target(G + 0xC)
                if kind == P.LEVEL_SHADOW or pk.u16(G + 2) & 0x4000 or pkt is None:
                    continue
                if kind == P.LEVEL_SKINNED:
                    r = pkt + 0x10
                    recs = []
                    while r + 8 <= len(pk.d) and pk.u32(r + 4):
                        sub = pk.target(r + 4)
                        recs.append(sub)
                        r += 8
                    parts.append(sl.blk(pk.d, pkt, r + 8 - pkt))
                    for sub in recs:
                        if sub is not None:
                            parts.append(sl.blk(pk.d, sub, dma_extent(pk, sub)))
                else:
                    parts.append(sl.blk(pk.d, pkt, 0x10 + dma_extent(pk, pkt + 0x10)))
        bad = False
        levels = []
        try:
            geo = P.read_geometry(pk, off, fix_winding=True)
            for lvl in geo["levels"]:
                if lvl["kind"] == P.LEVEL_SHADOW:
                    levels.append(("s", [None] * len(lvl["groups"])))
                    continue
                gl = []
                for g in lvl["groups"]:
                    if g["flags"] & 0x4000:
                        gl.append(None)
                    else:
                        gl.append([tuple((v[0], v[1]) for v in t[:3]) for t in g["tris"]])
                levels.append(("d", gl))
        except Exception:  # noqa: BLE001  (prototype packets the decoder doesn't know)
            bad = True
            levels = []
        out.append(geom_item(gi, parts, levels, pk.d[off + 0x10:off + 0x40], "<", bad))
    return out


def load_geometry(rel, folder):
    if not (Path(folder) / "data.bin").exists():
        return []
    fam = FAMILY[rel]
    return {"gc": gc_geometry_list, "pc": pc_geometry_list, "ps2": ps2_geometry_list}[fam](Path(folder))


def geom_distance(a, b):
    if a["n"] == b["n"]:
        return 0.0
    ta, tb = a["tset"], b["tset"]
    m = max(sum(ta.values()), sum(tb.values()))
    if not m:
        return 0.0 if a["x"] == b["x"] else 1.0
    return round(1 - sum((ta & tb).values()) / m, 4)


def pair_geometry(a, b, ra, rb):
    """By bounding box in order (bit-equal across releases unless edited), equal-length unmatched
    runs by order (geom_survey.align)."""
    sm = difflib.SequenceMatcher(None, [g["bbox"] for g in a], [g["bbox"] for g in b], autojunk=False)
    pairs = []
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag == "equal":
            for i, j in zip(range(i1, i2), range(j1, j2)):
                pairs.append((i, j, "index" if a[i]["i"] == b[j]["i"] else "content", geom_distance(a[i], b[j])))
        elif tag == "replace" and i2 - i1 == j2 - j1:
            for i, j in zip(range(i1, i2), range(j1, j2)):
                d = geom_distance(a[i], b[j])
                if d < 0.5 or "proto" not in (ra, rb):     # retail tables agree: keep the order
                    pairs.append((i, j, "order", d))
    # what is left (the prototype's bounding boxes differ): by triangles, greedy, unambiguous only
    ua, ub = {p[0] for p in pairs}, {p[1] for p in pairs}
    la = [i for i in range(len(a)) if i not in ua and a[i]["c"]]
    lb = [j for j in range(len(b)) if j not in ub and b[j]["c"]]
    cand = []
    for i in la:
        for j in lb:
            ca, cb = a[i]["c"], b[j]["c"]
            if abs(ca - cb) <= 0.1 * max(ca, cb):
                d = geom_distance(a[i], b[j])
                if d <= 0.1:
                    cand.append((d, abs(i - j), i, j))
    cand.sort()
    amb = {}
    for d, _, i, j in cand:
        if i in ua or j in ub:
            continue
        alt = [j2 for d2, _, i2, j2 in cand if i2 == i and j2 != j and j2 not in ub and d2 <= d + 0.02
               and b[j2]["n"] != b[j]["n"]]
        alt_a = [i2 for d2, _, i2, j2 in cand if j2 == j and i2 != i and i2 not in ua and d2 <= d + 0.02
                 and a[i2]["n"] != a[i]["n"]]
        ua.add(i)
        ub.add(j)
        if alt or alt_a:
            amb[("a", i)] = [j] + alt
            amb[("b", j)] = [i] + alt_a
            continue
        pairs.append((i, j, "content", d))
    return pairs, amb


# ---------------------------------------------------------------------------------------------
# per pack

KINDS = ("texture", "sound", "bank", "song", "geometry")
LOAD = {"texture": load_textures, "song": load_song, "geometry": load_geometry}
PAIR = {"texture": pair_textures, "sound": pair_sounds, "bank": pair_banks, "song": pair_songs,
        "geometry": pair_geometry}
KEEP = {"texture": ("i", "x", "s", "w", "h"), "sound": ("i", "x", "n", "s", "c"), "bank": ("i", "x", "n", "s", "c"),
        "song": ("i", "x", "n", "s"), "geometry": ("i", "x", "n", "c")}


def records(arch, pack, kind, groups):
    out = []
    for g in groups:
        hub = g["hub"]
        idx = g["m"]["gc"]["i"] if "gc" in g["m"] else f"{hub}:{g['m'][hub]['i']}"
        rec = {"id": f"base://{arch}/{pack}/{kind}/{idx}"}
        first = None
        for r in RELS:
            if r not in g["m"]:
                continue
            it = g["m"][r]
            e = {k: it[k] for k in KEEP[kind] if k in it}
            if e.get("x") == e.get("n"):
                del e["x"]                       # x omitted: the stored bytes are the normal form
            if first is None:
                first = e
            elif "w" in e and (e["w"], e["h"]) == (first.get("w"), first.get("h")):
                del e["w"], e["h"]               # same size as the first release's copy
            if r in g["pair"]:
                m, d = g["pair"][r][0], round(g["pair"][r][1], 2 if kind == "texture" else 3)
                if m != "index":
                    e["m"] = m                   # m omitted: paired by index
                if d:
                    e["d"] = d                   # d omitted: distance 0
            if it.get("undecoded") or it.get("bad"):
                e["bad"] = 1
            rec[r] = e
        rels = [r for r in RELS if r in g["m"]]
        flags = list(g["flags"])
        if len(rels) == 1:
            flags.insert(0, "only:" + rels[0])
        if flags:
            rec["flags"] = flags
        out.append(rec)
    return out


def do_pack(job):
    arch, pack, folders = job
    out = []
    snd = {r: load_sounds(r, f) for r, f in folders.items()}
    lists = {
        "texture": {r: load_textures(r, f) for r, f in folders.items()},
        "sound": {r: s for r, (s, _) in snd.items()},
        "bank": {r: b for r, (_, b) in snd.items()},
        "song": {r: load_song(r, f) for r, f in folders.items()},
        "geometry": {r: load_geometry(r, f) for r, f in folders.items()},
    }
    for kind in KINDS:
        L = {r: v for r, v in lists[kind].items() if v}
        if L:
            out += records(arch, pack, kind, assemble(L, PAIR[kind]))
    _corr_cache.clear()
    return arch, pack, out


# ---------------------------------------------------------------------------------------------
# movies


def movies(fmv):
    found = collections.defaultdict(dict)
    for r, d in fmv.items():
        d = Path(d)
        if not d.is_dir():
            continue
        for f in sorted(d.iterdir()):
            if f.is_file() and (r != "proto" or f.suffix.lower() == ".pss"):
                found[f.stem.lower()][r] = f
    out = []
    for name in sorted(found):
        rels = [r for r in RELS if r in found[name]]
        rec = {"id": f"base://fmv_/{name}/movie/0"}
        for r in rels:
            h = hashlib.blake2b(digest_size=16)
            with open(found[name][r], "rb") as fh:
                for chunk in iter(lambda: fh.read(1 << 20), b""):
                    h.update(chunk)
            e = {"i": 0, "x": h.hexdigest(), "s": found[name][r].stat().st_size}
            if r != rels[0]:
                e["m"] = "name"
            rec[r] = e
        if len(rels) == 1:
            rec["flags"] = ["only:" + rels[0]]
        out.append(rec)
    return out


# ---------------------------------------------------------------------------------------------
# summary and verification


def load_out(out):
    for f in sorted(Path(out).glob("*.jsonl")):
        for line in f.read_text().splitlines():
            yield f.stem, json.loads(line)


def summary(out):
    """Coverage per kind and release, as the tables of ../pairing.md."""
    C = collections.Counter
    st = collections.defaultdict(C)
    meth = collections.defaultdict(C)
    packs = {}
    for arch, r in load_out(out):
        if "packs" in r:
            packs[arch] = r["packs"]
            continue
        k = r["id"].split("/")[4]
        pk = r["id"].split("/")[3]
        rels = [x for x in RELS if x in r]
        amb = any(f.startswith("ambiguous") for f in r.get("flags", []))
        for x in rels:
            s = st[(k, x)]
            s["present"] += 1
            if len(rels) > 1:
                s["paired"] += 1
                if any(y != x and y != "proto" for y in rels):
                    s["paired retail"] += 1
            elif amb:
                s["ambiguous"] += 1
            else:
                s["single"] += 1
                if sum(1 for y in RELS if y in packs.get(arch, {}).get(pk, [])) == 1:
                    s["single, pack only here"] += 1
            if amb and len(rels) > 1:
                s["paired, ambiguous to another"] += 1
            if r[x].get("bad"):
                s["undecoded"] += 1
            if x != rels[0]:
                meth[(k, x)][r[x].get("m", "index")] += 1
                if not r[x].get("d") and r[x].get("m") != "name":
                    meth[(k, x)]["d=0"] += 1
        if "gc" in r:
            for y in ("pc", "ps2"):
                if y in r and "n" in r["gc"] and r["gc"]["n"] == r[y].get("n"):
                    st[(k, y)]["n equal to gc"] += 1
    cols = ["present", "paired", "paired retail", "single", "single, pack only here", "ambiguous",
            "paired, ambiguous to another", "n equal to gc", "undecoded"]
    print("| kind | release | " + " | ".join(cols) + " | method (vs first release) |")
    print("|---|---|" + "---|" * len(cols) + "---|")
    for (k, x) in sorted(st, key=lambda t: (KINDS + ("movie",)).index(t[0]) * 10 + RELS.index(t[1])):
        s = st[(k, x)]
        m = ", ".join(f"{a} {b:,}" for a, b in sorted(meth[(k, x)].items()))
        print(f"| {k} | {x} | " + " | ".join(f"{s[c]:,}" if s[c] else "" for c in cols) + f" | {m} |")


HEX = re.compile(r"^[0-9a-f]{32}$")
ID = re.compile(r"^base://[A-Za-z0-9_]+/[a-z0-9_]+/(texture|sound|bank|song|geometry|movie)/((gc|pc|ps2|proto):)?\d+$")
FLAG = re.compile(r"^(only:(gc|pc|ps2|proto)|ambiguous:(gc|pc|ps2|proto):[0-9,]+)$")
INT_KEYS = {"i", "s", "c", "w", "h", "bad"}


def verify(out):
    """Every value is an id, a release/kind/method name, a 32-hex hash, a number or a flag."""
    bad = 0
    n = 0
    for arch, r in load_out(out):
        n += 1
        if "packs" in r:
            ok = set(r) == {"archive", "packs"} and re.fullmatch(r"[A-Za-z0-9_]+", r["archive"]) and all(
                re.fullmatch(r"[a-z0-9_]+", p) and set(v) <= set(RELS) for p, v in r["packs"].items())
        else:
            ok = ID.match(r.get("id", ""))
            for key, v in r.items():
                if key == "id":
                    continue
                if key == "flags":
                    ok = ok and all(isinstance(f, str) and FLAG.match(f) for f in v)
                elif key in RELS:
                    for f, x in v.items():
                        if f in ("x", "n"):
                            ok = ok and isinstance(x, str) and bool(HEX.match(x))
                        elif f in INT_KEYS:
                            ok = ok and isinstance(x, int)
                        elif f == "m":
                            ok = ok and x in ("index", "content", "order", "name")
                        elif f == "d":
                            ok = ok and isinstance(x, (int, float))
                        else:
                            ok = False
                else:
                    ok = False
        if not ok:
            bad += 1
            if bad < 10:
                print("unexpected:", arch, json.dumps(r)[:200])
    print(f"{n} lines, {bad} unexpected")
    return 1 if bad else 0


# ---------------------------------------------------------------------------------------------


def main():
    argv = sys.argv[1:]
    out = Path(argv[argv.index("--out") + 1]) if "--out" in argv else Path(__file__).resolve().parents[1] / "pairing"
    if "--summary" in argv:
        summary(out)
        return 0
    if "--verify" in argv:
        return verify(out)
    skip = {argv.index(o) + 1 for o in ("--out", "--jobs") if o in argv}
    skip |= {i + 1 for i, a in enumerate(argv) if a == "--fmv"}
    root = Path(next(a for i, a in enumerate(argv) if not a.startswith("--") and i not in skip))
    njobs = int(argv[argv.index("--jobs") + 1]) if "--jobs" in argv else os.cpu_count()
    fmv = dict(FMV_DEFAULT)
    for i, a in enumerate(argv):
        if a == "--fmv":
            r, d = argv[i + 1].split("=", 1)
            fmv[r] = Path(d)
    only = [a for a in argv if a.startswith("--only=")]
    idx = index(root)
    js = []
    for key, a in idx.items():
        if only and key not in only[0][7:].split(","):
            continue
        for p in a["order"]:
            js.append((a["name"], p, a["packs"][p]))
    js.sort(key=lambda j: -sum(Path(f, "data.bin").stat().st_size if Path(f, "data.bin").exists() else 0
                               for f in j[2].values()))
    res = collections.defaultdict(list)
    with Pool(njobs) as pool:
        for k, (arch, pack, recs) in enumerate(pool.imap_unordered(do_pack, js)):
            res[arch] += recs
            if k % 50 == 0:
                print(f"{k}/{len(js)} packs", file=sys.stderr)
    out.mkdir(parents=True, exist_ok=True)
    order = {k: n for n, k in enumerate(KINDS + ("movie",))}


    def sort_key(r):
        p = r["id"].split("/")
        i = p[5]
        return (p[3], order[p[4]], ":" in i, i.split(":")[0], int(i.split(":")[-1]))
    for key, a in idx.items():
        if only and key not in only[0][7:].split(","):
            continue
        name = a["name"]
        packs = {p: [r for r in RELS if r in a["packs"][p]] for p in sorted(a["packs"])}
        lines = [json.dumps({"archive": name, "packs": packs}, separators=(",", ":"))]
        lines += [json.dumps(r, separators=(",", ":")) for r in sorted(res[name], key=sort_key)]
        (out / f"{name}.jsonl").write_text("\n".join(lines) + "\n")
    if not only:
        mv = movies(fmv)
        (out / "fmv_.jsonl").write_text("".join(json.dumps(r, separators=(",", ":")) + "\n" for r in mv))
    print(f"{len(js)} packs -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
