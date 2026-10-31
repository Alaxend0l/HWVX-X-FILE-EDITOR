# verify the beta 08_car_7 sfx1 <-> GC 32_car_31 sfx1 (and GC 02_car_1 sfx1) pair; plot overlay
import numpy as np
from PIL import Image, ImageDraw
from vag import *; from sndstats import descs
j, (sd, _), ds = descs('../beta_cars/08_car_7', '<'); t = ds[1]
b = np.array(vag_decode(sd[t['data']:t['data'] + t['len']])[0], float)
for g in ('../gc_cars/32_car_31', '../gc_cars/02_car_1'):
    j, (gs, _), gd = descs(g, '>'); u = gd[1]
    gp = np.array(gc_pcm16(gs[u['data']:u['data'] + u['len']]), float)
    print(g, u, 'identical to 32_car_31?' )
    lag = 40
    a = b[lag:lag + len(gp)]; n = min(len(a), len(gp)); a, c = a[:n], gp[:n]
    gain = (a @ c) / (a @ a)
    print(' n', n, 'ncc', (a @ c) / np.linalg.norm(a) / np.linalg.norm(c), 'gain GC/beta', gain, 'rms beta', np.sqrt((a*a).mean()), 'rms gc', np.sqrt((c*c).mean()), 'tail beta after', len(b) - lag - n)
W, H = 1600, 400
im = Image.new('RGB', (W, H), 'white'); d = ImageDraw.Draw(im)
for arr, col, off in ((b[40:], (200, 0, 0), 0), (gp, (0, 0, 200), 0)):
    seg = arr[:1600]
    pts = [(x, H / 2 - seg[x] / 32768 * H / 2) for x in range(len(seg))]
    d.line(pts, fill=col)
d.text((5, 5), 'red: beta 08_car_7 sfx1 (VAG decoded, shifted 40); blue: GC 02_car_1 sfx1 PCM16BE; first 1600 samples', fill='black')
im.save('snd_pair_overlay.png')
wav('snd_pair_beta.wav', [int(x) for x in b], 22062); wav('snd_pair_gc.wav', [int(x) for x in gp], 22050)
