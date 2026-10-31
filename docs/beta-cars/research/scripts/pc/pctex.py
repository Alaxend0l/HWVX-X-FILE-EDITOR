# PC texture headers + DXT1/DXT5 decoders, and GC<->PC comparison
import sys, struct, json, os, glob, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)) + '/..')
from texlib import load, gc_headers, gc_decode, gc_level_size
from PIL import Image
import numpy as np
SCR = 'SCRATCH'
def pc_headers(d):
    h, b = load(d); o = h['texture_data_offset']; out = []
    for i in range(h['num_textures']):
        fmt, w, ht, mips, ptr, unk = struct.unpack_from('<4HII', b, o + 0x10*i)
        out.append(dict(i=i, hdr=o+0x10*i, fmt=fmt, w=w, h=ht, mips=mips, ptr=ptr, unkC=unk))
    return out, b
def pc_level_size(fmt, w, h):
    bw, bh = (w + 3) & ~3, (h + 3) & ~3
    if fmt == 4: return bw*bh//2
    if fmt == 5: return bw*bh
    if fmt == 1: return w*h*4
    if fmt == 0: return w*h*3
    raise ValueError(fmt)
def pc_total(t):
    w, h, n = t['w'], t['h'], max(1, t['mips']); s = 0
    for _ in range(n):
        s += pc_level_size(t['fmt'], w, h); w = max(1, w//2); h = max(1, h//2)
    return s
def c565(c):
    return (((c >> 11) & 31)*255//31, ((c >> 5) & 63)*255//63, (c & 31)*255//31)
def dxt_color(b, p, force4):
    c0, c1, bits = struct.unpack_from('<HHI', b, p)
    a, bb = c565(c0), c565(c1)
    if c0 > c1 or force4:
        pal = [a+(255,), bb+(255,), tuple((2*a[k]+bb[k])//3 for k in range(3))+(255,), tuple((a[k]+2*bb[k])//3 for k in range(3))+(255,)]
    else:
        pal = [a+(255,), bb+(255,), tuple((a[k]+bb[k])//2 for k in range(3))+(255,), (0, 0, 0, 0)]
    return [pal[(bits >> (2*i)) & 3] for i in range(16)]
def dxt5_alpha(b, p):
    a0, a1 = b[p], b[p+1]; bits = int.from_bytes(b[p+2:p+8], 'little')
    if a0 > a1: pal = [a0, a1] + [((6-i)*a0 + (1+i)*a1)//7 for i in range(6)]
    else: pal = [a0, a1] + [((4-i)*a0 + (1+i)*a1)//5 for i in range(4)] + [0, 255]
    return [pal[(bits >> (3*i)) & 7] for i in range(16)]
def pc_decode(b, t, level=0):
    w, h, p = t['w'], t['h'], t['ptr']
    for _ in range(level):
        p += pc_level_size(t['fmt'], w, h); w = max(1, w//2); h = max(1, h//2)
    a = np.zeros((h, w, 4), np.uint8)
    if t['fmt'] in (4, 5):
        for by in range(0, h, 4):
            for bx in range(0, w, 4):
                if t['fmt'] == 5:
                    al = dxt5_alpha(b, p); col = dxt_color(b, p+8, True); p += 16
                    col = [c[:3]+(al[i],) for i, c in enumerate(col)]
                else:
                    col = dxt_color(b, p, False); p += 8
                for i, c in enumerate(col):
                    x, y = bx + (i & 3), by + (i >> 2)
                    if x < w and y < h: a[y, x] = c
    elif t['fmt'] == 1:
        a[:] = np.frombuffer(b, np.uint8, w*h*4, p).reshape(h, w, 4)
    elif t['fmt'] == 0:
        a[..., :3] = np.frombuffer(b, np.uint8, w*h*3, p).reshape(h, w, 3); a[..., 3] = 255
    return Image.fromarray(a, 'RGBA')
def gc_cmpr_to_dxt1(b, off, w, h):
    """re-tile GC CMPR to PC DXT1 (lossless): swap endian + reverse 2-bit order per row"""
    tw, th = -(-w//8)*8, -(-h//8)*8
    blocks = {}
    p = off
    for by in range(0, th, 8):
        for bx in range(0, tw, 8):
            for sb in range(4):
                sx, sy = bx + (sb & 1)*4, by + (sb >> 1)*4
                c0, c1 = struct.unpack_from('>HH', b, p); rows = b[p+4:p+8]; p += 8
                bits = 0
                for y in range(4):
                    r = rows[y]
                    for x in range(4):
                        bits |= ((r >> (6 - 2*x)) & 3) << (2*(y*4 + x))
                blocks[(sx, sy)] = struct.pack('<HHI', c0, c1, bits)
    out = b''.join(blocks[(x, y)] for y in range(0, max(4, h), 4) for x in range(0, max(4, w), 4))
    return out
