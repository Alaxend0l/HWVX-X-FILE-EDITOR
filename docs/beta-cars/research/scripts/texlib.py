# Texture decoders for HWVX car packs (beta PS2 v179 LE / GC v183 BE)
import struct, json, os
from PIL import Image

def load(d):
    j = json.load(open(d + "/pack.json"))["header"]
    blob = open(d + "/data.bin", "rb").read()
    return j, blob

# ---------------- GC ----------------
def gc_headers(d):
    h, b = load(d)
    o = h["texture_data_offset"]; out = []
    for i in range(h["num_textures"]):
        fmt, w, ht, mips = struct.unpack_from("<4H", b, o)      # u16s LITTLE-endian even on GC
        ptr, unk = struct.unpack_from(">2I", b, o + 8)
        out.append(dict(i=i, hdr=o, fmt=fmt, w=w, h=ht, mips=mips, ptr=ptr, unkC=unk))
        o += 0x10
        o = (o + 0x1F) & ~0x1F
    return out, b

def rgb565(c):
    return ((c >> 11) & 31) * 255 // 31, ((c >> 5) & 63) * 255 // 63, (c & 31) * 255 // 31

def gc_cmpr(b, off, w, h):
    img = Image.new("RGBA", (w, h)); px = img.load(); p = off
    for by in range(0, h, 8):
        for bx in range(0, w, 8):
            for sb in range(4):
                sx, sy = bx + (sb & 1) * 4, by + (sb >> 1) * 4
                c0, c1 = struct.unpack_from(">HH", b, p); bits = struct.unpack_from(">I", b, p + 4)[0]; p += 8
                a, bb = rgb565(c0), rgb565(c1)
                if c0 > c1:
                    pal = [a + (255,), bb + (255,),
                           tuple((2 * a[k] + bb[k]) // 3 for k in range(3)) + (255,),
                           tuple((a[k] + 2 * bb[k]) // 3 for k in range(3)) + (255,)]
                else:
                    pal = [a + (255,), bb + (255,), tuple((a[k] + bb[k]) // 2 for k in range(3)) + (255,), (0, 0, 0, 0)]
                for y in range(4):
                    for x in range(4):
                        idx = (bits >> (30 - 2 * (y * 4 + x))) & 3
                        if sx + x < w and sy + y < h: px[sx + x, sy + y] = pal[idx]
    return img

def gc_rgba8(b, off, w, h):
    img = Image.new("RGBA", (w, h)); px = img.load(); p = off
    for by in range(0, h, 4):
        for bx in range(0, w, 4):
            ar = b[p:p + 32]; gb = b[p + 32:p + 64]; p += 64
            for k in range(16):
                x, y = bx + (k & 3), by + (k >> 2)
                if x < w and y < h:
                    px[x, y] = (ar[2 * k + 1], gb[2 * k], gb[2 * k + 1], ar[2 * k])
    return img

def gc_level_size(fmt, w, h):
    bw = 8 if fmt == 4 else 4
    tw, th = -(-w // bw) * bw, -(-h // bw) * bw
    return tw * th * (4 if fmt == 4 else 32) // 8

def gc_decode(b, t):
    if t["fmt"] == 4: return gc_cmpr(b, t["ptr"], t["w"], t["h"])
    if t["fmt"] == 1: return gc_rgba8(b, t["ptr"], t["w"], t["h"])
    raise ValueError(t["fmt"])

# ---------------- beta PS2 ----------------
def beta_headers(d):
    h, b = load(d)
    o = h["texture_data_offset"]; out = []
    for i in range(h["num_textures"]):
        fmt, w, ht, mips, ptr, clut = struct.unpack_from("<4H2I", b, o)
        out.append(dict(i=i, hdr=o, fmt=fmt, w=w, h=ht, mips=mips, ptr=ptr, clut=clut))
        o += 0x10
    return out, b, h

def csm1(i):   # CSM1 storage index -> logical index (swap bits 3 and 4)
    return (i & ~0x18) | ((i & 8) << 1) | ((i & 0x10) >> 1)

def beta_palette(b, h, idx, unswizzle=True):
    o = h["palette_offset"] + idx * 0x400
    raw = [tuple(b[o + 4 * k:o + 4 * k + 4]) for k in range(256)]   # bytes R,G,B,A
    if unswizzle: raw = [raw[csm1(k)] for k in range(256)]
    return raw

def ps2a(a):  # beta CLUT alpha is already 0..255 (survey), so identity
    return a

def beta_decode(b, h, t, unswizzle=True, alpha_scale=True, level_off=0, w=None, hh=None):
    w = w or t["w"]; hh = hh or t["h"]
    img = Image.new("RGBA", (w, hh)); px = img.load(); p = t["ptr"] + level_off
    if t["fmt"] == 2:
        pal = beta_palette(b, h, t["clut"], unswizzle)
        for y in range(hh):
            for x in range(w):
                r, g, bb, a = pal[b[p + y * w + x]]
                px[x, y] = (r, g, bb, ps2a(a) if alpha_scale else a)
    elif t["fmt"] == 1:
        for y in range(hh):
            for x in range(w):
                r, g, bb, a = b[p + 4 * (y * w + x):p + 4 * (y * w + x) + 4]
                px[x, y] = (r, g, bb, ps2a(a) if alpha_scale else a)
    elif t["fmt"] == 0:
        for y in range(hh):
            for x in range(w):
                r, g, bb = b[p + 3 * (y * w + x):p + 3 * (y * w + x) + 3]
                px[x, y] = (r, g, bb, 255)
    return img

def beta_bpp(fmt): return {0: 24, 1: 32, 2: 8}[fmt]
