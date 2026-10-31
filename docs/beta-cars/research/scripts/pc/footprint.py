# geometry / texture footprints for PC and GC packs; remaining "other" chunks
import struct, collections
from pclib import PC, SCR
from gclib import GC
def strlist_end(B, a):
    while True:
        e = B.index(b'\0', a)
        if e == a: return a + 1
        a = e + 1
def pc_geo_ranges(pc):
    R = []
    for g in pc.geoms():
        nl = pc.u16(g+2); R.append((g, g+0x40, 'ghdr')); R.append((pc.T(g+8), pc.T(g+8)+0x18*nl, 'lods'))
        R.append((pc.T(g+0xC), pc.T(g+0xC)+4*nl, 'dist'))
        for l in pc.lods(g):
            gr = pc.groups(l); R.append((gr[0], gr[0]+0x20*len(gr), 'groups'))
            nm = pc.T(l+0x14)
            if nm != l: R.append((nm, strlist_end(pc.B, nm), 'names'))
            if pc.isshadow(l):
                mx = 0
                for x in gr:
                    n = pc.u32(x+0x10); a = pc.T(x+0x1C); R.append((a, a+0x20*n, 'edges'))
                    for i in range(n): mx = max(mx, *struct.unpack_from('<2H', pc.B, a+0x20*i+0xC))
                R.append((pc.T(l+0xC), pc.T(l+0xC)+12*(mx+1), 'spos'))
                continue
            for x in gr:
                a = pc.T(x+0x1C); R.append((a, a+6*pc.u32(x+0x10), 'idx'))
            v = pc.vb(l); nv = v['nv']; vb = v['off']; R.append((vb, vb+0x20, 'vbhdr'))
            pp, cp, up, np_ = v['ptrs']
            R.append((pp, pp+16*nv, 'pos'))
            if v['uv']: R.append((up, up+8*nv, 'uv'))
            if v['nrm']: R.append((np_, np_+16*nv, 'nrm'))
            if v['col']: R.append((cp, cp+4*nv, 'col'))
    return R
def gc_geo_ranges(gc):
    R = []
    for g in gc.geoms():
        nl = gc.u16(g+2); R.append((g, g+0x40, 'ghdr')); R.append((gc.T(g+8), gc.T(g+8)+0x18*nl, 'lods'))
        R.append((gc.T(g+0xC), gc.T(g+0xC)+4*nl, 'dist'))
        for i in range(nl):
            l = gc.T(g+8)+0x18*i; gr = [gc.T(l+8)+0x20*k for k in range(gc.u16(l+2))]
            R.append((gr[0], gr[0]+0x20*len(gr), 'groups'))
            R.append((gc.T(l+0x14), strlist_end(gc.B, gc.T(l+0x14)), 'names'))
            if gc.isshadow(l):
                mx = 0
                for x in gr:
                    n = gc.u32(x+0x10); a = gc.T(x+0x1C); R.append((a, a+0x20*n, 'edges'))
                    for j in range(n): mx = max(mx, *struct.unpack_from('>2H', gc.B, a+0x20*j+0xC))
                R.append((gc.T(l+0xC), gc.T(l+0xC)+12*(mx+1), 'spos'))
                continue
            for x in gr:
                a = gc.T(x+0x1C); R.append((a, a+gc.u32(x+0x10), 'dl'))
            vb = gc.T(l+0xC); R.append((vb, vb+gc.u32(vb+0xC), 'vb'))
            if (l+0x10) in gc.p.ptrs: R.append((gc.T(l+0x10), gc.T(l+0x10)+0x10, 'skin?'))
    return R
def tex_ranges(B, h, e, stride, sizefn):
    R = []; o = h['texture_data_offset']
    for i in range(h['num_textures']):
        a = o + stride*i; R.append((a, a+stride, 'texhdr'))
        fmt, w, hh, m = struct.unpack_from('<4H', B, a); ptr = struct.unpack_from(e+'I', B, a+8)[0]
        R.append((ptr, ptr+sizefn(fmt, w, hh, m), 'pixels'))
    return R
def others(B, R, align):
    cov = bytearray(len(B))
    for a, b, _ in R:
        for i in range(a, min(b, len(B))): cov[i] = 1
    # treat padding (zero bytes up to alignment after a covered range) as covered
    chunks = []; i = 0
    while i < len(B):
        if cov[i]: i += 1; continue
        j = i
        while j < len(B) and not cov[j]: j += 1
        chunks.append((i, j)); i = j
    return chunks, cov
