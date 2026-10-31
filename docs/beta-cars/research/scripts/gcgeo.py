import sys, struct
sys.path.insert(0, '..')
from xp import P
d = sys.argv[1]
p = P(d, '>')
B = p.blob
for a,v in p.refr:
    if not (v>>16): p.ptrs.add(a)
def u16(o): return struct.unpack_from('>H', B, o)[0]
def T(o): return p.u32(o) if o in p.ptrs else None
texslots = {a: v for a, v in p.tex}
def isgeom(g):
    n = u16(g+2); lods = T(g+8)
    if n == 0 or n > 8 or lods is None or T(g+0xC) is None: return False
    for i in range(n):
        lod = lods + i*0x18; ng = u16(lod+2); grp = T(lod+8)
        if ng == 0 or ng > 0x400 or grp is None or T(lod+0xC) is None: return False
        for j in range(ng):
            g2 = grp+j*0x20
            if T(g2+0x1C) is None or p.u32(g2+0x10) == 0: return False
    return True
def isshadow(g):
    grps = T(g+8); return grps is not None and (u16(grps+2) & 0x4000)
geoms = sorted(set(g for g in range(0, len(B)-0x40, 0x10) if (g+8) in p.ptrs and isgeom(g)))
# drop LOD levels that look like geometries (those that are inside another geometry's lod array)
lodset = set()
for g in geoms:
    for i in range(u16(g+2)): lodset.add(T(g+8)+i*0x18)
geoms = [g for g in geoms if g not in lodset]
verbose = len(sys.argv) > 2
for g in geoms:
    n = u16(g+2); lods = T(g+8); dist = T(g+0xC)
    f = struct.unpack_from('>12f', B, g+0x10)
    print(f"GEOM {g:06X} unk0={u16(g):04X} nLOD={n} unk4={u16(g+4):04X} unk6={u16(g+6):04X} lods={lods:X} dist={dist:X} "
          f"dists={[p.f32(dist+4*i) for i in range(n)]}")
    print("   min", ["%.3f"%x for x in f[0:4]], "max", ["%.3f"%x for x in f[4:8]], "ctr", ["%.3f"%x for x in f[8:12]])
    for i in range(n):
        lod = lods+i*0x18
        if isshadow(lod):
            print(f"  LOD{i} SHADOW {lod:X}: {u16(lod):04X} ng={u16(lod+2)} {u16(lod+4):04X} {u16(lod+6):04X} grp={T(lod+8)} verts={T(lod+0xC)} 10={p.u32(lod+0x10):X} 14={p.u32(lod+0x14):X}")
            continue
        vb = T(lod+0xC); grp = T(lod+8)
        vbh = [p.u32(vb+4*k) for k in range(4)]
        print(f"  LOD{i} {lod:X}: unk0={u16(lod):04X} ng={u16(lod+2)} vtxFlags={u16(lod+4):04X} unk6={u16(lod+6):04X} grp={grp:X} vb={vb:X} 10={p.u32(lod+0x10):X}{'P' if lod+0x10 in p.ptrs else ''} 14={p.u32(lod+0x14):X}{'P' if lod+0x14 in p.ptrs else ''}")
        npos = vbh[0]//12
        print(f"     VB uvOff={vbh[0]:X} clrOff={vbh[1]:X} nrmOff={vbh[2]:X} size={vbh[3]:X}  npos={npos} nuv={((vbh[1] or vbh[2] or vbh[3]-0x10)-vbh[0])//8}")
        for j in range(u16(lod+2)):
            g2 = grp+j*0x20
            uv = T(g2+0xC)
            t18 = texslots.get(g2+0x18); t1a = texslots.get(g2+0x1A)
            ts = [x for x in (texslots.get(g2+0x18), texslots.get(g2+0x16), texslots.get(g2+0x1A), texslots.get(g2+0x14))]
            print(f"     G{j} {g2:X}: unk0={u16(g2):04X} flags={u16(g2+2):04X} col={B[g2+4:g2+8].hex()} 8={p.u32(g2+8):X} uv={uv} dl={p.u32(g2+0x1C):X} dlSize={p.u32(g2+0x10):X} 14={u16(g2+0x14):04X} 16={u16(g2+0x16):04X} 18={u16(g2+0x18):04X} 1A={u16(g2+0x1A):04X} texrel={ts}")
            if uv is not None: print("        uvscroll", p.f32(uv), p.f32(uv+4), B[uv+8:uv+12].hex(), hex(p.u32(uv+12)))
