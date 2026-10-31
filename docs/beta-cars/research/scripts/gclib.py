import sys, struct
sys.path.insert(0, 'SCRATCH')
from xp import P
class GC:
    def __init__(s, d):
        s.p = p = P(d, '>'); s.B = p.blob
        s.texslots = {a: v for a, v in p.tex}
    def u16(s, o): return struct.unpack_from('>H', s.B, o)[0]
    def u32(s, o): return s.p.u32(o)
    def T(s, o): return s.p.u32(o) if o in s.p.ptrs else None
    def isgeom(s, g):
        u16, T, p = s.u16, s.T, s.p
        n = u16(g+2); lods = T(g+8)
        if n == 0 or n > 8 or lods is None or T(g+0xC) is None: return False
        for i in range(n):
            lod = lods + i*0x18; ng = u16(lod+2); grp = T(lod+8)
            if ng == 0 or ng > 0x400 or grp is None or T(lod+0xC) is None: return False
        return True
    def isshadow(s, lod):
        grps = s.T(lod+8); return grps is not None and (s.u16(grps+2) & 0x4000)
    def geoms(s):
        B = s.B
        gs = [g for g in range(0, len(B)-0x40, 0x10) if (g+8) in s.p.ptrs and s.isgeom(g)]
        lodset = set()
        for g in gs:
            for i in range(s.u16(g+2)): lodset.add(s.T(g+8)+i*0x18)
        return [g for g in gs if g not in lodset]
    def vb(s, lod):
        vb = s.T(lod+0xC); h = struct.unpack_from('>4I', s.B, vb)
        d = vb + 0x10
        uvO, cO, nO, size = h
        npos = uvO // 12
        uvEnd = cO or nO or (size - 0x10)
        nuv = (uvEnd - uvO) // 8
        ncol = ((nO or (size-0x10)) - cO)//4 if cO else 0
        nn = ((size - 0x10) - nO)//12 if nO else 0
        pos = [struct.unpack_from('>3f', s.B, d+12*i) for i in range(npos)]
        uv = [struct.unpack_from('>2f', s.B, d+uvO+8*i) for i in range(nuv)]
        col = [s.B[d+cO+4*i:d+cO+4*i+4] for i in range(ncol)]
        nrm = [struct.unpack_from('>3f', s.B, d+nO+12*i) for i in range(nn)]
        return dict(off=vb, hdr=h, pos=pos, uv=uv, col=col, nrm=nrm)
    @staticmethod
    def attrs_for(flags):
        if flags & 0x81: return ('pos', 'col', 'uv')
        if flags & 2: return ('pos', 'nrm', 'col')
        if flags & 4: return ('pos', 'nrm', 'uv')
        return ('pos', 'col', 'uv')
    def dl(s, grp, skinned=False):
        """returns list of (opcode, fmt, [vertex index tuples]) and trailing bytes info"""
        a = s.T(grp+0x1C); n = s.p.u32(grp+0x10); B = s.B
        o = a; end = a + n; prims = []; pad = 0
        while o < end:
            op = B[o]
            if op == 0: o += 1; pad += 1; continue
            if 0x80 <= op < 0xC0:
                cnt = struct.unpack_from('>H', B, o+1)[0]; o += 3
                if skinned:
                    vs = [struct.unpack_from('>B3H', B, o+7*i) for i in range(cnt)]
                    vs = [v[1:] + (v[0],) for v in vs]
                    o += 7*cnt
                else:
                    vs = [struct.unpack_from('>3H', B, o+6*i) for i in range(cnt)]
                    o += 6*cnt
                prims.append((op & 0xF8, op & 7, vs))
            else:
                raise ValueError(f'op {op:02X} at {o:X}')
        return prims, pad
def strip_tris(vs):
    t = []
    for i in range(len(vs)-2):
        a, b, c = vs[i], vs[i+1], vs[i+2]
        t.append((a, b, c) if i % 2 == 0 else (b, a, c))
    return t
def prim_tris(op, vs):
    if op == 0x90: return [tuple(vs[i:i+3]) for i in range(0, len(vs)-2, 3)]
    if op == 0x98: return strip_tris(vs)
    if op == 0xA0: return [(vs[0], vs[i], vs[i+1]) for i in range(1, len(vs)-1)]
    return []
