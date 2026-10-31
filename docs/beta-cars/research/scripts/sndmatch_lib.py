import numpy as np
from vag import *; from sndstats import descs
def load_dir(d, e):
    j, x, ds = descs(d, e); r = []
    for t in ds:
        buf = x[0][t['data']:t['data'] + t['len']]
        r.append((t['i'], t, np.array(vag_decode(buf)[0] if e == '<' else gc_pcm16(buf), float)))
    return r
def ncc(a, b):
    n = 1
    while n < len(a) + len(b): n <<= 1
    c = np.fft.irfft(np.fft.rfft(a, n) * np.conj(np.fft.rfft(b, n)), n)
    k = int(np.argmax(np.abs(c)))
    return c[k] / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-9), (k if k < n // 2 else k - n)
def ncc_rs(a, ra, b, rb):
    if rb * 1.5 < ra: a = a[::2]
    elif ra * 1.5 < rb: b = b[::2]
    return ncc(a, b)
