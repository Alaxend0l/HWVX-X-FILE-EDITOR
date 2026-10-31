import sys, glob, collections
from gclib import GC
C = collections.Counter
stats = collections.defaultdict(C)
for d in sorted(glob.glob(sys.argv[1] + '/*_car_*')):
    g = GC(d)
    for geo in g.geoms():
        stats['geom.unk0'][hex(g.u16(geo))] += 1
        stats['geom.unk4/6'][(g.u16(geo+4), g.u16(geo+6))] += 1
        if g.u16(geo+4): print('skinned', d, hex(geo))
        stats['nLOD'][g.u16(geo+2)] += 1
        lods = g.T(geo+8)
        for i in range(g.u16(geo+2)):
            lod = lods + i*0x18
            if g.isshadow(lod):
                stats['shadowlod'][(g.u16(lod), g.u16(lod+2), g.u16(lod+4), g.u16(lod+6), g.p.u32(lod+0x10), g.p.u32(lod+0x14))] += 1
                continue
            stats['lod.0/4/6/10'][(g.u16(lod), g.u16(lod+4), g.u16(lod+6), g.p.u32(lod+0x10))] += 1
            stats['lod.14ptr'][(lod+0x14) in g.p.ptrs] += 1
            vb = g.vb(lod)
            stats['ncol'][len(vb['col'])] += 1
            stats['vbsize_ok'][vb['hdr'][3] == 0x10 + (vb['hdr'][2] + 12*len(vb['nrm']) if vb['hdr'][2] else (vb['hdr'][1] + 4*len(vb['col'])))] += 1
            stats['vb_align'][(vb['off'] % 32, vb['hdr'][3] % 4)] += 1
            for c in vb['col']: stats['colors'][c.hex()] += 1
            grp = g.T(lod+8)
            for j in range(g.u16(lod+2)):
                gr = grp + j*0x20
                fl = g.u16(gr+2)
                stats['grp.flags'][hex(fl)] += 1
                stats['grp.unk0'][g.u16(gr)] += 1
                stats['grp.8'][g.p.u32(gr+8)] += 1
                stats['grp.14 by flags'][(hex(fl), g.u16(gr+0x14))] += 1
                stats['grp.uvscroll'][g.T(gr+0xC) is not None] += 1
                stats['grp.col by flags'][(hex(fl), g.B[gr+4:gr+8].hex())] += 1
                stats['grp.texreloc'][(gr+0x18) in g.texslots, g.u16(gr+0x1A) == 0xFFFF and g.u16(gr+0x18) == 0] += 1
                dla = g.T(gr+0x1C)
                stats['dl_align'][(dla % 32, g.p.u32(gr+0x10) % 32)] += 1
                prims, pad = g.dl(gr, g.u16(geo+4) != 0)
                if g.u16(geo+4): stats['skin mtx'][tuple(sorted(set(v[3] for _,_,vs in prims for v in vs)))] += 1
                attrs = GC.attrs_for(fl)
                for op, fmt, vs in prims:
                    stats['ops'][(hex(op), fmt)] += 1
                    for v in vs:
                        for k, a in enumerate(attrs):
                            arr = {'pos': vb['pos'], 'uv': vb['uv'], 'col': vb['col'], 'nrm': vb['nrm']}[a]
                            if v[k] >= len(arr): stats['OOB'][(hex(fl), a)] += 1
                        if attrs[1] == 'nrm': stats['nrm==pos'][v[1] == v[0]] += 1
                        if attrs[1] == 'col': stats['colidx'][v[1]] += 1
                stats['dlpad'][pad] += 1
for k, v in stats.items():
    print(k, dict(v.most_common(25)))
