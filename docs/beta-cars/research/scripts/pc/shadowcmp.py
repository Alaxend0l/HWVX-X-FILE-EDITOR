# compare PC vs GC shadow-volume geometry
from geocmp import *
import glob, collections, math
st = collections.Counter()
def edges(o, B, e, n):
    out = []
    for i in range(n):
        a = o + 0x20*i
        n0 = struct.unpack_from(e+'3f', B, a); v0, v1 = struct.unpack_from(e+'2H', B, a+0xC)
        n1 = struct.unpack_from(e+'3f', B, a+0x10); t = struct.unpack_from(e+'i', B, a+0x1C)[0]
        out.append((v0, v1, n0, n1, t))
    return out
for d in sorted(glob.glob(SCR+'/pc_cars/*car*')):
    nm = os.path.basename(d); pc = PC(d); gc = GC(SCR+'/gc_cars/'+nm)
    ps = [g for g in pc.geoms() if pc.isshadow(pc.lods(g)[0])]
    gs = [g for g in gc.geoms() if gc.isshadow(gc.T(g+8))]
    for p, g in zip(ps, gs):
        pl = pc.T(p+8); gl = gc.T(g+8)
        st['lod14->self', pc.u32(pl+0x14) == pl] += 1
        pg = pc.groups(pl); gg = [gc.T(gl+8)+0x20*k for k in range(gc.u16(gl+2))]
        # position arrays
        pp = pc.T(pl+0xC); gp = gc.T(gl+0xC)
        pmax = max(max(max(e[0], e[1]) for e in edges(pc.T(x+0x1C), pc.B, '<', pc.u32(x+0x10))) for x in pg if pc.u32(x+0x10))
        gmax = max(max(max(e[0], e[1]) for e in edges(gc.T(x+0x1C), gc.B, '>', gc.u32(x+0x10))) for x in gg if gc.u32(x+0x10))
        P = [struct.unpack_from('<3f', pc.B, pp+12*i) for i in range(pmax+1)]
        G = [struct.unpack_from('>3f', gc.B, gp+12*i) for i in range(gmax+1)]
        st['pos array same prefix', P[:min(len(P), len(G))] == G[:min(len(P), len(G))]] += 1
        st['pos len cmp', (len(P) > len(G)) - (len(P) < len(G))] += 1
        st['groups ptr after pos', pc.T(pg[0]+0x1C) - (pp + 12*len(P))] += 1
        for a, b in zip(pg, gg):
            pe = edges(pc.T(a+0x1C), pc.B, '<', pc.u32(a+0x10)); ge = edges(gc.T(b+0x1C), gc.B, '>', gc.u32(b+0x10))
            def k(e): return (tuple(P[e[0]]), tuple(P[e[1]])) 
            def kg(e): return (tuple(G[e[0]]), tuple(G[e[1]]))
            ps_ = {k(e): e for e in pe}; gs_ = {kg(e): e for e in ge}
            st['gc edges subset of pc', set(gs_) <= set(ps_)] += 1
            for e in pe:
                z1 = e[3] == (0.0, 0.0, 0.0)
                cop = (not z1) and max(abs(x - y) for x, y in zip(e[2], e[3])) < 1e-6
                inG = k(e) in gs_
                st[('pc edge', 'n1=0' if z1 else 'coplanar' if cop else 'crease', 'inGC' if inG else 'notGC')] += 1
                st['type', e[4]] += 1
            for kk, e in gs_.items():
                if kk in ps_:
                    q = ps_[kk]
                    st['n0 equal', max(abs(x-y) for x, y in zip(q[2], e[2])) < 1e-5] += 1
                    st['n1 equal', max(abs(x-y) for x, y in zip(q[3], e[3])) < 1e-5] += 1
            st['edge order same (common)', [x for x in map(k, pe) if x in gs_] == list(map(kg, ge))] += 1
for k, c in sorted(st.items(), key=str): print(c, k)
