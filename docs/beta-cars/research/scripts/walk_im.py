"""Interface (IMHeader) walkers, port of Port_Swap.c (car HUDs)."""
from walk import *


def obj_anim(s, off):
    if off == NONE: return
    s.lab(off, 0x28, "ObjAnim")
    track(s, off, KEYS_VEC4); track(s, off + 8, KEYS_VEC4); track(s, off + 0x10, KEYS_VEC4)
    track(s, off + 0x18, KEYS_COLOR); track(s, off + 0x20, KEYS_UV)


def state_anim(s, off):
    if off == NONE: return
    s.lab(off, 0x3C, "StateAnim")
    track(s, off, KEYS_ACTIVE); track(s, off + 8, KEYS_VEC4); track(s, off + 0x10, KEYS_COLOR)
    track(s, off + 0x18, KEYS_VEC4); track(s, off + 0x20, KEYS_UV); track(s, off + 0x28, KEYS_VEC4)
    sound_seq(s, s.target(off + 0x30)); action_list(s, off + 0x34)


def var_value(s, off):
    if off != NONE:
        s.lab(off, 4, "VarValue"); s.bytes_(off, 2); s.u16(off + 2)


def action_list(s, off):
    if off == NONE: return
    lst = s.target(off + 4); n = min(s.word(off), 0x400)
    if lst == NONE: return
    s.lab(lst, n * 8, "Action[]")
    for i in range(n):
        a = lst + i * 8; data = s.target(a + 4); ty = s.blob[a]
        s.bytes_(a, 4)
        if data == NONE: continue
        if ty == 1:
            s.lab(data, 8, "ActionData1"); s.bytes_(data, 2); s.u16(data + 2)
        elif ty == 2:
            s.lab(data, 8, "ActionData2")
            cv = s.target(data)
            if cv != NONE:
                s.lab(cv, 8, "CodeVar"); var_value(s, s.target(cv + 4))
            var_value(s, s.target(data + 4))
        elif ty == 3:
            var_value(s, data)


def event_def(s, off):
    if off == NONE or s.claim[off + 4] == BYTES: return
    s.lab(off, 0x24, "EventDef")
    ty = s.blob[off + 5]
    if ty in (1, 0x20): im_header(s, s.target(off))
    elif ty == 0xB: action_list(s, s.target(off))
    s.bytes_(off + 4, 2); s.u16(off + 6)
    sound_seq(s, s.target(off + 0x14))
    args = s.target(off + 8)
    if args != NONE:
        if ty == 0x11: var_value(s, args)
        else:
            s.lab(args, 8, "EventArgs")
            var_value(s, s.target(args)); var_value(s, s.target(args + 4))
    b = s.target(off + 0x18)
    if b != NONE:
        s.lab(b, 4, "EventBytes"); s.bytes_(b, 4)
    mods = s.target(off + 0x20); n = min(s.word(off + 0x1C), 0x100)
    if mods != NONE:
        s.lab(mods, n * 8, "EventMod[]")
        for i in range(n): s.bytes_(mods + i * 8, 4)


def event_list(s, cnt, lo):
    lst = s.target(lo); n = min(s.word(cnt), 0x1000)
    if lst == NONE: return
    s.lab(lst, n * 0xC, "InputEvent[]")
    for i in range(n):
        s.bytes_(lst + i * 0xC, 4); event_def(s, s.target(lst + i * 0xC + 8))


def im_object_data(s, off):
    if off == NONE: return
    s.lab(off, 0x18, "IMObjectData")
    s.bytes_(off, 8)
    st = s.target(off + 0xC); n = min(s.word(off + 8), 0x1000)
    if st != NONE:
        s.lab(st, n * 0x28, "IMStep[]")
        for i in range(n): s.bytes_(st + i * 0x28 + 0x14, 4)
    event_list(s, off + 0x10, off + 0x14)


def font(s, off):
    if off == NONE: return
    s.lab(off, 0x2C, "Font")
    s.bytes_(off + 8, 4)
    sp = s.target(off + 0x24); n = min(s.word(off + 0x20), 0x200)
    if sp != NONE:
        s.lab(sp, n * 0xC, "FontSpacing[]")
        for i in range(n): s.bytes_(sp + i * 0xC, 4)


def cstring(s, off):
    if off == NONE: return
    n = 0
    while off + n < s.size and s.blob[off + n]: n += 1
    s.cl(off, n + 1, STR); s.lab(off, n + 1, "string")


def im_object(s, off):
    s.bytes_(off, 10); s.u16(off + 0xA); s.u16(off + 0xC); s.u16(off + 0xE)
    sub = s.target(off + 0x14); ty = s.blob[off + 1]
    if sub != NONE:
        if ty == 0:
            s.lab(sub, 0x14, "ObjSub")
            if s.blob[sub] == 3:
                an = s.target(sub + 0x10)
                if an != NONE:
                    s.lab(an, 0x10, "TexKeyAnim"); s.bytes_(an + 0xC, 2); s.u16(an + 0xE)
            s.bytes_(sub, 4)
        elif ty == 2:
            s.lab(sub, 8, "DialogList")   # not walked further (no dialogue in car HUDs)
        elif ty in (1, 10, 12):
            s.lab(sub, 0x18, "TextInfo")
            s.bytes_(sub + 0xC, 8); s.u16(sub + 0x14); s.u16(sub + 0x16)
            st = s.target(sub)
            if st != NONE:
                s.lab(st, 4, "StringSlots")
                cstring(s, s.target(st))
            if s.blob[sub + 0x10] == 1:
                lst = s.target(sub + 8)
                if lst != NONE:
                    s.lab(lst, 0xC, "StringList")
                    texts = s.target(lst + 4)
                    s.bytes_(lst + 8, 2); s.u16(lst + 0xA)
                    for k in range(min(s.word(lst), 0x400) if texts != NONE else 0):
                        t = s.target(texts + 4 * k)
                        if t != NONE: cstring(s, s.target(t))
            font(s, s.target(sub + (0x10 if ty == 10 else 4)))
    obj_anim(s, s.target(off + 0x60))
    im_object_data(s, s.target(off + 0x44))
    if ty in (6, 7):
        h = s.target(off + 0x3C)
        if h != NONE: im_header(s, s.target(h))
    cl_ = s.target(off + 0x48)
    if cl_ != NONE:
        s.lab(cl_, 0xC, "CondList")
        c = s.target(cl_ + 8); n = min(s.word(cl_ + 4), 0x100)
        s.bytes_(cl_, 4)
        if c != NONE:
            s.lab(c, n * 0x1C, "Cond[]")
            for i in range(n):
                cond = c + i * 0x1C; data = s.target(cond + 8)
                if s.blob[cond] == 1: var_value(s, data)
                elif s.blob[cond] == 2 and data != NONE:
                    s.lab(data, 8, "CodeVar"); var_value(s, s.target(data + 4))
                s.bytes_(cond, 4)
    st = s.target(off + 0x68); n = min(s.word(off + 0x64), 0x100)
    if st != NONE:
        s.lab(st, n * 0xC, "ObjState[]")
        for i in range(n):
            s.bytes_(st + i * 0xC, 4); state_anim(s, s.target(st + i * 0xC + 8))


def im_header(s, off):
    if off == NONE or s.claim[off] == U16: return
    s.lab(off, 0x40, "IMHeader")
    s.u16(off); s.u16(off + 2); s.bytes_(off + 0x3C, 4)
    objs = s.target(off + 0x10); n = min(s.word(off + 0xC), 500)
    if objs != NONE:
        s.lab(objs, n * 0x6C, "IMObject[]")
        for i in range(n): im_object(s, objs + i * 0x6C)
    event_list(s, off + 0x14, off + 0x18)
    v = s.target(off + 0x1C)
    if v != NONE:
        s.lab(v, 8, "IMVars")
        lst = s.target(v + 4); n = min(s.word(v), 0x1000)
        if lst != NONE:
            s.lab(lst, n * 0x34, "IMVariable[]")
            for i in range(n):
                vv = lst + i * 0x34; cv = s.target(vv + 0x10)
                s.bytes_(vv, 4)
                if cv != NONE:
                    s.lab(cv, 8, "CodeVar")
                    val = s.target(cv + 4)
                    if val != NONE:
                        s.lab(val, 4, "VarValue"); s.bytes_(val, 2); s.u16(val + 2)
    for i in range(0x20, 0x30, 4):
        sound_seq(s, s.target(off + i))
