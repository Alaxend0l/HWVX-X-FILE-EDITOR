import sys, glob, collections
from gclib import GC
st = collections.Counter(); ex = {}
for d in sorted(glob.glob(sys.argv[1] + '/*_car_*')):
    g = GC(d)
    for geo in g.geoms():
        sk = g.u16(geo+4) != 0
        lods = g.T(geo+8)
        for i in range(g.u16(geo+2)):
            lod = lods + i*0x18
            if g.isshadow(lod): continue
            vb = g.vb(lod)
            grp = g.T(lod+8)
            for j in range(g.u16(lod+2)):
                gr = grp + j*0x20; fl = g.u16(gr+2)
                prims, pad = g.dl(gr, sk)
                vs = [v for _,_,x in prims for v in x]
                m0 = max(v[0] for v in vs); m1 = max(v[1] for v in vs); m2 = max(v[2] for v in vs)
                key = (hex(fl), 'pos<' + str(m0 < len(vb['pos'])), 'i1<ncol' + str(m1 < len(vb['col'])), 'i1<nnrm' + str(m1 < len(vb['nrm'])), 'i2<nuv' + str(m2 < len(vb['uv'])), 'ncol=%d' % min(len(vb['col']), 2), 'nn>0' + str(len(vb['nrm']) > 0))
                st[key] += 1; ex.setdefault(key, (d, hex(gr)))
                st['ntri%3', len(vs) % 3] += 1
for k, v in sorted(st.items(), key=str): print(v, k, ex.get(k))
