"""Reference GC texture record -> PC texture record (DXT1 / DXT5, full mip chain) + validation.

usage: python3 gc2pc_tex.py [pack ...]   (compares against the real PC textures of the same pack)
"""
import sys; sys.path.insert(0, '.')
from pctex import *
def to_pc_rgba(img_gc):
    """GC car textures are PC textures with every colour channel squared: gc = pc*pc/255. Invert."""
    a = np.asarray(img_gc).astype(np.float64)
    out = a.copy(); out[..., :3] = np.sqrt(a[..., :3] * 255.0)
    return np.clip(np.rint(out), 0, 255).astype(np.uint8)
def choose_fmt(rgba, gc_fmt):
    # PC format 4 is uploaded as GL_COMPRESSED_RGB_S3TC_DXT1 (no alpha), so anything with alpha needs DXT5
    return 5 if (gc_fmt == 1 or (rgba[..., 3] < 255).any()) else 4
def mip_chain(rgba):
    levels = [rgba]
    while levels[-1].shape[0] > 1 or levels[-1].shape[1] > 1:
        a = levels[-1].astype(np.float64); h, w = a.shape[:2]
        h2, w2 = max(1, h//2), max(1, w//2)
        a = a[:h2*2 if h > 1 else 1, :w2*2 if w > 1 else 1]
        a = a.reshape(h2, a.shape[0]//h2, w2, a.shape[1]//w2, 4).mean((1, 3))
        levels.append(np.rint(a).astype(np.uint8))
    return levels
def q565(c):
    r, g, b = (int(c[0])*31+127)//255, (int(c[1])*63+127)//255, (int(c[2])*31+127)//255
    return (r << 11) | (g << 5) | b
def e565(v): return np.array(c565(v), np.float64)
def enc_color_block(px, force4):
    """px: 16x3 float. returns 8 bytes (LE c0,c1, 2-bit indices pixel0 in low bits)"""
    mean = px.mean(0); d = px - mean
    ax = np.linalg.svd(d, full_matrices=False)[2][0] if np.abs(d).sum() > 0 else np.array([1., 1., 1.])
    t = d @ ax; c0 = q565(np.clip(mean + ax*t.max(), 0, 255)); c1 = q565(np.clip(mean + ax*t.min(), 0, 255))
    if c0 < c1: c0, c1 = c1, c0
    if c0 == c1:
        if not force4: return struct.pack('<HHI', c0, c1, 0)
        if c0 > 0: c1 = c0 - 1
        else: c0 = 1
    a, b = e565(c0), e565(c1)
    pal = np.array([a, b, (2*a+b)/3, (a+2*b)/3])
    idx = ((px[:, None, :] - pal[None])**2).sum(2).argmin(1)
    bits = 0
    for i, v in enumerate(idx): bits |= int(v) << (2*i)
    return struct.pack('<HHI', c0, c1, bits)
def enc_alpha_block(al):
    a0, a1 = int(al.max()), int(al.min())
    if a0 == a1: return bytes([a0, a1]) + bytes(6)
    pal = np.array([a0, a1] + [((6-i)*a0 + (1+i)*a1)//7 for i in range(6)], np.float64)
    idx = np.abs(al[:, None] - pal[None]).argmin(1)
    bits = 0
    for i, v in enumerate(idx): bits |= int(v) << (3*i)
    return bytes([a0, a1]) + bits.to_bytes(6, 'little')
def encode_level(rgba, fmt):
    h, w = rgba.shape[:2]; H, W = (h+3)&~3, (w+3)&~3
    pad = np.zeros((H, W, 4), np.uint8); pad[:h, :w] = rgba
    if h < H: pad[h:, :w] = rgba[-1:, :]
    if w < W: pad[:, w:] = pad[:, w-1:w]
    out = bytearray()
    for by in range(0, H, 4):
        for bx in range(0, W, 4):
            blk = pad[by:by+4, bx:bx+4].reshape(16, 4).astype(np.float64)
            if fmt == 5: out += enc_alpha_block(blk[:, 3]) + enc_color_block(blk[:, :3], True)
            else: out += enc_color_block(blk[:, :3], False)
    return bytes(out)
def convert(gb, g):
    """returns (fmt, w, h, mips_field, pixel_bytes)"""
    rgba = to_pc_rgba(gc_decode(gb, g))
    fmt = choose_fmt(rgba, g['fmt'])
    if g['mips'] > 1: levels = mip_chain(rgba)
    else: levels = [rgba]
    data = b''.join(encode_level(l, fmt) for l in levels)
    return fmt, g['w'], g['h'], (len(levels) if len(levels) > 1 else 0), data
if __name__ == '__main__':
    names = sys.argv[1:] or ['01_car_0', '02_car_1', '15_car_14']
    st = collections.Counter()
    for n in names:
        pt, pb = pc_headers(SCR+'/pc_cars/'+n); gt, gb = gc_headers(SCR+'/gc_cars/'+n)
        if len(pt) != len(gt): print(n, 'texture count differs, skipped'); continue
        for p, g in zip(pt, gt):
            fmt, w, h, mips, data = convert(gb, g)
            st['fmt match', fmt == p['fmt']] += 1; st['mips field match', mips == p['mips']] += 1
            st['size match', len(data) == pc_total(p)] += 1
            mine = np.asarray(pc_decode(data, dict(fmt=fmt, w=w, h=h, ptr=0))).astype(int)
            real = np.asarray(pc_decode(pb, p)).astype(int)
            m = real[..., 3] >= 128
            e = np.abs(mine[..., :3] - real[..., :3])[m].mean() if m.any() else 0
            print(f"{n} t{p['i']} {w}x{h} PC fmt{p['fmt']}/mips{p['mips']} -> ours fmt{fmt}/mips{mips} size {len(data)}/{pc_total(p)}  mean|RGB diff| vs real PC = {e:.1f}")
            if p['i'] < 2 and n == names[0]:
                sheet = Image.new('RGBA', (w*3, h)); sheet.paste(pc_decode(pb, p), (0, 0)); sheet.paste(pc_decode(data, dict(fmt=fmt, w=w, h=h, ptr=0)), (w, 0)); sheet.paste(gc_decode(gb, g), (2*w, 0))
                sheet.save(f"png/{n}_t{p['i']}_realPC_ours_GC.png")
    print(dict(st))
