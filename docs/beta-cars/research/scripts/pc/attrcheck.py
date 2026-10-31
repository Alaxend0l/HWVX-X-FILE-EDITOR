# for every PC vertex, check which attributes are recoverable from the GC DL triple
from geocmp import *
import collections, glob
def lodpairs(pc, gc):
    gmap = collections.defaultdict(list)
    for g in gc.geoms(): gmap[gbbox(gc, g)].append(g)
    for p in pc.geoms():
        c = gmap.get(pbbox(pc, p))
        if not c: continue
        g = c.pop(0)
        if gc.u16(g+4) or pc.u16(p+2) != gc.u16(g+2): continue
        for i, pl in enumerate(pc.lods(p)):
            gl = gc.T(g+8)+i*0x18
            if pc.isshadow(pl): continue
            pg = pc.groups(pl); gg = [gc.T(gl+8)+0x20*k for k in range(gc.u16(gl+2))]
            if len(pg) != len(gg): continue
            yield pl, gl, list(zip(pg, gg))
if __name__ == '__main__':
    st = collections.Counter()
    for d in sorted(glob.glob(SCR+'/pc_cars/*car*')):
        n = os.path.basename(d)
        pc = PC(d); gc = GC(SCR+'/gc_cars/'+n)
        for pl, gl, grs in lodpairs(pc, gc):
            pv = pc.vb(pl); gv = gc.vb(gl)
            for a, b in grs:
                f = gc.u16(b+2)
                if pc.u32(a+0x10)*3 != sum(len(x[2]) for x in gc.dl(b)[0]): continue
                gi = [v for _, _, vs in gc.dl(b)[0] for v in vs]
                pi = pc.idx(a)
                cls = 'alpha' if f & 0x81 else 'env' if f & 2 else 'lit' if f & 0x14 else 'unlit'
                for (A, Bi, C), j in zip(gi, pi):
                    if pv['pos'][j] != gv['pos'][A]: st[(cls, 'pos!=')] += 1
                    if cls in ('lit', 'env'):
                        st[(cls, 'nrm', pv['nrm'][j] == gv['nrm'][Bi])] += 1
                        if pv['col']: st[(cls, 'col', pv['col'][j].hex())] += 1
                    else:
                        st[(cls, 'col', pv['col'][j] == gv['col'][Bi] if Bi < len(gv['col']) else 'oob')] += 1
                        if pv['nrm']: st[(cls, 'nrm-present')] += 1
                    if cls == 'env':
                        st[(cls, 'uv[idxC]', C < len(gv['uv']) and pv['uv'][j] == gv['uv'][C])] += 1
                    else:
                        st[(cls, 'uv', pv['uv'][j] == gv['uv'][C])] += 1
    for k, c in sorted(st.items(), key=str): print(c, k)
