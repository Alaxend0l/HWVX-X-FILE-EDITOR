import sys, struct
from betalib import *
b = Beta(sys.argv[1]); B = b.B
for g in b.geoms():
    n = b.u16(g+2); lods = b.T(g+8); dist = b.T(g+0xC)
    f = struct.unpack_from('<12f', B, g+0x10)
    print(f"GEOM {g:06X} hdr={b.u16(g):04X} nLOD={n} type={b.u32(g+4)} lods={lods:X} dists={[b.p.f32(dist+4*i) for i in range(n)]}")
    print("   min", ["%.3f"%x for x in f[0:4]], "max", ["%.3f"%x for x in f[4:8]], "30:", B[g+0x30:g+0x40].hex())
    for i in range(n):
        lod = lods + 24*i
        grp = b.T(lod+8)
        print(f"  LOD{i} {lod:X}: b0={B[lod]} b1={B[lod+1]} ng={B[lod+2]} b3={B[lod+3]} 4={b.u32(lod+4)} grp={grp} C={b.u32(lod+0xC):08X} 10={B[lod+0x10:lod+0x14].hex()} 14={b.T(lod+0x14)}")
        if b.u32(g+4) != 0:
            b.p.dump(grp, 16); continue
        for j in range(B[lod+2]):
            gr = grp + 32*j
            try:
                bt = b.group_batches(gr); tris = sum(len(batch_tris(batch_verts(x))[0]) for x in bt)
                kinds = sorted(set(k for x in bt for k in x['data']))
                prims = sorted(set(hex(x['prim']) for x in bt))
            except Exception as e:
                tris = 'ERR %s' % e; kinds = prims = None
            print(f"     G{j} {gr:X}: 0={b.u16(gr):04X} flags={b.u16(gr+2):04X} col={B[gr+4:gr+8].hex()} 8={b.u32(gr+8):X} data={b.T(gr+0xC)} ntri={b.u32(gr+0x10)} 14={b.u16(gr+0x14):04X} 16={b.u16(gr+0x16):04X} tex={B[gr+0x18:gr+0x1C].hex()} rel={b.texslots.get(gr+0x18)} 1C={b.u32(gr+0x1C):08X} | tris={tris} batches={len(bt) if prims else None} prim={prims} unpacks={kinds}")
