"""Python port of HWVX-SDL Port_Swap.c's claim walkers (car-pack subset + the
generic shape scans), endianness-agnostic, so the same walk runs on GC (>) and
PS2 beta (<) car packs.  Adds per-byte object-type labels.

Claim kinds:  0 DEFAULT (word: u32/f32)  1 PTR (pointer slot)  2 U16  3 BYTES
              4 U32 (explicit word)  5 TEXREF (texture-reloc slot)  6 SNDREF
              7 REFREL (ref_relocs slot)  8 GEOM (geometry, by closure)
              9 TEX (texture header/pixels/palettes)  10 STR (string)
"""
import struct, json, os, sys

DEFAULT, PTR, U16, BYTES, U32, TEXREF, SNDREF, REFREL, GEOM, TEX, STR = range(11)
KNAMES = ["word", "ptr", "u16", "u8", "u32", "texslot", "sndslot", "refslot", "geom", "tex", "str"]
NONE = 0xFFFFFFFF


class Pack:
    def __init__(s, d, e):
        s.d = d; s.e = e
        s.blob = open(d + "/data.bin", "rb").read()
        s.size = len(s.blob)
        s.json = json.load(open(d + "/pack.json"))
        s.h = s.json["header"]

        def tab(n, w):
            p = d + f"/{n}.bin"
            if not os.path.exists(p):
                return []
            b = open(p, "rb").read()
            if w == 4:
                return list(struct.unpack(e + f"{len(b)//4}I", b))
            return [struct.unpack_from(e + "2I", b, i) for i in range(0, len(b), 8)]
        s.pointers = tab("pointers", 4)
        s.tex = tab("texture_relocs", 8)
        s.snd = tab("sound_relocs", 8)
        s.ref = tab("ref_relocs", 8)
        s.shd = tab("shader_relocs", 8)
        s.claim = bytearray(s.size)
        s.label = [None] * s.size          # object type name per byte (only set by lab())
        s.objs = {}                        # off -> type name (object starts)
        s.order = []                       # (name, off, size) in walk order
        s.ptrset = set(s.pointers)
        s.refset = {o for o, v in s.ref}
        for o in s.pointers:
            s.claim[o:o + 4] = bytes([PTR]) * 4
        # ref_relocs slots: in the game loader they are pointers too (local: += base;
        # foreign: shared pack export).  Port_Swap marks them DONE as well.
        for o, v in s.ref:
            s.claim[o:o + 4] = bytes([PTR if (v >> 16) == 0 else REFREL]) * 4
        for o, v in s.tex:
            if s.claim[o] == DEFAULT:
                s.claim[o:o + 4] = bytes([TEXREF]) * 4
        for o, v in s.snd:
            if s.claim[o] == DEFAULT:
                s.claim[o:o + 4] = bytes([SNDREF]) * 4
        s.refmap = {o: v for o, v in s.ref}

    # --- primitive access -----------------------------------------------------
    def inb(s, off, n):
        return off != NONE and 0 <= off < s.size and n <= s.size - off

    def word(s, off):
        return struct.unpack_from(s.e + "I", s.blob, off)[0] if s.inb(off, 4) else 0

    def half(s, off):
        return struct.unpack_from(s.e + "H", s.blob, off)[0] if s.inb(off, 2) else 0

    def f32(s, off):
        return struct.unpack_from(s.e + "f", s.blob, off)[0] if s.inb(off, 4) else 0.0

    def target(s, off):
        if not s.inb(off, 4) or s.claim[off] != PTR:
            return NONE
        v = s.word(off)
        return v if v < s.size else NONE

    def cl(s, off, n, kind):
        if off == NONE or not s.inb(off, n):
            return
        for i in range(off, off + n):
            if s.claim[i] not in (PTR, REFREL):
                s.claim[i] = kind

    def u16(s, off): s.cl(off, 2, U16)
    def bytes_(s, off, n): s.cl(off, n, BYTES)

    def lab(s, off, n, name):
        if off == NONE or not s.inb(off, 1):
            return
        n = min(n, s.size - off)
        if off not in s.objs:
            s.objs[off] = name
            s.order.append((name, off, n))
        for i in range(off, off + n):
            if s.label[i] is None:
                s.label[i] = name


# ---------------------------------------------------------------------------
# walkers (port of Port_Swap.c), plus lab() calls with the struct extents
# ---------------------------------------------------------------------------

def key_track(s, track, stride, bytes_at, name="keys"):
    keys = s.target(track + 4); n = s.word(track)
    if keys == NONE:
        return
    n = min(n, 0x10000)
    s.lab(keys, n * stride, name)
    for i in range(n):
        k = keys + i * stride
        s.bytes_(k + bytes_at, 4)
        for w in range(0, stride, 4):
            if w != bytes_at and s.inb(k + w, 4) and s.claim[k + w] == DEFAULT:
                s.cl(k + w, 4, U32)


KEYS_ACTIVE, KEYS_VEC4, KEYS_COLOR, KEYS_UV, KEYS_F32, KEYS_POINT = range(6)


def track(s, t, kind):
    if kind == KEYS_ACTIVE: key_track(s, t, 8, 0, "keys.active")
    elif kind == KEYS_VEC4: key_track(s, t, 0x20, 0x10, "keys.vec4")
    elif kind == KEYS_UV: key_track(s, t, 0x28, 0x20, "keys.uv")
    elif kind == KEYS_F32: key_track(s, t, 0xC, 4, "keys.f32")
    elif kind == KEYS_POINT: key_track(s, t, 0x10, 0xC, "keys.point")
    elif kind == KEYS_COLOR:
        key_track(s, t, 0xC, 0, "keys.color"); key_track(s, t, 0xC, 4, "keys.color")


def sound(s, off):
    if off != NONE:
        s.lab(off, 0x1C, "Sound")
        s.bytes_(off, 8)


def sound_seq(s, off):
    if off == NONE:
        return
    s.lab(off, 0x10, "SoundSeq")
    s.bytes_(off + 4, 2); s.u16(off + 6)
    keys = s.target(off + 0xC); n = s.word(off + 8)
    if keys == NONE:
        return
    n = min(n, 0x1000)
    s.lab(keys, n * 0x10, "SoundSeq.keys")
    for i in range(n):
        k = keys + i * 0x10
        lst = s.target(k + 0xC); ns = min(s.word(k + 8), 0x100)
        s.bytes_(k + 4, 4)
        if lst != NONE:
            s.lab(lst, ns * 4, "SoundSeq.soundptrs")
            for j in range(ns):
                sound(s, s.target(lst + j * 4))


def camera_list(s, off):
    lst = s.target(off + 4); n = min(s.word(off), 0x40)
    if lst == NONE:
        return
    s.lab(lst, n * 0xC, "CameraSubMode[]")
    for i in range(n):
        e = lst + i * 0xC
        s.bytes_(e, 4)
        bp = s.target(e + 4)
        if bp != NONE:
            s.lab(bp, 0x18 if s.blob[e] == 4 else 0x10, f"CameraParams.t{s.blob[e]}")
        if s.blob[e] == 4 and bp != NONE:
            s.u16(bp + 0x14); s.bytes_(bp + 0x16, 2)


def thing_record(s, rec):
    if rec == NONE or s.claim[rec + 4] == BYTES:
        return
    s.lab(rec, 0x20, "ThingRecord")
    if s.blob[rec + 4] == 1:
        motion_anim(s, s.target(rec))
    else:
        s.geom_roots.add(s.target(rec))
    s.bytes_(rec + 4, 4)
    info = s.target(rec + 0x10)
    if info != NONE:
        s.lab(info, 0x14, "DestructibleInfo")
        s.bytes_(info + 0x10, 2)
    slave_list(s, rec + 0x18)


def ai_state(s, st, depth):
    if st == NONE or depth > 32:
        return
    info = s.target(st)
    if info != NONE:
        s.lab(info, 0x60, "AIStateInfo")
        s.bytes_(info, 8)
        lim = s.target(info + 0x58)
        if lim != NONE:
            s.lab(lim, 0x14, "AILimits")
            s.bytes_(lim + 0x10, 4)
    ch = s.target(st + 8); n = min(s.word(st + 4), 0x100)
    if ch != NONE:
        s.lab(ch, n * 0xC, "AIState[]")
        for i in range(n):
            ai_state(s, ch + i * 0xC, depth + 1)


def ai_script(s, sc):
    if sc == NONE:
        return
    s.lab(sc, 0x34, "AICarScript")
    s.bytes_(sc + 0x24, 4)
    ai_state(s, sc + 0x28, 0)


def line_def(s, d):
    if d == NONE or s.claim[d] == BYTES:
        return
    s.lab(d, 0x1C, "LineDef")
    s.bytes_(d, 4); track(s, d + 0xC, KEYS_F32); track(s, d + 0x14, KEYS_COLOR)


def emitter(s, off):
    if off == NONE or s.claim[off] == BYTES:
        return
    s.lab(off, 0x24, "EmitterData")
    s.bytes_(off, 4)
    sound_seq(s, s.target(off + 0x20))


_es_depth = [0]


def event_seq(s, script):
    if script == NONE or _es_depth[0] > 4:
        return
    _es_depth[0] += 1
    s.lab(script, 8, "EventSeq")
    entries = s.target(script + 4); n = s.word(script)
    if entries != NONE:
        s.lab(entries, min(n, 0x400) * 0xC, "EventSeq.entries")
    for i in range(min(n, 0x400) if entries != NONE else 0):
        e = entries + i * 0xC
        a = s.target(e + 4); b = s.target(e + 8); ty = s.word(e)
        if ty in (1, 2, 3, 4, 7, 29):
            if b != NONE:
                s.lab(b, 0x30, "ParticleEmitter")
            if b != NONE and s.claim[b + 0x20] != BYTES:
                adv = s.target(b + 0x28); info = s.target(b + 0x2C)
                s.bytes_(b + 0x20, 4)
                if s.blob[b + 0x21] == 2 and adv != NONE:
                    s.lab(adv, 0x10, "PEmitter.adv")
                    d = s.target(adv + 0xC)
                    s.bytes_(adv + 8, 4)
                    if d != NONE and s.blob[adv + 9] in (1, 2):
                        s.lab(d, 0x14, "PEmitter.adv.data")
                        s.bytes_(d + 0x10, 4)
                if info != NONE and s.blob[b + 0x22] == 4:
                    s.lab(info, 0x14, "PEmitter.info")
                    s.bytes_(info + 0x10, 4)
            if a != NONE:
                if ty == 3: event_seq(s, a)
                elif ty == 4: emitter(s, a)
                elif ty == 7: thing_record(s, a)
            if ty == 2 and a != NONE and s.claim[a] != BYTES:
                s.lab(a, 0xAC, "Projectile")
                s.geom_roots.add(s.target(a + 0x10))
                tgt = s.target(a + 0xA8)
                s.bytes_(a, 0xC)
                track(s, a + 0x8C, KEYS_VEC4); track(s, a + 0x94, KEYS_COLOR)
                slave_list(s, a + 0x9C)
                if tgt != NONE:
                    s.u16(tgt); s.u16(tgt + 2)
            if ty == 1 and a != NONE:
                s.lab(a, 0x24, "Particle")
                anim = s.target(a + 4); kind = s.blob[a]
                s.bytes_(a, 4)
                if kind <= 2:
                    track(s, a + 0x14, KEYS_COLOR)
                if kind == 2:
                    fx = s.target(a + 0x20)
                    line_def(s, anim)
                    if fx != NONE:
                        for k in range(min(s.word(a + 0x1C), 0x40)):
                            line_def(s, s.target(fx + k * 4))
                elif anim != NONE:
                    s.lab(anim, 0x30, "Particle.anim")
                    s.bytes_(anim, 4)
                    if kind <= 1:
                        track(s, anim + 0x28, KEYS_VEC4 if kind == 0 else KEYS_F32)
        elif ty in (5, 6, 8, 10, 25, 26):
            if b != NONE:
                s.lab(b, 0x20, "PosEmitter")
                s.bytes_(b + 0x18, 4)
            if a != NONE:
                if ty == 5:
                    s.lab(a, 0x34, "Blast")
                    s.bytes_(a, 8)
                    dm = s.target(a + 0x1C)
                    if dm != NONE:
                        s.lab(dm, 4, "Blast.damage")
                        s.bytes_(dm, 4)
                    c = s.target(a + 0x20)
                    if c != NONE:
                        s.lab(c, 0xC, "FlashDef")
                        s.bytes_(c, 4); track(s, c + 4, KEYS_COLOR)
                    track(s, a + 0x2C, KEYS_POINT)
                elif ty == 6: thing_record(s, a)
                elif ty == 10:
                    s.lab(a, 4, "EventSeq.t10"); s.bytes_(a, 4)
                elif ty == 26: sound_seq(s, a)
    _es_depth[0] -= 1


def is_slot_or_null(s, off):
    return s.inb(off, 4) and (s.claim[off] == PTR or s.word(off) == 0)


def is_event_seq(s, script):
    if script == NONE or not s.inb(script, 8):
        return False
    n = s.word(script); entries = s.target(script + 4)
    if n == 0 or n > 0x100 or entries == NONE or not s.inb(entries, n * 0xC):
        return False
    em = 0
    for i in range(n):
        e = entries + i * 0xC; ty = s.word(e)
        if ty > 40:
            return False
        if ty in (1, 2, 3, 4, 5, 6, 7, 8, 10, 25, 26, 29):
            if not is_slot_or_null(s, e + 4) or s.target(e + 8) == NONE:
                return False
            em += 1
    return em != 0


def slave_data(s, ty, data):
    if data == NONE:
        return
    if ty == 1: event_seq(s, data)
    elif ty == 2: emitter(s, data)   # Port_Swap only claims bytes; it's EmitterData
    elif ty == 9:
        s.lab(data, 0x24, "RadarBlip")
        s.bytes_(data, 4)
        for o in (0x1C, 0x20):
            t = s.target(data + o)
            if t != NONE:
                s.lab(t, 4, "GXColor"); s.bytes_(t, 4)
    elif ty == 3: sound_seq(s, data)
    elif ty in (5, 6):
        s.lab(data, 0x24, "AttachData")
        s.bytes_(data, 4)
        s.geom_roots.add(s.target(data + 8))     # attached model (local or HW_Cars import)
        track(s, data + 0xC, KEYS_VEC4); track(s, data + 0x14, KEYS_VEC4); track(s, data + 0x1C, KEYS_COLOR)
    elif ty == 7:
        s.lab(data, 0x54, "Tracer")
        s.bytes_(data + 0x20, 4)
        track(s, data + 0x44, KEYS_COLOR); track(s, data + 0x4C, KEYS_COLOR)
    elif ty == 8: motion_anim(s, data)
    elif ty == 10:
        s.lab(data, 0xC, "SharedSlaves")
        slave_list(s, data + 4)
    else:
        s.lab(data, 4, f"SlaveData.t{ty}")


_sl_depth = [0]


def slave_list(s, off):
    t = s.target(off + 4); n = s.word(off)
    if _sl_depth[0] > 8 or t == NONE:
        return
    _sl_depth[0] += 1
    n = min(n, 0x400)
    s.lab(t, n * 0x20, "SlaveTrigger[]")
    for i in range(n):
        e = t + i * 0x20
        if s.claim[e + 0x10] == BYTES:
            continue
        s.bytes_(e + 0x10, 2); s.u16(e + 0x12); s.bytes_(e + 0x1C, 4)
        slave_data(s, s.blob[e + 0x10], s.target(e + 0x18))
    _sl_depth[0] -= 1


def key_anim(s, off):
    if off == NONE:
        return
    s.lab(off, 0x2C, "KeyAnim")
    flags = s.target(off + 0x1C); n = min(s.word(off + 0x10), 0x100)
    if flags != NONE:
        s.lab(flags, (n * 2 + 3) & ~3, "KeyAnim.chanflags(u16)")
        for i in range(n):
            s.u16(flags + i * 2)
    # times 0x20 (f32[frames]) and data 0x24: not claimed by Port_Swap (words)
    fr = s.word(off + 0x28)
    t = s.target(off + 0x20)
    if t != NONE and fr < 0x10000:
        s.lab(t, fr * 4, "KeyAnim.times")
    d = s.target(off + 0x24)
    if d != NONE:
        s.lab(d, 4, "KeyAnim.data")


def motion_anim(s, off):
    if off == NONE or s.claim[off + 0x1C] == BYTES:
        return
    s.lab(off, 0x3C, "MotionAnim")
    s.bytes_(off + 0x1C, 4)
    key_anim(s, s.target(off + 0x10))
    slave_list(s, off + 0x2C)
    for k in range(3):
        r = s.target(off + 0x20 + 4 * k)
        if r != NONE:
            s.lab(r, 8, "MotionRange")
    p = s.target(off + 0xC); n = min(s.word(off + 8), 0x400)
    if p != NONE:
        s.lab(p, n * 8, "MotionPart[]")
        for i in range(n):
            s.u16(p + i * 8 + 4); s.u16(p + i * 8 + 6)
            s.geom_roots.add(s.target(p + i * 8))
    g = s.target(off + 4); ng = s.word(off)
    if g != NONE and ng < 0x100:
        s.lab(g, ng * 4, "MotionAnim.skinned[]")
        for i in range(ng):
            s.geom_roots.add(s.target(g + 4 * i))
    p = s.target(off + 0x14)
    if p != NONE:
        s.lab(p, 0x14, "Skeleton")
        bones = s.target(p + 0x10); n = min(s.word(p), 0x100)
        if bones != NONE:
            s.lab(bones, n * 0x20, "AnimNode[]")
            for i in range(n):
                s.bytes_(bones + i * 0x20 + 0x1C, 4)


def is_skeleton(s, sk):
    if sk == NONE or not s.inb(sk, 0x14):
        return False
    n = s.word(sk); bones = s.target(sk + 0x10)
    if n == 0 or n > 0x100 or bones == NONE or not s.inb(bones, n * 0x20):
        return False
    ch = 0
    for i in range(n):
        b = bones + i * 0x20
        l = sum(s.f32(b + 4 * k) ** 2 for k in range(4))
        if not (0.9 < l < 1.1):
            return False
        ch += s.blob[b + 0x1C]
    return ch == n - 1


def is_motion_anim(s, m):
    if m == NONE or not s.inb(m, 0x34) or s.word(m) > 0x100 or s.word(m + 8) > 0x400:
        return False
    fps = s.f32(m + 0x18)
    return 1.0 <= fps <= 1000.0 and is_skeleton(s, s.target(m + 0x14))


def is_key_anim(s, a):
    if a == NONE or not s.inb(a, 0x34):
        return False
    bones = s.word(a + 0x10); frames = s.word(a + 0x28)
    t0 = s.f32(a + 8); t1 = s.f32(a + 0xC)
    return (bones != 0 and bones <= 0x100 and frames != 0 and frames <= 0x10000 and
            t0 > -1e7 and t1 < 1e7 and t0 <= t1 and s.target(a + 0x1C) != NONE and
            s.target(a + 0x20) != NONE and s.target(a + 0x24) != NONE and
            s.inb(s.target(a + 0x1C), bones * 2))


def im_header_stub(s, off):
    import walk_im
    walk_im.im_header(s, off)


def car_a_info(s, off):
    s.lab(off, 0x120, "CarAInfo")
    for i in range(4):
        ai_script(s, s.target(off + 0x90 + 4 * i))
    s.bytes_(off + 0x10, 4); s.u16(off + 0x14); s.u16(off + 0x16)
    p = s.target(off + 0x6C)
    if p != NONE:
        s.lab(p, 4, "GXColor(skid)"); s.bytes_(p, 4)
    p = s.target(off + 0x78)
    if p != NONE:
        s.lab(p, 0x20, "CarAModelRef")
        m = s.target(p + 0x10)
        motion_anim(s, m)
        if m != NONE:
            lst = s.target(m + 0x38)
            if lst != NONE:
                s.lab(lst, 8, "CarPartList")
                parts = s.target(lst + 4)
                if parts != NONE:
                    n = min(s.word(lst), 0x100)
                    s.lab(parts, n * 0x1C, "CarPart[]")
                    for i in range(n):
                        s.bytes_(parts + i * 0x1C, 4); s.u16(parts + i * 0x1C + 4); s.u16(parts + i * 0x1C + 6)
                        s.geom_roots.add(s.target(parts + i * 0x1C + 8))
        for k in (0x14, 0x18, 0x1C):
            key_anim(s, s.target(p + k))
    key_anim(s, s.target(off + 0x48))
    s.geom_roots.add(s.target(off + 0x54))      # reflectionGeom
    p = s.target(off + 0x80)
    if p != NONE:
        s.lab(p, 0x90, "CarAEngine")
        camera_list(s, p + 0x80); camera_list(s, p + 0x88)
    p = s.target(off + 0x84)
    if p != NONE:
        s.lab(p, 0x38, "CarAHud")
        s.bytes_(p + 0x34, 4)
        for k in (0x10, 0x18, 0x1C):
            im_header_stub(s, s.target(p + k))
    for k, n, nm in ((0x7C, 4, "f32(grip)"), (0x88, 0x3C, "CarAAccel"), (0x8C, 0x58, "CarAStuntInfo")):
        t = s.target(off + k)
        if t != NONE:
            s.lab(t, n, nm)
    st = s.target(off + 0x8C)
    if st != NONE:
        for cnt, ptr, nm in ((0x34, 0x38, "u32[](rewardThings)"), (0x4C, 0x50, "u32[](failThings)")):
            t = s.target(st + ptr)
            if t != NONE:
                s.lab(t, s.word(st + cnt) * 4, nm)
        t = s.target(st + 0x40)
        if t != NONE:
            n = s.word(st + 0x3C)
            s.lab(t, n * 4, "StuntMsgTables")
            for i in range(min(n, 0x40)):
                t2 = s.target(t + 4 * i)
                if t2 != NONE:
                    s.lab(t2, 4, "StuntMsgTable")
                    s.strs.add(s.target(t2))
        t = s.target(st + 0x48)
        if t != NONE:
            s.lab(t, 4, "StuntFailMsg"); s.strs.add(s.target(t))
    p = s.target(off + 0xA0)
    if p != NONE:
        s.lab(p, 0x4C, "CarACollider")
        s.bytes_(p + 0x10, 4)
        for k, n, nm in ((0x30, 0x1C, "CarAMaterial"), (0x44, 0x20, "Collider.box/spheres"), (0x48, 0x20, "CarAHull")):
            t = s.target(p + k)
            if t != NONE:
                s.lab(t, n, nm)
        t = s.target(p + 0x38)
        if t != NONE:
            s.lab(t, s.word(p + 0x34) * 4, "u32[](endFx)")
        h = s.target(p + 0x48)
        if h != NONE:
            v = s.target(h + 0x1C)
            if v != NONE:
                s.lab(v, s.word(h + 0xC) * 0x10, "Vector4[](hull)")
    p = s.target(off + 0xA4)
    if p != NONE:
        s.lab(p, 0xC, "CarAForcePoints")
        pts = s.target(p + 8); n = min(s.word(p + 4), 0x100)
        if pts != NONE:
            s.lab(pts, n * 0x30, "CarAForcePoint[]")
            for i in range(n):
                s.bytes_(pts + i * 0x30 + 0x10, 4)
    p = s.target(off + 0xE4)
    if p != NONE:
        n = s.word(off + 0xE0)
        s.lab(p, max(n, 11) * 0xC, "CarASlaveSet[](slaves)")
        for i in range(max(n, 11) if n < 0x40 else 11):
            slave_list(s, p + i * 0xC + 4)
    for cnt, ptr, nm in ((0xE8, 0xEC, "CarASlaveSet[](weapons)"), (0xF0, 0xF4, "CarASlaveSet[](pickup)")):
        p = s.target(off + ptr); n = min(s.word(off + cnt), 0x40)
        if p != NONE:
            s.lab(p, n * 0xC, nm)
            for i in range(n):
                slave_list(s, p + i * 12 + 4)
    sound(s, s.target(off + 0x60))
    p = s.target(off + 0xDC)
    if p != NONE:
        s.lab(p, s.word(off + 0xD8) * 8, "CarAGear[]")
        sound(s, s.target(p + 0xC))
    p = s.target(off + 0x11C); n = min(s.word(off + 0x118), 0x40)
    if p != NONE:
        s.lab(p, n * 4, "CarASoundSet*[]")
        for i in range(n):
            ss = s.target(p + 4 * i)
            sound(s, ss)
            if ss != NONE:
                r = s.target(ss + 0x18)
                if r != NONE:
                    s.lab(r, 8, "f32[2](range)")
    # start slaves 0x100/0x104 (0x50 each, u16 tag 0x32) - not in Port_Swap
    p = s.target(off + 0x104); n = min(s.word(off + 0x100), 0x40)
    if p != NONE:
        s.lab(p, n * 0x50, "CarAStartSlave[]")
        for i in range(n):
            s.u16(p + i * 0x50 + 0x30); s.u16(p + i * 0x50 + 0x32)
    p = s.target(off + 0x10C); n = min(s.word(off + 0x108), 0x40)
    if p != NONE:
        s.lab(p, n * 4, "ItemRecord*[](startItems)")
    p = s.target(off + 0x114); n = min(s.word(off + 0x110), 0x40)
    if p != NONE:
        s.lab(p, n * 4, "u32[](shiftSounds)")
    p = s.target(off + 0xD4); n = min(s.word(off + 0xD0), 0x20)
    if p != NONE:
        s.lab(p, n * 0x50, "CarTrailBone[]")
        for i in range(n):
            w = p + i * 0x50
            s.bytes_(w + 0x3C, 4); s.u16(w + 0x40); s.bytes_(w + 0x42, 2)


def is_word_char(c):
    return chr(c).isalnum() or c == 0x5F


def string_targets(s):
    for po in s.pointers:
        t = s.target(po)
        if t == NONE or s.claim[t] != DEFAULT:
            continue
        c0 = s.blob[t]
        if not is_word_char(c0) and (c0 == 0 or chr(c0) not in "\\/.|[(\"'-+#*<!"):
            continue
        n = w = 0
        while t + n < s.size and 0x20 <= s.blob[t + n] < 0x7F:
            w += is_word_char(s.blob[t + n]); n += 1
        if n >= 3 and (w * 2 > n or n >= 8) and t + n < s.size and s.blob[t + n] == 0:
            s.cl(t, n + 1, STR)
            s.lab(t, n + 1, "string")


def walk(d, e):
    s = Pack(d, e)
    s.geom_roots = set(); s.strs = set(); s.unwalked = []
    # textures: region from texture_data_offset to palette/end handled by caller
    for po in s.pointers:                 # shape scans
        m = s.target(po)
        if is_motion_anim(s, m):
            motion_anim(s, m)
    for po in s.pointers:
        t = s.target(po)
        if is_event_seq(s, t):
            event_seq(s, t)
    for po in s.pointers:
        a = s.target(po)
        if is_key_anim(s, a):
            key_anim(s, a)
    tab = s.target(4); n = s.word(0)
    s.lab(0, 8, "RootHeader")
    if tab != NONE:
        s.lab(tab, n * 8, "RootTable")
        for i in range(n):
            o = s.target(tab + 8 * i)
            if o != NONE:
                car_a_info(s, o)
    for t in s.strs:
        if t != NONE:
            n = 0
            while t + n < s.size and s.blob[t + n]:
                n += 1
            s.cl(t, n + 1, STR); s.lab(t, n + 1, "string")
    string_targets(s)
    s.geom_roots.discard(NONE)
    return s


def pointer_targets(s):
    return sorted({s.word(o) for o in s.pointers if s.word(o) < s.size} |
                  {s.word(o) for o, v in s.ref if (v >> 16) == 0 and s.word(o) < s.size})


def geometry_closure(s, roots, stop):
    """All blob ranges reachable from geometry roots through pointer slots.
    `stop`: offsets of labelled (non-geometry) objects that must not be entered.
    Ranges are chunks between consecutive pointer targets."""
    tg = pointer_targets(s)
    import bisect
    seen = set(); stack = [r for r in roots if r != NONE and r < s.size]
    chunks = []
    while stack:
        a = stack.pop()
        if a in seen:
            continue
        seen.add(a)
        i = bisect.bisect_right(tg, a)
        end = tg[i] if i < len(tg) else s.size
        # do not run into a texture region
        chunks.append((a, end))
        for o in range(a & ~3, end, 4):
            if o in s.ptrset or (o in s.refmap and (s.refmap[o] >> 16) == 0):
                t = s.word(o)
                if t < s.size and t not in stop:
                    stack.append(t)
    return chunks
