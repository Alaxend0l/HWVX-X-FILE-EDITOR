# test GC = PC^2/255 (per channel) hypothesis on all paired textures
import sys; sys.path.insert(0, '.')
from pctex import *
st = collections.Counter(); res = []
for d in sorted(glob.glob(SCR+'/pc_cars/*')):
    n = os.path.basename(d)
    if n.startswith('man'): continue
    pt, pb = pc_headers(d); gt, gb = gc_headers(SCR+'/gc_cars/'+n)
    if len(pt) != len(gt): continue
    for p, g in zip(pt, gt):
        pi = np.asarray(pc_decode(pb, p)).astype(float); gi = np.asarray(gc_decode(gb, g)).astype(float)
        m = (gi[..., 3] >= 128) & (pi[..., 3] >= 128)
        P = pi[..., :3][m]; G = gi[..., :3][m]
        if not len(P): continue
        e_id = np.abs(P - G).mean(); e_sq = np.abs(P*P/255 - G).mean(); e_sqrt = np.abs(P - np.sqrt(G*255)).mean()
        best = min(('identity', e_id), ('gc=pc^2/255', e_sq), key=lambda x: x[1])
        st[best[0]] += 1
        res.append(f"{n} t{p['i']} {p['w']} id={e_id:.1f} sq={e_sq:.1f} pc-vs-sqrt(gc)={e_sqrt:.1f} -> {best[0]}")
        # alpha
        if p['fmt'] == 5:
            st['alpha: (pc>=128)==(gc>=128) frac>0.97', ((pi[..., 3] >= 128) == (gi[..., 3] >= 128)).mean() > 0.97] += 1
print('\n'.join(res)); print(st)
