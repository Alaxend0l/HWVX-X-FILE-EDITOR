import sys, struct, os
SCR='SCRATCH'
sys.path.insert(0, SCR); sys.path.insert(0, SCR+'/research')
from xp import P
class PC:
    def __init__(s, d):
        s.p = p = P(d, '<'); s.B = p.blob
        s.texslots = {a: v for a, v in p.tex}
        s.ptrsorted = sorted(p.ptrs)
    def u16(s, o): return struct.unpack_from('<H', s.B, o)[0]
    def u32(s, o): return s.p.u32(o)
    def T(s, o): return s.p.u32(o) if o in s.p.ptrs else None
    def isgeom(s, g):
        u16, T = s.u16, s.T
        if u16(g) != 2: return False
        n = u16(g+2); lods = T(g+8)
        if n == 0 or n > 8 or lods is None or T(g+0xC) is None: return False
        for i in range(n):
            lod = lods + i*0x18; ng = u16(lod+2); grp = T(lod+8)
            if ng == 0 or ng > 0x400 or grp is None or T(lod+0xC) is None: return False
        return True
    def geoms(s):
        gs = [g for g in range(0, len(s.B)-0x40, 0x10) if (g+8) in s.p.ptrs and s.isgeom(g)]
        return gs
    def lods(s, g):
        return [s.T(g+8)+i*0x18 for i in range(s.u16(g+2))]
    def groups(s, lod):
        a = s.T(lod+8); return [a+0x20*i for i in range(s.u16(lod+2))]
    def isshadow(s, lod):
        return s.u16(s.T(lod+8)+2) & 0x4000
    def idx(s, grp):
        n = s.u32(grp+0x10); a = s.T(grp+0x1C)
        return list(struct.unpack_from(f'<{3*n}H', s.B, a))
    def vb(s, lod, nv=None):
        vb = s.T(lod+0xC)
        pp, cp, up, np_ = struct.unpack_from('<4I', s.B, vb)
        if nv is None:
            nv = max(max(s.idx(g)) for g in s.groups(lod)) + 1
        B = s.B
        pos = [struct.unpack_from('<3f', B, pp+16*i) for i in range(nv)]
        uv = [struct.unpack_from('<2f', B, up+8*i) for i in range(nv)] if (vb+8) in s.p.ptrs else None
        col = [B[cp+4*i:cp+4*i+4] for i in range(nv)] if (vb+4) in s.p.ptrs else None
        nrm = [struct.unpack_from('<3f', B, np_+16*i) for i in range(nv)] if (vb+0xC) in s.p.ptrs else None
        return dict(off=vb, ptrs=(pp, cp, up, np_), nv=nv, pos=pos, uv=uv, col=col, nrm=nrm, tail=B[vb+0x10:vb+0x20])
