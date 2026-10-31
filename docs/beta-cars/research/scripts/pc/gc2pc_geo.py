"""Reference GC (v183 BE) -> PC (v183 LE) geometry builder + byte comparison against real PC packs.

build(gc, g, base, hidden=None) -> (bytes, pointer_slots, texrelocs)
  hidden: optional callback(lodIndex, groupIndex, vertexPosInGroup) -> (nrm or None, col or None) used only for
  attributes that GC does not carry (normals of alpha/unlit vertices, colours of lit/env vertices).
  Default: normal (0,0,0) [any value: lighting is off for those groups], colour FF FF FF FF.
"""
import struct, sys, os, glob, collections
from pclib import PC, SCR
from gclib import GC
CD = b'\xcd\xcd\xcd\xcd'
def al(x, a=16): return (x + a - 1) & ~(a - 1)
def gattrs(f):
    if f & 0x81: return 'alpha'
    if f & 2: return 'env'
    if f & 0x14: return 'lit'
    return 'unlit'
def strlist(B, a):
    e = a
    while True:
        z = B.index(b'\0', e)
        if z == e: return B[a:z+1]
        e = z + 1
def build(gc, g, base, hidden=None):
    out = bytearray(); ptrs = []; texr = []
    def put(off, data):
        need = off - base + len(data)
        if len(out) < need: out.extend(bytes(need - len(out)))
        out[off-base:off-base+len(data)] = data
    def ptr(slot, target): put(slot, struct.pack('<I', target)); ptrs.append(slot)
    B = gc.B; u16 = gc.u16; u32 = gc.u32
    nl = u16(g+2); shadow = gc.isshadow(gc.T(g+8))
    # header
    hdr = struct.pack('<HHHH', 2, nl, u16(g+4), u16(g+6))
    put(base, hdr + bytes(8) + bytes(reversed(B[g+0x10:g+0x14])) )
    for o in range(0x10, 0x40, 4): put(base+o, struct.pack('<I', u32(g+o)))
    lodarr = base + 0x40; ptr(base+8, lodarr)
    cur = al(lodarr + 0x18*nl)
    for i in range(nl):
        gl = gc.T(g+8) + 0x18*i; pl = lodarr + 0x18*i
        ng = u16(gl+2); ggr = [gc.T(gl+8) + 0x20*k for k in range(ng)]
        put(pl, struct.pack('<HHHH', u16(gl), ng, u16(gl+4), u16(gl+6)))
        put(pl+0x10, struct.pack('<I', 0))
        grp = cur; ptr(pl+8, grp); cur = al(grp + 0x20*ng)
        if shadow:
            # positions: same array as GC (f32[3] packed), then edges per group
            mx = 0
            for x in ggr:
                for j in range(u32(x+0x10)):
                    mx = max(mx, *struct.unpack_from('>2H', B, gc.T(x+0x1C)+0x20*j+0xC))
            sp = cur; ptr(pl+0xC, sp)
            gp = gc.T(gl+0xC)
            put(sp, b''.join(struct.pack('<3f', *struct.unpack_from('>3f', B, gp+12*j)) for j in range(mx+1)))
            cur = al(sp + 12*(mx+1))
            ptr(pl+0x14, pl)   # PC points the (empty) name list at the LOD itself
            for k, x in enumerate(ggr):
                a = grp + 0x20*k; n = u32(x+0x10)
                put(a, struct.pack('<HH4sIIIIII', 0, u16(x+2), bytes(4), 0, 0, n, 0, 0, 0)[:0x1C])
                ea = cur; ptr(a+0x1C, ea)
                ed = bytearray()
                for j in range(n):
                    e = gc.T(x+0x1C) + 0x20*j
                    n0 = struct.unpack_from('>3f', B, e); v = struct.unpack_from('>2H', B, e+0xC); n1 = struct.unpack_from('>3f', B, e+0x10)
                    ed += struct.pack('<3f2H3fi', *n0, *v, *n1, 0)
                put(ea, bytes(ed)); cur = al(ea + len(ed))
            continue
        vb = gc.vb(gl)
        # unified vertices in first-appearance order of the GC key
        keyidx = {}; verts = []; idxlists = []
        hasc = bool(vb['col']) ; hasn = bool(vb['nrm'])
        for gk, x in enumerate(ggr):
            f = u16(x+2); cls = gattrs(f)
            prims, _ = gc.dl(x)
            il = []; pos_ = 0
            for op, fmt, vs in prims:
                assert op == 0x90
                for A, Bi, C in vs:
                    # attributes GC does not carry: normal of alpha/unlit vertices, colour of lit/env vertices
                    hn, hc = hidden(i, gk, pos_) if hidden else (None, None)
                    pos_ += 1
                    if cls in ('lit', 'env'):
                        v = dict(pos=vb['pos'][A], uv=vb['uv'][C], nrm=vb['nrm'][Bi], col=(hc or b'\xff\xff\xff\xff') if hasc else None)
                    else:
                        v = dict(pos=vb['pos'][A], uv=vb['uv'][C], col=bytes(vb['col'][Bi]), nrm=(hn or (0.0, 0.0, 0.0)) if hasn else None)
                    k = (v['pos'], v['uv'], v['nrm'], v['col'])   # PC de-duplicates on the full vertex
                    if k not in keyidx:
                        keyidx[k] = len(verts); verts.append(v)
                    il.append(keyidx[k])
            idxlists.append(il)
        for k, x in enumerate(ggr):
            a = grp + 0x20*k; f = u16(x+2)
            put(a, struct.pack('<HH', 0, f) + B[x+4:x+8] + bytes(8))
            put(a+0x10, struct.pack('<IHH', len(idxlists[k])//3, u16(x+0x14), u16(x+0x16)))
            if (x+0x18) in gc.texslots:
                put(a+0x18, struct.pack('<I', 0)); texr.append((a+0x18, gc.texslots[x+0x18]))
            else:
                put(a+0x18, struct.pack('<HH', u16(x+0x18), u16(x+0x1A)))
            ia = cur; ptr(a+0x1C, ia)
            put(ia, struct.pack(f'<{len(idxlists[k])}H', *idxlists[k])); cur = al(ia + 2*len(idxlists[k]))
        nv = len(verts); vh = cur; ptr(pl+0xC, vh)
        pp = vh + 0x20; up = pp + 16*nv; npp = al(up + 8*nv) if hasn else None
        cp = (al(npp + 16*nv) if hasn else al(up + 8*nv)) if hasc else None
        ptr(vh, pp); ptr(vh+8, up)
        if hasc: ptr(vh+4, cp)
        else: put(vh+4, bytes(4))
        if hasn: ptr(vh+0xC, npp)
        else: put(vh+0xC, bytes(4))
        put(vh+0x10, bytes(16))
        put(pp, b''.join(struct.pack('<3f', *v['pos']) + CD for v in verts))
        put(up, b''.join(struct.pack('<2f', *v['uv']) for v in verts))
        end = up + 8*nv
        if hasn:
            put(npp, b''.join(struct.pack('<3f', *(v['nrm'] or (0.0, 0.0, 0.0))) + CD for v in verts)); end = npp + 16*nv
        if hasc:
            put(cp, b''.join((v['col'] or b'\xff\xff\xff\xff') for v in verts)); end = cp + 4*nv
        names = strlist(B, gc.T(gl+0x14))
        if names == b'\0':
            ptr(pl+0x14, pl); cur = al(end)      # empty list: PC points at the LOD itself (its first byte is 0)
        else:
            nm = al(end); ptr(pl+0x14, nm); put(nm, names[:-1]); cur = al(nm + len(names) - 1)   # PC: no room reserved for the final NUL (next object supplies a zero)
    dist = cur; ptr(base+0xC, dist)
    put(dist, b''.join(struct.pack('<f', gc.p.f32(gc.T(g+0xC)+4*i)) for i in range(nl)))
    end = al(dist + 4*nl)
    if len(out) < end - base: out.extend(bytes(end - base - len(out)))
    return bytes(out), ptrs, texr
def gbbox(gc, g): return struct.unpack_from('>8f', gc.B, g+0x10)
def pbbox(pc, g): return struct.unpack_from('<8f', pc.B, g+0x10)
def pairs(pc, gc):
    """pair geometries: same index in pack order when the bboxes agree, else first unused equal bbox"""
    P = pc.geoms(); G = gc.geoms(); used = set(); out = []
    for i, p in enumerate(P):
        if i < len(G) and gbbox(gc, G[i]) == pbbox(pc, p) and G[i] not in used:
            used.add(G[i]); out.append((p, G[i])); continue
        c = [g for g in G if g not in used and gbbox(gc, g) == pbbox(pc, p)]
        if c: used.add(c[0]); out.append((p, c[0]))
        else: out.append((p, None))
    return out
def oracle(pc, p):
    """hidden-attribute oracle taken from the real PC geometry (validation only)"""
    lods = pc.lods(p); cache = {}
    def h(i, gk, n):
        if i not in cache: cache[i] = (pc.vb(lods[i]), [pc.idx(x) for x in pc.groups(lods[i])])
        v, il = cache[i]
        if gk >= len(il) or n >= len(il[gk]): return None, None
        j = il[gk][n]
        return (v['nrm'][j] if v['nrm'] else None), (bytes(v['col'][j]) if v['col'] else None)
    return h
if __name__ == '__main__':
    use_oracle = '--oracle' in sys.argv
    names = [a for a in sys.argv[1:] if not a.startswith('--')] or sorted(os.path.basename(x) for x in glob.glob(SCR+'/pc_cars/*car*'))
    tot = collections.Counter()
    for n in names:
        pc = PC(SCR+'/pc_cars/'+n); gc = GC(SCR+'/gc_cars/'+n)
        st = collections.Counter()
        for p, g in pairs(pc, gc):
            if g is None: st['no GC pair'] += 1; continue
            if gc.u16(g+4): st['skinned (skipped)'] += 1; continue
            out, ptrs, texr = build(gc, g, p, oracle(pc, p) if use_oracle else None)
            real = bytearray(pc.B[p:p+len(out)])
            for slot, _ in texr: real[slot-p:slot-p+4] = bytes(4)   # pre-reloc junk in texture slots: don't care
            real = bytes(real)
            st['texrelocs match', sorted(texr) == sorted((a, v) for a, v in pc.texslots.items() if p <= a < p+len(out))] += 1
            realptrs = sorted(x for x in pc.p.ptrs if p <= x < p+len(out))
            if out == real and sorted(ptrs) == realptrs: st['geometry byte-identical'] += 1
            else:
                st['geometry differs'] += 1
                nd = sum(1 for a, b in zip(out, real) if a != b)
                st['  differing bytes'] += nd; st['  total bytes'] += len(out)
                if sorted(ptrs) != realptrs: st['  pointer set differs'] += 1
                if len(sys.argv) > 1 and '--v' in sys.argv:
                    d = [i for i in range(len(out)) if out[i] != real[i]]
                    print('   ', hex(p), len(d), [hex(p+x) for x in d[:8]])
                if len(out) != len(real) or pc.B[p+len(out):p+len(out)+4] and (p+len(out)) < len(pc.B) and not pc.isgeom(p+len(out)) and False: pass
            st['bytes'] += len(out)
        print(n, dict(st)); tot += st
    print('TOTAL', dict(tot))
