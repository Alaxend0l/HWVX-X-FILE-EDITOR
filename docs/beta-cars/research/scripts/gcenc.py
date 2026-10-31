# Reference encoder: beta (PS2 v179) texture -> GC (v183) texture record + pixel data.
import struct, numpy as np
from PIL import Image
from texlib import *

def beta_levels(b, h, t):
    """RGBA PIL images for every stored beta level (stored levels = mips-1, or 1 if mips==0)."""
    n = 1 if t['mips'] == 0 else t['mips'] - 1
    w, hh, off, out = t['w'], t['h'], 0, []
    for _ in range(n):
        out.append(beta_decode(b, h, t, level_off=off, w=w, hh=hh))
        off += w * hh * beta_bpp(t['fmt']) // 8
        w, hh = max(w >> 1, 1), max(hh >> 1, 1)
    return out

def to565(c):
    r, g, b = [int(x) for x in c[:3]]
    return ((r * 31 + 127) // 255) << 11 | ((g * 63 + 127) // 255) << 5 | ((b * 31 + 127) // 255)

def exp565(v):
    return np.array([((v >> 11) & 31) * 255 // 31, ((v >> 5) & 63) * 255 // 63, (v & 31) * 255 // 31], float)

def enc_block(px):  # px: 16x4 uint8, row-major 4x4 -> 8 bytes (GC DXT1 sub-block)
    rgb = px[:, :3].astype(float); a = px[:, 3]
    trans = a < 128
    opaque = rgb[~trans]
    if len(opaque) == 0:
        return struct.pack('>HHI', 0, 0, 0xFFFFFFFF)          # c0<=c1 mode, all index 3 (transparent)
    # endpoints: the two extremes along the principal axis of the opaque pixels
    m = opaque.mean(0); d = opaque - m
    ax = np.linalg.svd(d, full_matrices=False)[2][0] if len(opaque) > 1 and d.any() else np.array([1., 1., 1.])
    p = d @ ax
    hi, lo = opaque[p.argmax()], opaque[p.argmin()]
    c0, c1 = to565(hi), to565(lo)
    if trans.any():
        if c0 > c1: c0, c1 = c1, c0                             # 3-colour mode: c0 <= c1, idx3 = transparent
        pal = [exp565(c0), exp565(c1), (exp565(c0) + exp565(c1)) / 2]
    else:
        if c0 < c1: c0, c1 = c1, c0
        if c0 == c1:                                            # flat block: c0>c1 impossible; 3-colour mode is fine (only idx 0 used)
            pal = [exp565(c0)]
        else:
            e0, e1 = exp565(c0), exp565(c1)
            pal = [e0, e1, (2 * e0 + e1) / 3, (e0 + 2 * e1) / 3]
    bits = 0
    for k in range(16):
        if trans[k]: idx = 3
        else: idx = int(np.argmin([((rgb[k] - q) ** 2).sum() for q in pal]))
        bits |= idx << (30 - 2 * k)                             # pixel 0 in the top 2 bits (MSB-first per row byte)
    return struct.pack('>HHI', c0, c1, bits)

def enc_cmpr(img):
    w, h = img.size
    a = np.zeros((-(-h // 8) * 8, -(-w // 8) * 8, 4), np.uint8); a[:h, :w] = np.array(img.convert('RGBA'))
    out = bytearray()
    for by in range(0, a.shape[0], 8):
        for bx in range(0, a.shape[1], 8):
            for sy, sx in ((0, 0), (0, 4), (4, 0), (4, 4)):      # TL, TR, BL, BR
                out += enc_block(a[by + sy:by + sy + 4, bx + sx:bx + sx + 4].reshape(16, 4))
    return bytes(out)

def enc_rgba8(img):
    w, h = img.size
    a = np.zeros((-(-h // 4) * 4, -(-w // 4) * 4, 4), np.uint8); a[:h, :w] = np.array(img.convert('RGBA'))
    out = bytearray()
    for by in range(0, a.shape[0], 4):
        for bx in range(0, a.shape[1], 4):
            t = a[by:by + 4, bx:bx + 4].reshape(16, 4)
            out += bytes(t[:, [3, 0]].reshape(-1))              # 16 x (A,R)
            out += bytes(t[:, [1, 2]].reshape(-1))              # 16 x (G,B)
    return bytes(out)

def choose_format(img):
    a = np.array(img.getchannel('A'))
    partial = ((a > 8) & (a < 247)).mean()
    return 1 if partial > 0.25 else 4                           # heuristic; see spec

def convert(b, h, t, force=None):
    lv = beta_levels(b, h, t)
    fmt = force or choose_format(lv[0])
    if fmt == 1:
        return fmt, 0, enc_rgba8(lv[0])                         # RGBA8: one level only, header mips 0
    w = min(t['w'], t['h'])
    gc_levels = 1 if t['mips'] == 0 else min(t['mips'] - 1, max(1, w.bit_length() - 1 - 3))   # GC chains stop at 16x16
    gc_levels = min(gc_levels, len(lv))
    data = b''.join(enc_cmpr(lv[k]) for k in range(gc_levels))
    return fmt, (0 if gc_levels == 1 else gc_levels), data

def gc_header(fmt, w, h, mips, ptr):
    # u16s little-endian (game byte-reverses them), pointer + unkC big-endian; padded to 0x20
    return struct.pack('<4H', fmt, w, h, mips) + struct.pack('>II', ptr, 0) + bytes(16)

if __name__ == '__main__':
    import sys, os
    d = sys.argv[1] if len(sys.argv) > 1 else '../beta_cars/08_car_7'
    ts, b, h = beta_headers(d)
    tiles = []
    for t in ts:
        fmt, mips, data = convert(b, h, t)
        tt = dict(fmt=fmt, w=t['w'], h=t['h'], mips=mips, ptr=0)
        exp = 0; w, hh = t['w'], t['h']
        for _ in range(max(mips, 1)): exp += gc_level_size(fmt, w, hh); w >>= 1; hh >>= 1
        assert exp == len(data), (exp, len(data))
        dec = gc_decode(data, tt)
        src = beta_levels(b, h, t)[0]
        err = np.abs(np.array(dec, float) - np.array(src, float)).mean(axis=(0, 1))
        print(f"tex{t['i']} beta fmt{t['fmt']} {t['w']}x{t['h']} mips{t['mips']} -> GC fmt{fmt} mips{mips} {len(data)} bytes; mean abs err RGBA {err.round(1)}")
        dec.save(f"tex/{os.path.basename(d)}_t{t['i']}_roundtrip_gc.png")
        tiles.append((src, dec))
    S = Image.new('RGBA', (260 * len(tiles), 520), (255, 0, 255, 255))
    for i, (s, g) in enumerate(tiles):
        S.paste(s.resize((256, 256), Image.NEAREST), (i * 260, 0)); S.paste(g.resize((256, 256), Image.NEAREST), (i * 260, 262))
    S.save(f"sheet_roundtrip_{os.path.basename(d)}.png")
