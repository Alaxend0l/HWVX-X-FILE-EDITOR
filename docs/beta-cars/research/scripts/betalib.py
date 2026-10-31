import sys, struct, collections
from vif import walk, P
class Beta:
    def __init__(s, d):
        s.p = p = P(d, '<'); s.B = p.blob
        for a, v in p.refr:
            if not v >> 16: p.ptrs.add(a)
        s.texslots = {a: v for a, v in p.tex}
    def u16(s, o): return struct.unpack_from('<H', s.B, o)[0]
    def u32(s, o): return struct.unpack_from('<I', s.B, o)[0]
    def T(s, o): return s.u32(o) if o in s.p.ptrs else None
    def geoms(s):
        B = s.B
        return [o for o in range(0, len(B)-0x40, 0x10) if s.u16(o) == 0x1005 and (o+8) in s.p.ptrs and (o+0xC) in s.p.ptrs]
    def group_batches(s, grp):
        """returns list of batches; each batch = dict(giftag, verts=[(xyz, adc, st, nrm or None, rgba or None)])"""
        B = s.B
        d = s.T(grp+0xC)
        qwc = s.u32(d+0x10) & 0xFFFF
        cmds = walk(B, d+0x20, qwc*16)
        batches = []; cur = None; cyc = None
        for c in cmds:
            if c[0] == 'UNPACK':
                kind, n, addr, off = c[1], c[2], c[3], c[6]
                if addr == 0 and n == 1:
                    lo, hi = struct.unpack_from('<QQ', B, off)
                    cur = dict(nloop=lo & 0x7FFF, eop=(lo >> 15) & 1, prim=(lo >> 47) & 0x7FF, flg=(lo >> 58) & 3, nreg=lo >> 60, regs=hi, data={})
                    batches.append(cur)
                else:
                    if kind == 'V4-32': vals = [struct.unpack_from('<3fI', B, off+16*i) for i in range(n)]
                    elif kind == 'V2-32': vals = [struct.unpack_from('<2f', B, off+8*i) for i in range(n)]
                    elif kind == 'V3-32': vals = [struct.unpack_from('<3f', B, off+12*i) for i in range(n)]
                    elif kind == 'V4-8': vals = [tuple(B[off+4*i:off+4*i+4]) for i in range(n)]
                    else: vals = (kind, off)
                    cur['data'][(addr, kind)] = vals
            elif c[0] == 'STCYCL':
                cyc = c[1]
        return batches
    def group_strips(s, grp):
        """flatten into strips: list of list of vertex dicts"""
        strips = []
        for b in s.group_batches(grp):
            dd = b['data']
            pos = dd.get((1, 'V4-32')); st = dd.get((2, 'V2-32')); nrm = dd.get((3, 'V3-32'))
            other = {k: v for k, v in dd.items() if k not in ((1, 'V4-32'), (2, 'V2-32'), (3, 'V3-32'))}
            cur = None
            for i, (x, y, z, w) in enumerate(pos):
                v = dict(pos=(x, y, z), w=w, uv=st[i] if st else None, nrm=nrm[i] if nrm else None)
                for k, vals in other.items():
                    if isinstance(vals, list): v[k] = vals[i]
                if w & 0x8000 and (cur is None or len(cur) >= 2):
                    cur = [v]; strips.append(cur)
                else:
                    cur.append(v)
        return strips
def strip_to_tris(strip):
    t = []
    for i in range(2, len(strip)):
        a, b, c = strip[i-2], strip[i-1], strip[i]
        t.append((a, b, c) if i % 2 == 0 else (b, a, c))
    return t

def batch_verts(b):
    dd = b['data']
    pos = dd.get((1, 'V4-32')); st = dd.get((2, 'V2-32')); nrm = dd.get((3, 'V3-32'))
    other = {k: v for k, v in dd.items() if k not in ((1, 'V4-32'), (2, 'V2-32'), (3, 'V3-32'))}
    vs = []
    for i, (x, y, z, w) in enumerate(pos):
        v = dict(pos=(x, y, z), w=w, uv=st[i] if st else None, nrm=nrm[i] if nrm else None)
        for k, vals in other.items():
            if isinstance(vals, list): v[k] = vals[i]
        vs.append(v)
    return vs
def _same(a, b):
    return a['pos'] == b['pos'] and a['uv'] == b['uv'] and a['nrm'] == b['nrm']
def batch_tris(vs, offset=0):
    """GS semantics: the vertex queue restarts with each batch (GIF tag PRE=1).
    A vertex with ADC (w bit 15) clear kicks a triangle from the last 3 vertices.
    Strip start = two consecutive ADC vertices; triangle k of a strip (k = position
    of its last vertex - 2) is (v0,v1,v2) if k even else (v1,v0,v2).
    offset: strip position of vertex 0 when the batch continues a strip from the
    previous batch.  Returns (tris, strip position of the last vertex)."""
    tris = []; start = -offset
    for i, v in enumerate(vs):
        if i >= 1 and (vs[i-1]['w'] & 0x8000) and (v['w'] & 0x8000) and not (i == 1 and offset):
            start = i - 1
        if i >= 2 and not (v['w'] & 0x8000):
            a, b_, c = vs[i-2], vs[i-1], v
            tris.append((a, b_, c) if (i - start) % 2 == 0 else (b_, a, c))
    return tris, len(vs) - 1 - start
def group_tris(beta, grp):
    t = []; prev = None; last = 0
    for b in beta.group_batches(grp):
        vs = batch_verts(b); off = 0
        # continuation: new batch re-sends the previous batch's last two vertices
        if prev is not None and len(vs) >= 3 and not (prev[-1]['w'] & 0x8000) and \
           _same(vs[0], prev[-2]) and _same(vs[1], prev[-1]) and not (vs[2]['w'] & 0x8000):
            off = last - 1
        tt, last = batch_tris(vs, off); t += tt; prev = vs
    return t
