# Map beta shared-pack texture/sound indices -> GC shared-pack indices (pack id 1 = HW_share, 2 = HW_Cars 00_share_2)
import numpy as np, json, sys
from PIL import Image
from texlib import *
import sndmatch_lib as sm
PACKS = {1: ('../beta_share/00_share_1', '../gc_share/00_share_1'), 2: ('../beta_cars/00_share_2', '../gc_cars/00_share_2')}
def feat(im):
    a = np.array(im.convert('RGBA').resize((16, 16), Image.BOX), float)
    rgb = a[..., :3] * (a[..., 3:] / 255.0)
    g = rgb.mean(2); g = (g - g.mean()) / (g.std() + 1e-6)
    return g.ravel(), rgb.reshape(-1, 3).mean(0), (im.size)
res = {}
for pid, (bd, gd) in PACKS.items():
    bts, bb, bh = beta_headers(bd); gts, gb = gc_headers(gd)
    bf = [feat(beta_decode(bb, bh, t)) for t in bts]; gf = [feat(gc_decode(gb, t)) for t in gts]
    m = {}
    for i, (s, c, sz) in enumerate(bf):
        sc = []
        for j, (s2, c2, sz2) in enumerate(gf):
            corr = float(s @ s2) / len(s); cd = float(np.abs(c - c2).mean())
            sc.append((corr - cd / 100.0 + (0.05 if sz == sz2 else 0), j, round(corr, 3), round(cd, 1), sz2))
        sc.sort(reverse=True)
        m[i] = sc[0][1:]; print(f'pack{pid} beta tex {i:3d} {sz} -> GC {sc[0][1:]}  2nd {sc[1][1:3]}')
    res[f'tex{pid}'] = m
    # sounds
    B = sm.load_dir(bd, '<'); G = sm.load_dir(gd, '>')
    ms = {}
    for bi, bt, bp in B:
        best = max(((abs(sm.ncc_rs(bp, bt['rate'], gp, gt['rate'])[0]), gi) for gi, gt, gp in G if abs(len(bp) / bt['rate'] - len(gp) / gt['rate']) < 0.3 * len(gp) / gt['rate'] + 0.05), default=(0, None))
        ms[bi] = best; print(f'pack{pid} beta snd {bi} ({len(bp)/bt["rate"]:.2f}s) -> GC {best[1]} ncc {best[0]:.3f}')
    res[f'snd{pid}'] = ms
json.dump({k: {str(a): b for a, b in v.items()} for k, v in res.items()}, open('share_index_map.json', 'w'), indent=1, default=str)
