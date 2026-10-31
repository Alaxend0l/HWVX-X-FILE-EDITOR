# decode PC and GC textures of exact pairs, compare, write PNG sheets
import sys; sys.path.insert(0, '.')
from pctex import *
os.makedirs('png', exist_ok=True)
st = collections.Counter(); lines = []
for d in sorted(glob.glob(SCR+'/pc_cars/*')):
    n = os.path.basename(d)
    if n.startswith('man'): continue
    pt, pb = pc_headers(d); gt, gb = gc_headers(SCR+'/gc_cars/'+n)
    if len(pt) != len(gt): lines.append(f'{n}: PC {len(pt)} textures vs GC {len(gt)}'); continue
    for p, g in zip(pt, gt):
        pi = np.asarray(pc_decode(pb, p)).astype(int); gi = np.asarray(gc_decode(gb, g)).astype(int)
        if pi.shape != gi.shape: st['shape diff'] += 1; continue
        ga = gi[..., 3]; pa = pi[..., 3]
        gtrans = (ga < 128).mean(); gpart = ((ga > 8) & (ga < 247)).mean()
        m = ga >= 128
        rgb = np.abs(pi[..., :3] - gi[..., :3])[m].mean() if m.any() else 0
        ad = np.abs(pa - ga).mean()
        cls = 'GC RGBA8' if g['fmt'] == 1 else ('GC CMPR w/ transparent' if gtrans > 0 else 'GC CMPR opaque')
        st[(cls, 'PC fmt', p['fmt'])] += 1
        lines.append(f"{n} t{p['i']} {p['w']}x{p['h']} PC fmt{p['fmt']} mips{p['mips']} | GC fmt{g['fmt']} mips{g['mips']} trans={gtrans:.3f} partial={gpart:.3f} | meanRGBdiff={rgb:.1f} meanAdiff={ad:.1f}")
        st['rgbdiff<6', rgb < 6] += 1
        if n in ('01_car_0', '02_car_1', '15_car_14'):
            W = p['w']; H = p['h']
            sheet = Image.new('RGBA', (W*2, H)); sheet.paste(pc_decode(pb, p), (0, 0)); sheet.paste(gc_decode(gb, g), (W, 0))
            sheet.save(f"png/{n}_t{p['i']}_pc_vs_gc.png")
        # mip check: PC level count == log2+1 ?
        if p['mips']:
            import math
            st['pc mips == log2(w)+1', p['mips'] == int(math.log2(max(p['w'], p['h'])))+1] += 1
        st['pc size gap ok'] += 0
open('texcmp.txt', 'w').write('\n'.join(lines) + '\n' + '\n'.join(f'{c} {k}' for k, c in sorted(st.items(), key=str)))
for k, c in sorted(st.items(), key=str): print(c, k)
