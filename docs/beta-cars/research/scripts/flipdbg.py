import sys, collections
from betalib import *
from gclib import GC, prim_tris
b = Beta(sys.argv[1]); g = GC(sys.argv[2]); bgr = int(sys.argv[3], 16); ggr = int(sys.argv[4], 16); glod = int(sys.argv[5], 16)
R = lambda t: tuple(round(x, 4) for x in t)
vb = g.vb(glod)
gt = set()
for op, f, vs in g.dl(ggr)[0]:
    for t in prim_tris(op, vs):
        k = tuple(R(vb['pos'][v[0]]) for v in t); gt.add(min(k[i:]+k[:i] for i in range(3)))
for bi, bt in enumerate(b.group_batches(bgr)):
    vs = batch_verts(bt)
    pat = ''.join('A' if v['w'] & 0x8000 else 'k' for v in vs)
    start = 0
    for i, v in enumerate(vs):
        if i >= 1 and (vs[i-1]['w'] & 0x8000) and (v['w'] & 0x8000): start = i-1
        if i >= 2 and not v['w'] & 0x8000:
            t = (vs[i-2], vs[i-1], v) if (i-start) % 2 == 0 else (vs[i-1], vs[i-2], v)
            k = tuple(R(x['pos']) for x in t); k = min(k[j:]+k[:j] for j in range(3))
            if k not in gt:
                print('batch', bi, 'i', i, 'start', start, 'pattern around', pat[max(0, start):i+3], [R(x['pos']) for x in vs[i-2:i+1]])
