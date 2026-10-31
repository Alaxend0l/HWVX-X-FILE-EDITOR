# walk interface headers in an extracted interface pack (GC big endian)
import sys, struct
sys.path.insert(0, 'SCRATCH')
from xp import P
NONE = 0xFFFFFFFF
class W:
    def __init__(s, d):
        s.p = P(d, '>'); s.b = s.p.blob; s.seen = {}
    def u32(s, o): return s.p.u32(o)
    def s32(s, o): return struct.unpack_from('>i', s.b, o)[0]
    def u16(s, o): return struct.unpack_from('>H', s.b, o)[0]
    def ptr(s, o):
        if o not in s.p.ptrs: return None
        v = s.u32(o)
        return v
    def cstr(s, o):
        e = s.b.index(b'\0', o); return s.b[o:e].decode('latin1')
    def headers(s, off, parent=None, depth=0):
        if off is None or off in s.seen: return
        s.seen[off] = (parent, depth)
        n = s.u32(off+0xC); objs = s.ptr(off+0x10)
        for i in range(n if objs is not None else 0):
            o = objs + i*0x6C
            if s.b[o+1] in (6, 7):
                h = s.ptr(o+0x3C)
                if h is not None:
                    hh = s.ptr(h)
                    s.headers(hh, off, depth+1)
            data = s.ptr(o+0x44)
            if data is not None: s.events(s.u32(data+0x10), s.ptr(data+0x14), off, depth)
        s.events(s.u32(off+0x14), s.ptr(off+0x18), off, depth)
    def events(s, n, lst, parent, depth):
        if lst is None: return
        for i in range(n):
            d = s.ptr(lst+i*0xC+8)
            if d is None: continue
            t = s.b[d+5]
            if t in (1, 0x20):
                s.headers(s.ptr(d), parent, depth+1)
    def objtext(s, o):
        sub = s.ptr(o+0x14)
        if sub is None or s.b[o+1] not in (1, 10, 12): return None
        strs = s.ptr(sub)
        if strs is None: return None
        t = s.ptr(strs)
        out = s.cstr(t) if t is not None else None
        if s.b[sub+0x10] == 1:
            lst = s.ptr(sub+8)
            if lst is not None:
                cnt = s.u32(lst); texts = s.ptr(lst+4); var = s.u16(lst+0xA)
                names = []
                for k in range(cnt):
                    tx = s.ptr(texts+4*k)
                    names.append(s.cstr(s.ptr(tx)) if tx is not None and s.ptr(tx) is not None else None)
                out = (out, 'LIST var=%d u8_8=%d count=%d' % (var, s.b[lst+8], cnt), names, lst)
        return out
    def vars(s, off):
        v = s.ptr(off+0x1C); out = []
        if v is None: return out
        n = s.u32(v); lst = s.ptr(v+4)
        for i in range(n):
            e = lst + i*0x34
            cv = s.ptr(e+0x10); cvs = None
            if cv is not None:
                val = s.ptr(cv+4)
                cvs = (hex(s.u32(cv)), (s.b[val], s.u16(val+2)) if val is not None else None)
            out.append(dict(i=i, at=hex(e), type=s.b[e], upd=s.b[e+1], save=s.b[e+2], min=s.s32(e+4), max=s.s32(e+8), off=s.s32(e+0xC), code=cvs))
        return out
    def conds(s, o):
        cl = s.ptr(o+0x48)
        if cl is None: return None
        n = s.u32(cl+4); c = s.ptr(cl+8); out = [s.b[cl]]
        for i in range(n):
            cd = c + i*0x1C; t = s.b[cd]; val = s.s32(cd+4); data = s.ptr(cd+8)
            if t == 1 and data is not None:
                out.append(('var', 'isVar=%d' % s.b[data], s.u16(data+2), 'cmp?%s' % s.b[cd+1:cd+4].hex(), '==', val, 'rest', s.b[cd+0xC:cd+0x1C].hex()))
            elif t == 2 and data is not None:
                vv = s.ptr(data+4)
                out.append(('code', hex(s.u32(data)), (s.b[vv], s.u16(vv+2)) if vv is not None else None, s.b[cd+1:cd+4].hex(), '==', val, s.b[cd+0xC:cd+0x1C].hex()))
            else:
                out.append(('t%d' % t, val, s.b[cd:cd+0x1C].hex()))
        return out
