# survey all car-pack textures: formats, sizes, mip semantics (gap between data pointers), alpha
import glob, os, collections, struct
from texlib import *
R = '..'
def gaps(ts, end):
    ps = sorted(t['ptr'] for t in ts)
    return {p: (ps[k+1] if k+1 < len(ps) else end) - p for k, p in enumerate(ps)}
print("== GC ==")
c = collections.Counter()
for d in sorted(glob.glob(R+'/gc_cars/*_*')):
    if not os.path.isdir(d): continue
    h, b = load(d)
    if h['num_textures'] == 0: continue
    ts, b = gc_headers(d); g = gaps(ts, h['texture_data_offset'] if max(t['ptr'] for t in ts) < h['texture_data_offset'] else len(b))
    for t in ts:
        lv = max(t['mips'], 1); w, hh = t['w'], t['h']; s = 0
        for _ in range(lv): s += gc_level_size(t['fmt'], w, hh); w = max(w>>1,1); hh = max(hh>>1,1)
        s1 = gc_level_size(t['fmt'], t['w'], t['h'])
        gap = g[t['ptr']]
        key = (t['fmt'], t['mips'], 'match' if gap - s < 32 and gap >= s else ('L1' if gap-s1 < 32 and gap>=s1 else f'gap{gap}/s{s}'))
        c[key] += 1
        if t['unkC'] or (t['w'] != t['h']): print(os.path.basename(d), t)
    # header placement
print(c)
print("== beta ==")
c = collections.Counter(); al = collections.Counter(); hdrpos = collections.Counter()
for d in sorted(glob.glob(R+'/beta_cars/*_*')):
    if not os.path.isdir(d): continue
    h, b = load(d)
    if h['num_textures'] == 0: continue
    ts, b, h = beta_headers(d)
    g = gaps(ts, h['texture_data_offset'])
    hdrpos[('pix<hdr' if max(t['ptr'] for t in ts) < h['texture_data_offset'] else 'pix>hdr')] += 1
    for t in ts:
        bpp = beta_bpp(t['fmt']); gap = g[t['ptr']]
        w, hh = t['w'], t['h']; lv = 0; s = 0
        sizes = []
        while True:
            s += w*hh*bpp//8; lv += 1; sizes.append(s)
            if s >= gap or w == 1 or hh == 1: break
            w >>= 1; hh >>= 1
        nl = sizes.index(gap)+1 if gap in sizes else f'?gap{gap} {sizes}'
        c[(t['fmt'], t['mips'], nl)] += 1
        if t['w'] != t['h']: print(os.path.basename(d), t)
        if t['fmt'] == 2 and t['clut'] >= h['num_palettes']: print('clut OOR', d, t)
    for k in range(h['num_palettes']):
        for e in range(256):
            al[b[h['palette_offset'] + k*0x400 + 4*e + 3]] += 1
print(c); print(hdrpos)
print('palette alpha hist', sorted(al.items()))
