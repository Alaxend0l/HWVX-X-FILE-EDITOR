# compare PC vs GC geometry of the same car (exact pairs)
import sys, struct, glob, collections, os
from pclib import PC, SCR
from gclib import GC, prim_tris
def bbox(B, g, e): return B[g+0x10:g+0x30] if e=='>' else None
def gbbox(gc, g): return struct.unpack_from('>8f', gc.B, g+0x10)
def pbbox(pc, g): return struct.unpack_from('<8f', pc.B, g+0x10)
def gattrs(f):
    if f & 0x81: return ('pos','col','uv')
    if f & 2: return ('pos','nrm','col')
    if f & 0x14: return ('pos','nrm','uv')
    return ('pos','col','uv')
def gc_tris(gc, lod, grp, vb):
    f = gc.u16(grp+2); att = gattrs(f)
    prims, pad = gc.dl(grp, skinned=False)
    out = []
    for op, fmt, vs in prims:
        for t in prim_tris(op, vs):
            tri = []
            for v in t:
                d = {}
                for a, i in zip(att, v):
                    if a == 'pos': d['pos'] = vb['pos'][i]
                    elif a == 'uv': d['uv'] = vb['uv'][i]
                    elif a == 'nrm': d['nrm'] = vb['nrm'][i]
                    elif a == 'col': d['col'] = bytes(vb['col'][i]) if i < len(vb['col']) else None
                tri.append(d)
            out.append(tri)
    return out
def pc_tris(pc, grp, vb):
    ix = pc.idx(grp); out = []
    for k in range(0, len(ix), 3):
        tri = []
        for i in ix[k:k+3]:
            d = {'pos': vb['pos'][i]}
            if vb['uv']: d['uv'] = vb['uv'][i]
            if vb['nrm']: d['nrm'] = vb['nrm'][i]
            if vb['col']: d['col'] = vb['col'][i]
            tri.append(d)
        out.append(tri)
    return out
def key(tri, attrs):
    return tuple(tuple(v.get(a) for a in attrs) for v in tri)
def canon(t):  # rotation-invariant
    r = [t[i:]+t[:i] for i in range(3)]; return min(r)
def compare_pack(pd, gd, verbose=False):
    pc = PC(pd); gc = GC(gd)
    pg = pc.geoms(); gg = gc.geoms()
    gmap = collections.defaultdict(list)
    for g in gg: gmap[gbbox(gc, g)].append(g)
    st = collections.Counter()
    for p in pg:
        cands = gmap.get(pbbox(pc, p))
        if not cands: st['geom_nomatch'] += 1; continue
        g = cands.pop(0); st['geom'] += 1
        if pc.u16(p+2) != gc.u16(g+2): st['nlod_diff'] += 1; continue
        if gc.u16(g+4): st['skinned'] += 1; continue
        for pl, gl in zip(pc.lods(p), [gc.T(g+8)+i*0x18 for i in range(gc.u16(g+2))]):
            st['lod'] += 1
            if pc.isshadow(pl): st['shadow_lod'] += 1; continue
            pgr = pc.groups(pl); ggr = [gc.T(gl+8)+0x20*i for i in range(gc.u16(gl+2))]
            if len(pgr) != len(ggr): st['ngrp_diff'] += 1; continue
            pvb = pc.vb(pl); gvb = gc.vb(gl)
            for a, b in zip(pgr, ggr):
                st['grp'] += 1
                f = gc.u16(b+2)
                if pc.u16(a+2) != f: st['flag_diff'] += 1
                if pc.B[a+4:a+8] != gc.B[b+4:b+8]: st['col_diff'] += 1
                if pc.u16(a+0x14) != gc.u16(b+0x14): st['blend_diff'] += 1
                if pc.u16(a+0x16) != gc.u16(b+0x16): st['sort_diff'] += 1
                tp = pc.texslots.get(a+0x18); tg = gc.texslots.get(b+0x18)
                if tp != tg: st['tex_diff'] += 1
                if tp is None and pc.u32(a+0x18) != 0xFFFF: st['notex_slot_not_FFFF'] += 1
                gt = gc_tris(gc, gl, b, gvb); pt = pc_tris(pc, a, pvb)
                if pc.u32(a+0x10) != len(gt): st['count_diff'] += 1
                attrs = ['pos'] + list(GC.attrs_for(f)[1:])
                attrs = [x for x in attrs if not (f & 2 and x == 'col')]
                if not (f & 0x81) and (f & 0x16): attrs = ['pos','nrm'] + ([] if f & 2 else ['uv'])
                else: attrs = ['pos','uv','col']
                kg = [key(t, attrs) for t in gt]; kp = [key(t, attrs) for t in pt]
                if kg == kp: st['tris_same_order'] += 1
                elif sorted(map(canon, kg)) == sorted(map(canon, kp)):
                    st['tris_same_set_rot' if sorted(kg) != sorted(kp) else 'tris_same_set'] += 1
                else:
                    st['tris_diff'] += 1
                    if len(gt) == len(pt):
                        bad = set()
                        for x, y in zip(gt, pt):
                            for u, v in zip(x, y):
                                for k in attrs:
                                    a1, b1 = u.get(k), v.get(k)
                                    if a1 == b1: continue
                                    if k == 'col' or a1 is None or b1 is None: bad.add(k); continue
                                    if max(abs(p-q) for p, q in zip(a1, b1)) > (1e-3 if k != 'nrm' else 1e-3): bad.add(k)
                        st['  diffattrs:' + ','.join(sorted(bad)) if bad else '  diff_within_1e-3'] += 1
                    if verbose: print('diff', hex(a), hex(b), hex(f), len(kg), len(kp), len(set(map(canon,kg))&set(map(canon,kp))))
    return st
if __name__ == '__main__':
    tot = collections.Counter()
    names = sys.argv[1:] or sorted(os.path.basename(x) for x in glob.glob(SCR+'/pc_cars/*car*'))
    for n in names:
        st = compare_pack(SCR+'/pc_cars/'+n, SCR+'/gc_cars/'+n, verbose=len(sys.argv) > 1)
        print(n, dict(st)); tot += st
    print('TOTAL', dict(tot))
