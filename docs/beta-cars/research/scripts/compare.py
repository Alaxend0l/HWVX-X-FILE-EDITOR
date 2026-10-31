import sys, collections
from betalib import *
from gclib import GC, prim_tris
bd, gd, bg, gg = sys.argv[1], sys.argv[2], int(sys.argv[3], 16), int(sys.argv[4], 16)
b = Beta(bd); g = GC(gd)
R = lambda t, n=4: tuple(round(x, n) for x in t)
nl = b.u16(bg+2)
for li in range(nl):
    blod = b.T(bg+8) + 24*li; glod = g.T(gg+8) + 0x18*li
    vb = g.vb(glod)
    print(f'LOD{li}: beta ng={b.B[blod+2]} lod+4={b.u32(blod+4)}  gc ng={g.u16(glod+2)} npos={len(vb["pos"])} nuv={len(vb["uv"])} nnrm={len(vb["nrm"])} ncol={len(vb["col"])}')
    allb_pos = set(); allb_uv = set(); allb_n = set(); allb_v = set()
    for j in range(b.B[blod+2]):
        bgr = b.T(blod+8) + 32*j; ggr = g.T(glod+8) + 32*j
        btris = group_tris(b, bgr)
        fl = g.u16(ggr+2); attrs = GC.attrs_for(fl)
        prims, pad = g.dl(ggr)
        gtris = []
        for op, fmt, vs in prims:
            for t in prim_tris(op, vs):
                gtris.append(t)
        def gvert(v):
            d = dict(pos=R(vb['pos'][v[0]]), uv=R(vb['uv'][v[2]]))
            if attrs[1] == 'nrm': d['nrm'] = R(vb['nrm'][v[1]], 3)
            else: d['col'] = vb['col'][v[1]].hex()
            return d
        bset = collections.Counter(tuple((R(v['pos']), R(v['uv'])) for v in t) for t in btris)
        gset = collections.Counter(tuple((gvert(v)['pos'], gvert(v)['uv']) for v in t) for t in gtris)
        def canon(c, flip=False):
            out = collections.Counter()
            for t, n in c.items():
                if flip: t = (t[1], t[0], t[2])
                k = min([t[i:] + t[:i] for i in range(3)])
                out[k] += n
            return out
        same = sum((canon(bset) & canon(gset)).values())
        samef = sum((canon(bset, True) & canon(gset)).values())
        print(f'  G{j}: beta flags={b.u16(bgr+2):X} tris={len(btris)}  gc flags={fl:X} tris={len(gtris)}  match(same winding)={same} match(flipped)={samef}')
        for t in btris:
            for v in t:
                allb_pos.add(R(v['pos'])); allb_uv.add(R(v['uv']))
                if v['nrm']: allb_n.add(R(v['nrm'], 3))
        # normals check
        if attrs[1] == 'nrm':
            bn = collections.Counter((R(v['pos']), R(v['nrm'], 3)) for t in btris for v in t)
            gn = collections.Counter((gvert(v)['pos'], gvert(v)['nrm']) for t in gtris for v in t)
            print('     normal (pos,nrm) pairs shared:', len(set(bn) & set(gn)), 'of', len(set(bn)), len(set(gn)))
        if btris and 'V4-8' in str(b.group_batches(bgr)[0]['data'].keys()):
            print('     beta rgba sample', [t[0].get((3, 'V4-8')) for t in btris[:4]], 'gc col', [gvert(v).get('col') for v in gtris[0]])
    gp = set(R(p) for p in vb['pos']); gu = set(R(u) for u in vb['uv']); gn = set(R(n, 3) for n in vb['nrm'])
    print(f'  unique pos beta={len(allb_pos)} gc={len(gp)} (gc array {len(vb["pos"])}) common={len(allb_pos & gp)}; uv beta={len(allb_uv)} gc={len(gu)} ({len(vb["uv"])}) common={len(allb_uv & gu)}; nrm beta={len(allb_n)} gc={len(gn)} ({len(vb["nrm"])}) common={len(allb_n & gn)}')
