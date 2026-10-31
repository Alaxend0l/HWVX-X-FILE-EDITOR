# word-wise compare geometry header / LOD / group fields PC vs GC (bbox-paired)
from geocmp import *
import glob, collections
st = collections.Counter(); ex = {}
def w(o, B, e): return struct.unpack_from(e+'I', B, o)[0]
for d in sorted(glob.glob(SCR+'/pc_cars/*car*')):
    n = os.path.basename(d); pc = PC(d); gc = GC(SCR+'/gc_cars/'+n)
    gmap = collections.defaultdict(list)
    for g in gc.geoms(): gmap[gbbox(gc, g)].append(g)
    for p in pc.geoms():
        c = gmap.get(pbbox(pc, p))
        if not c: continue
        g = c.pop(0)
        sh = pc.isshadow(pc.lods(p)[0])
        for o in range(0, 0x40, 4):
            if o in (8, 0xC): continue
            a = w(p+o, pc.B, '<'); b = w(g+o, gc.B, '>')
            if o == 0: a = (pc.u16(p), pc.u16(p+2)); b = (gc.u16(g), gc.u16(g+2))
            k = ('hdr', 'shadow' if sh else 'geo', hex(o))
            st[k + (a == b,)] += 1
            if a != b: ex.setdefault(k, []).append((n, hex(p), a if o == 0 else hex(a), b if o == 0 else hex(b)))
        # distances
        nl = pc.u16(p+2)
        if nl == gc.u16(g+2):
            st[('dist', pc.B[pc.T(p+0xC):pc.T(p+0xC)+4*nl] == bytes(reversed(gc.B[gc.T(g+0xC):gc.T(g+0xC)+4*nl])) or [pc.p.f32(pc.T(p+0xC)+4*i) for i in range(nl)] == [gc.p.f32(gc.T(g+0xC)+4*i) for i in range(nl)])] += 1
            for i in range(nl):
                pl = pc.T(p+8)+0x18*i; gl = gc.T(g+8)+0x18*i
                for o in (0, 4, 0x10):
                    a = w(pl+o, pc.B, '<') if o else (pc.u16(pl), pc.u16(pl+2)); b = w(gl+o, gc.B, '>') if o else (gc.u16(gl), gc.u16(gl+2))
                    if o == 0x10 and (pl+0x10) in pc.p.ptrs: a = 'ptr'
                    if o == 0x10 and (gl+0x10) in gc.p.ptrs: b = 'ptr'
                    k = ('lod', 'shadow' if sh else 'geo', hex(o)); st[k + (a == b,)] += 1
                    if a != b: ex.setdefault(k, []).append((n, hex(pl), a, b))
                if sh:
                    pg = pc.groups(pl); gg = [gc.T(gl+8)+0x20*k for k in range(gc.u16(gl+2))]
                    st[('shadow ngroups same', len(pg) == len(gg))] += 1
                    for a_, b_ in zip(pg, gg):
                        for o in range(0, 0x1C, 4):
                            a = w(a_+o, pc.B, '<'); b = w(b_+o, gc.B, '>')
                            if o == 0: a = (pc.u16(a_), pc.u16(a_+2)); b = (gc.u16(b_), gc.u16(b_+2))
                            if o == 4: a = pc.B[a_+4:a_+8].hex(); b = gc.B[b_+4:b_+8].hex()
                            k = ('sgrp', hex(o)); st[k + (a == b,)] += 1
                            if a != b: ex.setdefault(k, []).append((n, hex(a_), a, b))
for k, c in sorted(st.items(), key=str): print(c, k)
for k, v in ex.items(): print(k, len(v), v[:4])
