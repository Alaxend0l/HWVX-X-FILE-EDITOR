# find beta<->GC sound pairs by normalized cross-correlation of decoded PCM
import glob, os, numpy as np
from vag import *; from sndstats import descs
def load(tag, e):
    r = []
    for d in sorted(glob.glob(f'../{tag}/*_*')):
        j, x, ds = descs(d, e)
        if not ds: continue
        for t in ds:
            buf = x[0][t['data']:t['data'] + t['len']]
            pcm = vag_decode(buf)[0] if e == '<' else gc_pcm16(buf)
            r.append((os.path.basename(d), t, np.array(pcm, float)))
    return r
B = load('beta_cars', '<'); G = load('gc_cars', '>')
def ncc(a, b):
    n = 1
    while n < len(a) + len(b): n <<= 1
    c = np.fft.irfft(np.fft.rfft(a, n) * np.conj(np.fft.rfft(b, n)), n)
    k = int(np.argmax(np.abs(c)))
    lag = k if k < n // 2 else k - n
    return c[k] / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-9), lag
best = []
for bn, bt, bp in B:
    if bn == '00_share_2': continue
    cands = []
    for gn, gt, gp in G:
        if gn == '00_share_2': continue
        rb, rg = bt['rate'], gt['rate']
        db, dg = len(bp) / rb, len(gp) / rg
        if abs(db - dg) / dg > 0.25: continue
        a, b = bp, gp
        if rg * 1.5 < rb: a = bp[::2]
        elif rb * 1.5 < rg: b = gp[::2]
        s, lag = ncc(a, b)
        cands.append((abs(s), s, lag, gn, gt['i'], gt['rate'], len(gp), gt['loop']))
    cands.sort(reverse=True)
    if cands and cands[0][0] > 0.5:
        best.append((bn, bt['i'], bt['rate'], len(bp), bt['loop'], cands[0][1:]))
for b in best: print(b)
