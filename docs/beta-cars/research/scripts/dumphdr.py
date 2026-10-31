import sys
from ifwalk import W
w = W(sys.argv[1]); h = int(sys.argv[2], 16)
print('header', hex(h), 'id', w.u16(h), 'numObjects', w.u32(h+0xC), 'objs@', hex(w.ptr(h+0x10) or 0))
for v in w.vars(h): print(' VAR', v)
n = w.u32(h+0xC); objs = w.ptr(h+0x10)
for i in range(n):
    o = objs + i*0x6C
    b = w.b
    data = w.ptr(o+0x44)
    did = b[data] if data is not None else None
    steps = []
    evs = []
    if data is not None:
        ns = w.u32(data+8); sp = w.ptr(data+0xC)
        for j in range(ns):
            s = sp + j*0x28
            steps.append((w.u32(s), [w.s32(s+4+4*k) for k in range(4)], list(b[s+0x14:s+0x18])))
        ne = w.u32(data+0x10); ep = w.ptr(data+0x14)
        for j in range(ne):
            d = w.ptr(ep+j*0xC+8)
            if d is None: continue
            t = b[d+5]; by = w.ptr(d+0x18)
            args = w.ptr(d+8); a=None
            if args is not None:
                if t == 0x11: a = (b[args], w.u16(args+2))
            evs.append(dict(in_=b[ep+j*0xC], t=hex(t), id=w.u16(d+6), bytes=b[by:by+4].hex() if by is not None else None, arg=a, nmods=w.u32(d+0x1C)))
    print('OBJ %2d @%x kind=%d type=%d active=%d id=%d dataid=%s next=%s txt=%s' % (i, o, b[o], b[o+1], b[o+8], w.u16(o+0xA), did, list(b[data+1:data+5]) if data is not None else None, w.objtext(o)))
    c = w.conds(o)
    if c: print('    conds', c)
    if steps: print('    steps', steps)
    if evs: print('    events', evs)
    nst = w.u32(o+0x64)
    if nst: print('    states', nst)
hev = w.u32(h+0x14); hl = w.ptr(h+0x18)
for j in range(hev):
    d = w.ptr(hl+j*0xC+8)
    if d is None: continue
    t = w.b[d+5]; by = w.ptr(d+0x18)
    print(' HDR-EVENT in', w.b[hl+j*0xC], 't', hex(t), 'id', w.u16(d+6), 'bytes', w.b[by:by+4].hex() if by is not None else None, 'nmods', w.u32(d+0x1C))
