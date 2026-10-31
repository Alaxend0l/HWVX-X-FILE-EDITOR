"""Minimal texture decoders (base level only) for matching shared textures
between builds.  PS2 beta: header 0x10 {u16 fmt, w, h, mips; pixels ptr; u32
palette index}, fmt 2 = 8bpp indexed (palette 256 x RGBA, alpha 0..0x80, CSM1
swizzle), fmt 1 = 32bpp RGBA linear.  GC: header 0x20 {LE u16 fmt, w, h, mips;
BE pixels ptr}, fmt 4 = CMPR, fmt 1 = RGBA8 tiled."""
import struct, json
import numpy as np


def _load(d):
    blob = open(d + "/data.bin", "rb").read()
    h = json.load(open(d + "/pack.json"))["header"]
    return blob, h


def ps2_textures(d):
    blob, h = _load(d)
    out = []
    o = h["texture_data_offset"]
    for i in range(h["num_textures"]):
        f, w, hh, m = struct.unpack_from("<4H", blob, o + 0x10 * i)
        pix, pal = struct.unpack_from("<2I", blob, o + 0x10 * i + 8)
        if f == 2:
            idx = np.frombuffer(blob, np.uint8, w * hh, pix)
            p = np.frombuffer(blob, np.uint8, 1024, h["palette_offset"] + 1024 * pal).reshape(256, 4).copy()
            # CSM1: swap entries 8-15 and 16-23 in every 32
            q = p.copy()
            for b in range(0, 256, 32):
                q[b + 8:b + 16] = p[b + 16:b + 24]; q[b + 16:b + 24] = p[b + 8:b + 16]
            img = q[idx].reshape(hh, w, 4).astype(np.float32)
        elif f == 1:
            img = np.frombuffer(blob, np.uint8, w * hh * 4, pix).reshape(hh, w, 4).astype(np.float32)
        else:
            img = None
        if img is not None:
            img[..., 3] = np.minimum(img[..., 3] * 2, 255)
        out.append(((f, w, hh, m), img))
    return out


def _rgb565(c):
    r = (c >> 11) & 31; g = (c >> 5) & 63; b = c & 31
    return np.array([r * 255 // 31, g * 255 // 63, b * 255 // 31, 255], np.float32)


def _cmpr(blob, pix, w, h):
    img = np.zeros((h, w, 4), np.float32)
    o = pix
    for by in range(0, h, 8):
        for bx in range(0, w, 8):
            for sb in range(4):
                sx = bx + (sb & 1) * 4; sy = by + (sb >> 1) * 4
                c0, c1, bits = struct.unpack_from(">HHI", blob, o); o += 8
                a, b = _rgb565(c0), _rgb565(c1)
                if c0 > c1:
                    pal = [a, b, (2 * a + b) / 3, (a + 2 * b) / 3]
                else:
                    pal = [a, b, (a + b) / 2, np.array([0, 0, 0, 0], np.float32)]
                for k in range(16):
                    v = (bits >> (30 - 2 * k)) & 3
                    y, x = sy + k // 4, sx + k % 4
                    if y < h and x < w:
                        img[y, x] = pal[v]
    return img


def _rgba8(blob, pix, w, h):
    img = np.zeros((h, w, 4), np.float32)
    o = pix
    for ty in range(0, h, 4):
        for tx in range(0, w, 4):
            ar = blob[o:o + 32]; gb = blob[o + 32:o + 64]; o += 64
            for k in range(16):
                y, x = ty + k // 4, tx + k % 4
                img[y, x] = (ar[2 * k + 1], gb[2 * k], gb[2 * k + 1], ar[2 * k])
    return img


def gc_textures(d):
    blob, h = _load(d)
    out = []
    o = h["texture_data_offset"]
    for i in range(h["num_textures"]):
        f, w, hh, m = struct.unpack_from("<4H", blob, o)
        pix = struct.unpack_from(">I", blob, o + 8)[0]
        if f == 4: img = _cmpr(blob, pix, w, hh)
        elif f == 1: img = _rgba8(blob, pix, w, hh)
        else: img = None
        out.append(((f, w, hh, m), img))
        o += 0x20
    return out


def thumb(img, n=8):
    if img is None: return None
    h, w, _ = img.shape
    if h < n or w < n: return None
    return img.reshape(n, h // n, n, w // n, 4).mean(axis=(1, 3))


def match(A, B):
    """A, B: lists of (hdr, img). Returns for each A index the best B (index, score) list."""
    TA = [thumb(i) for _, i in A]; TB = [thumb(i) for _, i in B]
    res = []
    for i, ta in enumerate(TA):
        sc = []
        for j, tb in enumerate(TB):
            if ta is None or tb is None: continue
            # colour only where both are mostly opaque; alpha compared separately
            d = np.abs(ta[..., :3] - tb[..., :3]).mean() + 0.5 * np.abs(ta[..., 3] - tb[..., 3]).mean()
            sc.append((round(float(d), 1), j))
        sc.sort()
        res.append(sc[:3])
    return res


if __name__ == "__main__":
    import sys
    from PIL import Image
    A = ps2_textures(sys.argv[1]); B = gc_textures(sys.argv[2])
    out = sys.argv[3] if len(sys.argv) > 3 else None
    for i, r in enumerate(match(A, B)):
        print(f"beta {i:3d} {A[i][0]} -> " + ", ".join(f"gc {j} {B[j][0]} d={s}" for s, j in r))
    if out:
        import os; os.makedirs(out, exist_ok=True)
        for tag, L in (("b", A), ("g", B)):
            for i, (hd, img) in enumerate(L):
                if img is not None:
                    Image.fromarray(np.clip(img, 0, 255).astype(np.uint8), "RGBA").save(f"{out}/{tag}{i:03d}.png")
