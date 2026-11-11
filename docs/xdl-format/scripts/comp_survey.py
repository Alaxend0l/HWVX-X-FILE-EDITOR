#!/usr/bin/env python3
"""Which object subgraphs repeat across packs: data for the `.xdl` components (../components.md).

    python3 comp_survey.py SURVEY_ROOT [--rel pc|gc] [--only ARCH,ARCH]
        writes SURVEY_ROOT/components/<rel>_instances.jsonl and <rel>_variants.pkl
    python3 comp_survey.py SURVEY_ROOT --summary [--rel pc]      prints the tables of components.md

SURVEY_ROOT holds `hwvx-x extract` trees (survey_common.py). Run with plain `python3` (not -I).

Per pack:
1. Types: the Port_Swap walkers as ported in docs/beta-cars/research/scripts (walk.py: cars,
   effects, sounds, models; walk_im.py: interface), plus the roots of world, item, audio and
   share packs, the pack's TAC triggers and shaders (ported here from Port_Swap.c).
2. Nodes: the blob cut at every pointer target (pointers.bin + local ref relocs); a node runs to
   the next target, or further when a walker knows its struct is larger.
3. Fingerprints (Merkle):
   * deep  = md5 of a node's bytes with every pointer slot replaced by its target's deep
     fingerprint, texture slots by the texture's content hash (header + top level pixels),
     sound slots by the sample's content hash (rate, loop, PCM), imports from the share packs
     (HW_share #1, HW_Cars #2, HW_Items #3) resolved to the same content hashes, other imports by
     (pack id, index). Position in the blob doesn't count.
   * shape = the same with every scalar word (and texture/sound slot) zeroed: equal shapes are the
     same structure, near-duplicates when the deep fingerprints differ.
   Cycles (back edges) hash as a fixed token; their count is reported.
4. Per instance of a component kind (a walker label, see KINDS): deep, shape, closure bytes
   (geometry and texture pixels excluded: they are leaves, counted on their own), and for one
   instance per variant the flattened field list (node label + offset + value) used to count the
   fields two near-duplicates differ in.
"""

import bisect
import collections
import hashlib
import json
import pickle
import struct
import sys
import threading
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parents[1] / "beta-cars" / "research" / "scripts"))

import survey_common as S  # noqa: E402
import walk as W  # noqa: E402
import walk_im  # noqa: E402

NONE = W.NONE

# walker label -> (component group, kind name). Only these become instances.
KINDS = {
    # effects
    "EventSeq": ("effects", "EventSeq (effect script)"),
    "ParticleEmitter": ("effects", "particle emitter"),
    "Particle": ("effects", "particle description"),
    "Projectile": ("weapons", "projectile"),
    "Blast": ("weapons", "blast"),
    "Tracer": ("effects", "tracer / trail"),
    "AttachData": ("effects", "attached model (corona, flare)"),
    "EmitterData": ("effects", "FX emitter (EmitterData)"),
    "LineDef": ("effects", "line effect"),
    "RadarBlip": ("interface", "radar blip"),
    "LightDef": ("world", "light description"),
    # sound
    "SoundSeq": ("sound", "sound sequence"),
    "Sound": ("sound", "sound (descriptor)"),
    "CarASoundSet*[]": ("sound", "car sound set"),
    # car parts
    "CarAEngine": ("car", "engine + cameras"),
    "CameraSubMode[]": ("car", "camera list"),
    "CarAHud": ("interface", "car HUD"),
    "CarAStuntInfo": ("car", "stunt setup"),
    "CarAAccel": ("car", "acceleration tuning"),
    "CarACollider": ("car", "collider"),
    "CarAForcePoints": ("car", "force points"),
    "CarTrailBone[]": ("car", "wheels"),
    "CarAGear[]": ("car", "gears"),
    "CarAStartSlave[]": ("car", "start slaves"),
    "CarASlaveSet[](slaves)": ("car", "car slave sets (FX)"),
    "CarASlaveSet[](weapons)": ("weapons", "car weapon slave sets"),
    "CarASlaveSet[](pickup)": ("pickups", "car pickup slave sets"),
    "SlaveTrigger[]": ("effects", "slave list"),
    "AICarScript": ("ai", "AI car script"),
    "AIStateInfo": ("ai", "AI state"),
    "CarAModelRef": ("car", "model ref (model + anims)"),
    "MotionAnim": ("models", "motion animation"),
    "KeyAnim": ("models", "key animation"),
    "Skeleton": ("models", "skeleton"),
    "CarAInfo": ("car", "whole vehicle (CarAInfo)"),
    # world
    "ThingRecord": ("world", "thing record (prop)"),
    "DestructibleInfo": ("world", "destructible info"),
    "TileCar": ("ai", "world AI car slot"),
    "CreatureInfo": ("ai", "creature info"),
    "WorldGameData": ("world", "world game data"),
    "TilePath": ("world", "tile path"),
    "Trigger": ("triggers", "TAC trigger"),
    "Shader": ("materials", "shader (animated texture)"),
    "ItemInfo": ("pickups", "item record"),
    "WeaponInfo": ("weapons", "weapon info"),
    # interface
    "IMHeader": ("interface", "interface script (IMHeader)"),
    "IMObject[]": ("interface", "interface object list"),
    "Font": ("interface", "font"),
    "StateAnim": ("interface", "state animation"),
    "ObjAnim": ("interface", "object animation"),
    "EventDef": ("interface", "interface event"),
    # leaves
    "Geometry": ("models", "geometry"),
}


# ------------------------------------------------------------------------------------------
# extra walkers (Port_Swap.c roots the research walkers don't have)

def tac_trigger(s, t, depth):
    s.lab(t, 0x2C, "Trigger")
    s.bytes_(t + 0x28, 4)
    if s.word(t) == 7:
        p = s.target(t + 4)
        if p != NONE:
            walk_im.im_header(s, s.target(p))
    for lo, no in ((0x14, 0x1C), (0x18, 0x20)):
        lst = s.target(t + lo); n = min(s.word(t + no), 0x100)
        if lst == NONE:
            continue
        s.lab(lst, n * 0x18, "Action[](TAC)")
        for i in range(n):
            a = lst + i * 0x18; data = s.target(a + 4); ty = s.word(a)
            if data == NONE:
                continue
            if ty == 4:
                walk_im.im_header(s, s.target(data))
            elif ty == 31:
                walk_im.im_header(s, s.target(data + 4))
    conds = s.target(t + 0xC); n = s.word(t + 0x10)
    if depth < 2 and conds != NONE:
        for i in range(min(n, 0x40)):
            tac_trigger(s, conds + i * 0x2C, depth + 1)


def shaders(s, table, count):
    for i in range(min(count, 0x1000)):
        d = s.target(table + i * 0xC)
        if d == NONE or s.claim[d] == W.BYTES:
            continue
        s.lab(d, 0x18, "Shader")
        s.bytes_(d, 4)
        W.track(s, d + 4, W.KEYS_COLOR)
        W.key_track(s, d + 0xC, 0x10, 0xC)


def light_def(s, d):
    if d == NONE or s.claim[d] == W.BYTES:
        return
    s.lab(d, 0x1C, "LightDef")
    s.bytes_(d, 4)
    W.track(s, d + 0xC, W.KEYS_F32); W.track(s, d + 0x14, W.KEYS_COLOR)


def item_info(s, off):
    s.lab(off, 0x3C, "ItemInfo")
    s.bytes_(off, 2); s.u16(off + 2)
    sub = s.target(off + 0x30)
    if sub != NONE:
        s.lab(sub, 0x38, "WeaponInfo")
        card = s.target(sub)
        if card != NONE and W.is_motion_anim(s, card):
            W.motion_anim(s, card)
        s.bytes_(sub + 0x2C, 2); s.u16(sub + 0x2E); s.bytes_(sub + 0x34, 4)
    W.slave_list(s, off + 0x38)


def world(s):
    off = 0
    s.lab(0, 0xA0, "World")
    lst = s.target(0xC); n = s.word(0x8)
    for i in range(min(n, 0x100) if lst != NONE else 0):
        e = lst + i * 0x60
        s.lab(e, 0x60, "TileCar")
        s.bytes_(e + 0x34, 4); s.bytes_(e + 0x58, 8)
        for k in range(4):
            W.ai_script(s, s.target(e + 0x40 + k * 4))
    lst = s.target(0x24); n = s.word(0x20)
    for i in range(min(n, 0x1000) if lst != NONE else 0):
        q = lst + i * 0x50
        il = s.target(q + 0x30)
        if il == NONE:
            continue
        s.lab(il, 0x1C, "QuadContents")
        statics, dyn = s.target(il + 8), s.target(il + 0x10)
        for j in range(min(s.word(il + 4), 0x1000) if statics != NONE else 0):
            W.thing_record(s, s.target(statics + j * 0x40 + 0x24))
        for j in range(min(s.word(il + 0xC), 0x1000) if dyn != NONE else 0):
            W.emitter(s, s.target(dyn + j * 0x40 + 0x24))
    lst = s.target(0x14); n = s.word(0x10)
    for i in range(min(n, 0x400) if lst != NONE else 0):
        s.lab(lst + i * 0x20, 0x20, "TilePath")
        W.thing_record(s, s.target(lst + i * 0x20 + 0xC))
    lst = s.target(0x3C); n = s.word(0x38)
    for i in range(min(n, 0x400) if lst != NONE else 0):
        light_def(s, s.target(lst + i * 0x20 + 0x10))
    lst = s.target(0x44); n = s.word(0x40)
    for i in range(min(n, 0x400) if lst != NONE else 0):
        c = s.target(lst + i * 0x40 + 0x18)
        if c != NONE:
            s.lab(c, 0x74, "CreatureInfo")
    gd = s.target(0x9C)
    if gd != NONE:
        s.lab(gd, 0x110, "WorldGameData")
        s.bytes_(gd, 4)
        walk_im.im_header(s, s.target(gd + 0xCC))
        W.camera_list(s, gd + 0x100); W.camera_list(s, gd + 0x108)


def label_pack(folder, e, ptype):
    s = W.Pack(str(folder), e)
    s.geom_roots = set(); s.strs = set(); s.unwalked = []
    for po in s.pointers:
        m = s.target(po)
        if W.is_motion_anim(s, m):
            W.motion_anim(s, m)
    for po in s.pointers:
        t = s.target(po)
        if W.is_event_seq(s, t):
            W.event_seq(s, t)
    for po in s.pointers:
        a = s.target(po)
        if W.is_key_anim(s, a):
            W.key_anim(s, a)
    h = s.h
    if ptype == "world":
        world(s)
    elif ptype in ("car", "item", "interface", "audio"):
        tab = s.target(4); n = s.word(0)
        s.lab(0, 8, "RootHeader")
        if tab != NONE:
            s.lab(tab, n * 8, "RootTable")
            for i in range(min(n, 0x100)):
                o = s.target(tab + 8 * i)
                if o == NONE:
                    continue
                if ptype == "car":
                    W.car_a_info(s, o)
                elif ptype == "item":
                    item_info(s, o)
                elif ptype == "interface":
                    walk_im.im_header(s, o)
                elif ptype == "audio":
                    W.sound_seq(s, o)
    # TileCar 0x24 -> CarAInfo (world AI vehicles)
    if ptype == "world":
        lst = s.target(0xC); n = s.word(0x8)
        for i in range(min(n, 0x100) if lst != NONE else 0):
            ci = s.target(lst + i * 0x60 + 0x24)
            if ci != NONE and s.inb(ci, 0x120) and ci not in s.objs:
                W.car_a_info(s, ci)
    if h.get("num_triggers"):
        for i in range(h["num_triggers"]):
            t = h["trigger_offset"] + i * 0x2C
            if s.inb(t, 0x2C):
                tac_trigger(s, t, 0)
    if h.get("num_shaders"):
        shaders(s, h["shader_offset"], h["num_shaders"])
    # local ref relocs are geometry (share pack exports, ThingRecord models ...)
    for o, v in s.ref:
        if (v >> 16) == 0:
            s.geom_roots.add(s.target(o))
    s.geom_roots.discard(NONE)
    for g in s.geom_roots:
        if g not in s.objs:
            s.lab(g, 4, "Geometry")
        else:
            s.objs[g] = "Geometry"
    return s


# ------------------------------------------------------------------------------------------
# content hashes of textures and sounds

def md5(*parts):
    m = hashlib.md5()
    for p in parts:
        m.update(p if isinstance(p, bytes) else str(p).encode())
    return m.hexdigest()[:16]


TEX_BPB = {4: 8, 5: 16}


def texture_table(h, d, e):
    """[(hash, header range, pixel range)] for the pack's textures (PC/GC headers)."""
    out = []
    n = h.get("num_textures", 0)
    if not n:
        return out
    o = h["texture_data_offset"]
    for i in range(n):
        if e == "<":
            fmt, w, ht, mips, ptr = struct.unpack_from("<4HI", d, o + 0x10 * i)
            ho = o + 0x10 * i
        else:  # GameCube: 0x10 header (u16s little-endian in the extract, pointer big-endian), 32-aligned
            fmt, w, ht, mips = struct.unpack_from("<4H", d, o)   # u16s little-endian as extracted
            ptr = struct.unpack_from(">I", d, o + 8)[0]
            ho = o; o = (o + 0x10 + 0x1F) & ~0x1F
        if e == "<":
            size = (max(1, (w + 3) // 4) * max(1, (ht + 3) // 4) * TEX_BPB[fmt]) if fmt in TEX_BPB else w * ht * (3 if fmt == 0 else 4)
        else:
            size = ((w + 7) // 8) * ((ht + 7) // 8) * 32 if fmt == 4 else w * ht * 4
        out.append((md5(b"T", fmt, w, ht, d[ptr:ptr + size]), ho, ptr))
    return out


def sound_table(folder, h, e):
    f = Path(folder) / "sound_data.bin"
    if not h.get("num_sounds") or not f.exists():
        return []
    sd = f.read_bytes()
    out = []
    for i in range(h["num_sounds"]):
        o = h["sfx_offset"] + 0x20 * i
        ptr, ln, loop, rate = struct.unpack_from(e + "4I", sd, o)
        pcm = sd[ptr:ptr + ln]
        if e == ">":   # host order, so PC and GC samples hash alike
            n = len(pcm) // 2
            pcm = struct.pack("<%dh" % n, *struct.unpack(">%dh" % n, pcm[:2 * n]))
        out.append(md5(b"S", rate, loop, pcm))
    return out


SHARE_IDS = {("hw_share", "share"): 1, ("hw_cars", "share"): 2, ("hw_items", "share"): 3}


# ------------------------------------------------------------------------------------------
# fingerprints

class Graph:
    def __init__(s, w, texs, snds, shared):
        s.w = w
        b = w.blob
        s.size = len(b)
        tg = set(W.pointer_targets(w))
        tg.add(0)
        s.tex_pix = {}
        for hsh, ho, ptr in texs:
            s.tex_pix[ptr] = hsh
        s.tex_hdr = {ho for _, ho, _ in texs}
        if texs:
            tg |= set(s.tex_pix)
        s.targets = sorted(t for t in tg if t < s.size)
        labsize = {}
        for name, off, n in w.order:
            labsize[off] = max(labsize.get(off, 0), n)
        s.labsize = labsize
        s.slots = {}
        for o in w.pointers:
            s.slots[o] = ("p", w.word(o))
        for o, v in w.ref:
            pk, idx = v >> 16, v & 0xFFFF
            if pk == 0:
                s.slots[o] = ("p", w.word(o))
            else:
                s.slots[o] = ("r", shared.get(("r", pk, idx), f"ref{pk}:{idx}"))
        for o, v in w.tex:
            pk, idx = v >> 16, v & 0xFFFF
            if o in s.slots and s.slots[o][0] == "p":
                continue
            tok = (texs[idx][0] if idx < len(texs) else f"tex?{idx}") if pk == 0 else \
                shared.get(("t", pk, idx), f"tex{pk}:{idx}")
            s.slots[o] = ("t", tok)
        for o, v in w.snd:
            pk, idx = v >> 16, v & 0xFFFF
            if o in s.slots:
                continue
            tok = (snds[idx] if idx < len(snds) else f"snd?{idx}") if pk == 0 else \
                shared.get(("s", pk, idx), f"snd{pk}:{idx}")
            s.slots[o] = ("s", tok)
        s.slot_offs = sorted(s.slots)
        s.geom = set(w.geom_roots)
        s.deep, s.shape = {}, {}
        s.cycles = 0

    def extent(s, t):
        i = bisect.bisect_right(s.targets, t)
        end = s.targets[i] if i < len(s.targets) else s.size
        return max(end - t, min(s.labsize.get(t, 0), s.size - t))

    def node_slots(s, t, n):
        i = bisect.bisect_left(s.slot_offs, t)
        j = bisect.bisect_left(s.slot_offs, t + n)
        return s.slot_offs[i:j]

    def _fp(s, t, n, which, stack):
        if t in s.tex_pix:
            return "tex:" + s.tex_pix[t]
        b = s.w.blob
        m = hashlib.md5()
        m.update(b"%d:" % n)
        pos = t
        for o in s.node_slots(t, n):
            if o < pos:
                continue
            if which == "deep":
                m.update(b[pos:o])
            else:
                m.update(b"%d;" % (o - t))
            kind, v = s.slots[o]
            if kind == "p" and which == "shape" and v in s.geom:
                m.update(b"PG")      # a model is one field for near-duplicates
            elif kind == "p":
                m.update(b"P" + s.fp(v, which, stack).encode() if v < s.size else b"P-")
            elif which == "deep":
                m.update(kind.encode() + str(v).encode())
            else:
                m.update(kind.encode())
            pos = o + 4
        if which == "deep":
            m.update(b[pos:t + n])
        return m.hexdigest()[:16]

    def fp(s, t, which="deep", stack=None):
        memo = s.deep if which == "deep" else s.shape
        if t in memo:
            return memo[t]
        if stack is None:
            stack = set()
        if t in stack:
            s.cycles += 1
            return "cyc"
        stack.add(t)
        r = s._fp(t, s.extent(t), which, stack)
        stack.discard(t)
        memo[t] = r
        return r

    def span_fp(s, off, n, which):
        """fingerprint of an object that may not start at a pointer target"""
        if off in s.targets_set:
            return s.fp(off, which)
        return s._fp(off, n, which, set())

    def closure(s, off, n):
        """(object bytes, geometry roots, texture hashes) reachable from [off, off+n)"""
        seen = set(); geoms = set(); stack = [(off, n)]; total = 0
        first = True
        s.last_nodes = []
        while stack:
            t, ln = stack.pop()
            if not first:
                if t in seen or t >= s.size:
                    continue
                if t in s.tex_pix:
                    continue
                if t in s.geom:
                    geoms.add(t); continue
                ln = s.extent(t)
            first = False
            seen.add(t)
            total += ln
            s.last_nodes.append((t, ln))
            for o in s.node_slots(t, ln):
                k, v = s.slots[o]
                if k == "p":
                    stack.append((v, 0))
        return total, geoms

    def flatten(s, off, n, limit=200000):
        """[(label, rel offset, value)] in DFS order; geometry and textures as tokens"""
        out = []; seen = set(); b = s.w.blob
        stack = [(off, n, True)]
        while stack and len(out) < limit:
            t, ln, first = stack.pop()
            if not first:
                if t in seen or t >= s.size:
                    continue
                if t in s.tex_pix:
                    out.append(("tex", 0, s.tex_pix[t])); continue
                if t in s.geom:
                    out.append(("geom", 0, s.fp(t))); continue
                ln = s.extent(t)
            seen.add(t)
            lab = s.w.objs.get(t, "?")
            sl = s.node_slots(t, ln); sls = set(sl)
            kids = []
            for o in range(t, t + ln - 3, 4):
                if o in sls:
                    k, v = s.slots[o]
                    if k == "p":
                        out.append((lab, o - t, "P")); kids.append((v, 0, False))
                    else:
                        out.append((lab, o - t, k + ":" + str(v)))
                else:
                    out.append((lab, o - t, b[o:o + 4]))
            stack.extend(reversed(kids))
        return out


def shared_tokens(root, rel, e):
    """content tokens of the share packs' textures, sounds and geometry exports"""
    tok = {}
    for a, folder in S.archives(root / rel).items():
        for name, f, ent in S.packs(folder):
            pk = SHARE_IDS.get((a, ent["type"]))
            if not pk:
                continue
            h = json.loads((f / "pack.json").read_text())["header"]
            d = (f / "data.bin").read_bytes()
            for i, (hsh, _, _) in enumerate(texture_table(h, d, e)):
                tok[("t", pk, i)] = hsh
            for i, hsh in enumerate(sound_table(f, h, e)):
                tok[("s", pk, i)] = hsh
    return tok


def pack_type(ent):
    t = ent["type"]
    return {"world_texture": "world_texture"}.get(t, t)


def cstr(b, off):
    if off >= len(b):
        return ""
    end = b.find(b"\0", off)
    return b[off:end].decode("latin-1")


def survey_pack(rel, e, a, name, f, ent, shared, pkl, agg, cov):
    ptype = pack_type(ent)
    if ptype in ("geometry", "collision") or not (f / "pointers.bin").exists():
        return []
    w = label_pack(f, e, ptype)
    h = w.h
    texs = texture_table(h, w.blob, e)
    snds = sound_table(f, h, e)
    g = Graph(w, texs, snds, shared)
    g.targets_set = set(g.targets)
    pk = SHARE_IDS.get((a, ent["type"]))
    if pk:
        for o, v in w.ref:
            if (v >> 16) == 0:
                shared[("r", pk, o // 4)] = "geom:" + g.fp(w.target(o))
    # family: the world a world pack builds on (its quad pack archive name); else the pack
    fam = f"{a}/{name}"
    if ptype == "world":
        fam = "world:" + cstr(w.blob, w.target(0x8C)).lower()
    insts = []
    pk_nodes = collections.defaultdict(dict)     # label -> {node offset: (deep, length)}
    for off, lab in w.objs.items():
        if lab not in KINDS:
            continue
        n = g.extent(off) if off in g.targets_set else w_labsize(g, off)
        deep = g.span_fp(off, n, "deep")
        shape = g.span_fp(off, n, "shape")
        if lab == "Geometry":
            byts, geoms = g.closure(off, n)[0], set()
            g.geom.discard(off); byts, _ = g.closure(off, n); g.geom.add(off)
        else:
            byts, geoms = g.closure(off, n)
        for t, ln in g.last_nodes:
            pk_nodes[lab][t] = (g.span_fp(t, ln, "deep") if t == off else g.fp(t), ln)
        insts.append({"rel": rel, "arch": a, "pack": name, "ptype": ptype, "off": off, "label": lab,
                      "fam": fam, "deep": deep, "shape": shape, "bytes": byts, "ngeom": len(geoms)})
        if deep not in pkl and lab != "Geometry":
            pkl[deep] = g.flatten(off, n)
    # maximal instances: not inside the closure of another instance of the same kind
    by = collections.defaultdict(list)
    for x in insts:
        by[x["label"]].append(x)
    for lab, xs in by.items():
        offs = {x["off"] for x in xs}
        inner = set()
        if len(xs) > 1 and lab != "Geometry":
            for x in xs:
                seen = set(); st = [x["off"]]; first = True
                while st:
                    t = st.pop()
                    if t in seen:
                        continue
                    seen.add(t)
                    if not first and t in offs:
                        inner.add(t)
                    if not first and (t in g.geom or t in g.tex_pix):
                        continue
                    ln = g.extent(t) if (t in g.targets_set or not first) else w_labsize(g, t)
                    first = False
                    for o in g.node_slots(t, ln):
                        k, v = g.slots[o]
                        if k == "p" and v < g.size:
                            st.append(v)
        for x in xs:
            x["top"] = x["off"] not in inner
    for x in insts:
        x["cycles"] = g.cycles
    cat = S.category(a)
    for lab, nodes in pk_nodes.items():
        for key in ((lab, cat), (lab, "all")):
            A = agg.setdefault(key, {"total": 0, "deep": {}, "fam": set()})
            for deep, ln in nodes.values():
                A["total"] += ln
                A["deep"][deep] = ln
                A["fam"].add((fam, deep, ln))
    # coverage of the blob: textures, geometry, objects under a kind, other (first claim wins)
    m = bytearray(g.size)          # 0 other, 1 texture, 2 geometry, 3 object
    for hsh, ho, ptr in texs:
        m[ho:ho + 0x10] = b"\1" * len(m[ho:ho + 0x10])
    for t in g.tex_pix:
        e_ = t + g.extent(t); m[t:e_] = b"\1" * (e_ - t)

    def paint(t, ln, v):
        m[t:t + ln] = bytes(m[t:t + ln]).translate(bytes([v]) + bytes(range(1, 256)))
    for t in list(g.geom):
        g.geom.discard(t); g.closure(t, g.extent(t)); g.geom.add(t)
        for u, ln in g.last_nodes:
            paint(u, ln, 2)
    for lab, nodes in pk_nodes.items():
        for t, (_, ln) in nodes.items():
            paint(t, ln, 3)
    c = cov.setdefault(cat, collections.Counter())
    c["packs"] += 1; c["blob"] += g.size
    for k, v in (("tex", 1), ("geom", 2), ("obj", 3)):
        c[k] += m.count(v)
    return insts


def w_labsize(g, off):
    return max(4, min(g.labsize.get(off, 4), g.size - off))


def run(root, rel, only):
    e = ">" if rel == "gc" else "<"
    out_dir = root / "components"
    out_dir.mkdir(exist_ok=True)
    shared = shared_tokens(root, rel, e)
    pkl = {}; agg = {}; cov = {}
    arcs = S.archives(root / rel)
    # share packs first so their geometry exports resolve
    order = sorted(arcs, key=lambda a: (a not in ("hw_share", "hw_cars", "hw_items"), a))
    n = 0
    with open(out_dir / f"{rel}_instances.jsonl", "w") as fo:
        for a in order:
            if only and a not in only:
                continue
            for name, f, ent in S.packs(arcs[a]):
                try:
                    insts = survey_pack(rel, e, a, name, f, ent, shared, pkl, agg, cov)
                except Exception as ex:  # noqa: BLE001
                    print(f"{a}/{name}: {type(ex).__name__} {ex}", file=sys.stderr)
                    continue
                for x in insts:
                    fo.write(json.dumps(x) + "\n")
                n += 1
            print(f"{a}: done", file=sys.stderr)
    with open(out_dir / f"{rel}_variants.pkl", "wb") as fo:
        pickle.dump(pkl, fo)
    # node-level bytes per (kind, category): as stored, deduplicated within a family, across all
    out = {}
    for (lab, cat), A in agg.items():
        fam_bytes = sum(ln for _, _, ln in A["fam"])
        out.setdefault(lab, {})[cat] = {"total": A["total"], "dedup_family": fam_bytes,
                                        "dedup_all": sum(A["deep"].values())}
    (out_dir / f"{rel}_bytes.json").write_text(json.dumps({"kinds": out, "coverage": cov}, indent=1))
    print(f"{n} packs", file=sys.stderr)


def main():
    root = S.survey_root(sys.argv[1:])
    rel = sys.argv[sys.argv.index("--rel") + 1] if "--rel" in sys.argv else "pc"
    only = set(sys.argv[sys.argv.index("--only") + 1].lower().split(",")) if "--only" in sys.argv else None
    if "--summary" in sys.argv:
        import comp_summary
        comp_summary.summary(root, rel)
        return 0
    sys.setrecursionlimit(1000000)
    threading.stack_size(512 * 1024 * 1024)
    t = threading.Thread(target=run, args=(root, rel, only))
    t.start(); t.join()
    return 0


if __name__ == "__main__":
    sys.exit(main())
