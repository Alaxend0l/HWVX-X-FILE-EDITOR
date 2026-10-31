import sys, collections
from betalib import *
from gclib import GC, prim_tris
b = Beta(sys.argv[1]); c = GC(sys.argv[2]); bg = int(sys.argv[3], 16); gg = int(sys.argv[4], 16); li = int(sys.argv[5]); j = int(sys.argv[6])
R = lambda t, n=4: tuple(round(x, n) for x in t)
blod = b.T(bg+8)+24*li; glod = c.T(gg+8)+0x18*li; vb = c.vb(glod)
bgr = b.T(blod+8)+32*j; ggr = c.T(glod+8)+32*j
bt = group_tris(b, bgr); gt = [t for op, f, vs in c.dl(ggr, c.u16(gg+4) != 0)[0] for t in prim_tris(op, vs)]
canon = lambda t: min(t[i:]+t[:i] for i in range(3))
bp = collections.Counter(canon(tuple(R(v['pos']) for v in t)) for t in bt)
gp = collections.Counter(canon(tuple(R(vb['pos'][v[0]]) for v in t)) for t in gt)
print('pos-only match', sum((bp & gp).values()), len(bt), len(gt))
# per-vertex uv comparison for pos-matched tris
gmap = {}
for t in gt:
    k = tuple(R(vb['pos'][v[0]]) for v in t)
    gmap[canon(k)] = (k, [vb['uv'][v[2]] for v in t])
diffs = collections.Counter(); n = 0
for t in bt:
    k = tuple(R(v['pos']) for v in t); ck = canon(k)
    if ck in gmap:
        gk, guv = gmap[ck]
        for i, v in enumerate(t):
            gi = gk.index(k[i]); bu = v['uv']; gu = guv[gi]
            if R(bu) != R(gu):
                n += 1
                if n < 12: print('  beta uv', R(bu), 'gc uv', R(gu), 'diff', R((gu[0]-bu[0], gu[1]-bu[1])))
                diffs[R((gu[0]-bu[0], gu[1]-bu[1]), 2)] += 1
print(diffs.most_common(10))
