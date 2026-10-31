import glob, struct, os, collections, sys
from betalib import *
from gclib import GC, prim_tris
R = lambda t, n=4: tuple(round(x, n) for x in t)
def canon(t): return min(t[i:]+t[:i] for i in range(3))
bb = {}; gg = {}
for d in sorted(glob.glob('../beta_cars/*_*')):
    b = Beta(d); bb[os.path.basename(d)] = b
for d in sorted(glob.glob('../gc_cars/*_*')):
    gg[os.path.basename(d)] = GC(d)
gidx = []
for name, c in gg.items():
    for g in c.geoms():
        gidx.append((struct.unpack_from('>3f', c.B, g+0x10) + struct.unpack_from('>3f', c.B, g+0x20), name, g))
st = collections.defaultdict(collections.Counter); bad = []
for bname, b in bb.items():
    for bg in b.geoms():
        f = struct.unpack_from('<3f', b.B, bg+0x10) + struct.unpack_from('<3f', b.B, bg+0x20)
        cand = [x for x in gidx if max(abs(x[0][i]-f[i]) for i in range(6)) < 1e-4]
        if not cand: st['geom']['nomatch'] += 1; continue
        def score(x):
            c = gg[x[1]]; g_ = x[2]
            if c.u16(g_+2) != b.u16(bg+2) or b.u32(bg+4): return 0
            try:
                blod = b.T(bg+8); glod = c.T(g_+8)
                if c.isshadow(glod) or b.B[blod+2] != c.u16(glod+2): return 0
                vb = c.vb(glod); bgr = b.T(blod+8); ggr = c.T(glod+8)
                bt = collections.Counter(canon(tuple(R(v['pos']) for v in t)) for t in group_tris(b, bgr))
                gt = collections.Counter(canon(tuple(R(vb['pos'][v[0]]) for v in t)) for op, fm, vs in c.dl(ggr)[0] for t in prim_tris(op, vs))
                return sum((bt & gt).values()) + (1 if x[1][3:] == bname[3:] else 0) * 0.5
            except Exception: return 0
        cand.sort(key=score, reverse=True)
        _, gname, gg_ = cand[0]; c = gg[gname]
        st['hdr'][(hex(b.u16(bg)), b.u32(bg+4), hex(c.u16(gg_)), c.u16(gg_+4), c.u16(gg_+6))] += 1
        nl = b.u16(bg+2)
        st['nLOD eq'][nl == c.u16(gg_+2)] += 1
        if nl != c.u16(gg_+2): continue
        bd = b.T(bg+0xC); gd = c.T(gg_+0xC)
        st['dists eq'][all(b.p.f32(bd+4*i) == c.p.f32(gd+4*i) for i in range(nl))] += 1
        bw = struct.unpack_from('<f', b.B, bg+0x1C)[0]; gw = struct.unpack_from('>ff', c.B, gg_+0x1C)
        st['min.w eq'][abs(bw-gw[0]) < 1e-5] += 1
        if b.u32(bg+4) != 0: continue
        for li in range(nl):
            blod = b.T(bg+8)+24*li; glod = c.T(gg_+8)+0x18*li
            if c.isshadow(glod):
                st['shadow lod'][(B0 := b.B[blod], b.B[blod+2])] += 1; continue
            st['lod b0'][(b.B[blod], b.B[blod+1], b.B[blod+3], b.B[blod+0x10:blod+0x14].hex())] += 1
            if b.B[blod+2] != c.u16(glod+2): st['ng mismatch'][1] += 1; continue
            vb = c.vb(glod)
            for j in range(b.B[blod+2]):
                bgr = b.T(blod+8)+32*j; ggr = c.T(glod+8)+32*j
                bf = b.u16(bgr+2); gf = c.u16(ggr+2)
                st['flags'][(hex(bf), hex(gf))] += 1
                tn = b.B[b.T(blod+0x14):b.T(blod+0x14)+24].split(b'\0')[0].decode('latin1')
                if (bf & 0xFF) != (gf & 0xFF): st['flag change detail'][(hex(bf), hex(gf), b.u16(bgr+0x14), c.u16(ggr+0x14), b.B[bgr+4:bgr+8].hex(), c.B[ggr+4:ggr+8].hex(), tn[:14])] += 1
                st['col eq'][b.B[bgr+4:bgr+8] == c.B[ggr+4:ggr+8]] += 1
                if b.B[bgr+4:bgr+8] != c.B[ggr+4:ggr+8]: st['col diff'][(hex(bf), hex(gf), b.B[bgr+4:bgr+8].hex(), c.B[ggr+4:ggr+8].hex())] += 1
                if b.u16(bgr+0x14) != c.u16(ggr+0x14): st['0x14 diff'][(hex(bf), hex(gf), b.u16(bgr+0x14), c.u16(ggr+0x14))] += 1
                st['0x14,0x16 eq'][(b.u16(bgr+0x14) == c.u16(ggr+0x14), b.u16(bgr+0x16) == c.u16(ggr+0x16))] += 1
                if b.u16(bgr+0x16) != c.u16(ggr+0x16): st['0x16 diff'][(hex(bf), b.u16(bgr+0x16), c.u16(ggr+0x16))] += 1
                st['grp0/8'][(b.u16(bgr), b.u32(bgr+8), c.u16(ggr), c.u32(ggr+8))] += 1
                bt = b.texslots.get(bgr+0x18); gt = c.texslots.get(ggr+0x18)
                st['tex'][(hex(bf), 'beta has' if bt is not None else 'beta none', 'gc has' if gt is not None else 'gc none', 'shared' if (bt or 0) >> 16 else '')] += 1
                try: btris = group_tris(b, bgr)
                except Exception as e: st['beta err'][str(e)] += 1; continue
                st['beta tricount==0x10'][len(btris) == b.u32(bgr+0x10)] += 1
                sk = c.u16(gg_+4) != 0
                prims, pad = c.dl(ggr, sk)
                gtr = [t for op, fm, vs in prims for t in prim_tris(op, vs)]
                bset = collections.Counter(canon(tuple((R(v['pos']), R(v['uv'])) for v in t)) for t in btris)
                gset = collections.Counter(canon(tuple((R(vb['pos'][v[0]]), R(vb['uv'][v[2]])) for v in t)) for t in gtr)
                ok = bset == gset
                bpo = collections.Counter(canon(tuple(R(v['pos']) for v in t)) for t in btris)
                gpo = collections.Counter(canon(tuple(R(vb['pos'][v[0]]) for v in t)) for t in gtr)
                def fr(u): return tuple(round(x - __import__('math').floor(x + 1e-4), 3) % 1.0 for x in u)
                bfr = collections.Counter(canon(tuple((R(v['pos']), fr(v['uv'])) for v in t)) for t in btris)
                gfr = collections.Counter(canon(tuple((R(vb['pos'][v[0]]), fr(vb['uv'][v[2]])) for v in t)) for t in gtr)
                cat = 'exact' if ok else ('uv equal mod 1' if bfr == gfr else ('positions equal, uv differ' if bpo == gpo else ('same tri count' if len(btris) == len(gtr) else 'different mesh')))
                st['tri sets'][cat] += 1
                st['tris by category'][cat] += len(btris)
                if cat in ('different mesh', 'same tri count'): st['diff examples'][(bname, hex(bg), gname, hex(gg_), li, j, len(btris), len(gtr))] += 1
                if cat in ('positions equal, uv differ',):
                    bad.append((bname, hex(bg), gname, hex(gg_), li, j, hex(bf), hex(gf), len(btris), len(gtr), sum((bset & gset).values())))
                # order
                border = [canon(tuple((R(v['pos']), R(v['uv'])) for v in t)) for t in btris]
                gorder = [canon(tuple((R(vb['pos'][v[0]]), R(vb['uv'][v[2]])) for v in t)) for t in gtr]
                st['tri order equal'][border == gorder] += 1
                kinds = tuple(sorted(set(k for x in b.group_batches(bgr) for k in x['data'])))
                st['unpacks by flags'][(hex(bf), hex(gf), kinds, tuple(sorted(set(hex(x['prim']) for x in b.group_batches(bgr)))))] += 1
                if (3, 'V4-8') in kinds:
                    bcols = collections.Counter(bytes(v[(3, 'V4-8')]).hex() for t in btris for v in t)
                    gcols = collections.Counter(vb['col'][v[1]].hex() for _, _, vs in prims for v in vs) if not (gf & 6) else None
                    st['rgba beta->gc'][(tuple(bcols)[:3], tuple(gcols)[:3] if gcols else None)] += 1
                if GC.attrs_for(gf)[1] == 'nrm' and btris and btris[0][0]['nrm'] is not None:
                    bn = set((R(v['pos']), R(v['nrm'], 3)) for t in btris for v in t)
                    gn = set((R(vb['pos'][v[0]]), R(vb['nrm'][v[1]], 3)) for _, _, vs in prims for v in vs)
                    st['normals equal'][bn == gn] += 1
for k, v in st.items(): print(k, dict(v.most_common(30)))
print('BAD', len(bad)); [print(x) for x in bad[:30]]
