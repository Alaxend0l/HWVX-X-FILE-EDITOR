# PC -> GC direction: rebuild GC VB arrays (unique-in-order of the PC unified arrays, bitwise) and the
# GC display lists from PC index lists; compare with the real GC bytes.
from gc2pc_geo import *
def ub(v): return struct.pack('<%df' % len(v), *v)  # bitwise key
st = collections.Counter()
for n in sorted(os.path.basename(x) for x in glob.glob(SCR+'/pc_cars/*car*')):
    pc = PC(SCR+'/pc_cars/'+n); gc = GC(SCR+'/gc_cars/'+n)
    for p, g in pairs(pc, gc):
        if g is None or gc.u16(g+4): continue
        for i, pl in enumerate(pc.lods(p)):
            if pc.isshadow(pl): continue
            gl = gc.T(g+8)+0x18*i
            v = pc.vb(pl)
            def uniq(seq):
                d = {}; out = []
                for x in seq:
                    if x not in d: d[x] = len(out); out.append(x)
                return d, out
            dp, P = uniq([ub(x) for x in v['pos']]); du, U = uniq([ub(x) for x in v['uv']])
            dn, N = uniq([ub(x) for x in v['nrm']]) if v['nrm'] else ({}, [])
            dc, C = uniq([bytes(x) for x in v['col']]) if v['col'] else ({}, [])
            vbb = b''.join(bytes(reversed(x[k:k+4])) for x in P for k in (0, 4, 8)) + b''.join(bytes(reversed(x[k:k+4])) for x in U for k in (0, 4)) + b''.join(C) + b''.join(bytes(reversed(x[k:k+4])) for x in N for k in (0, 4, 8))
            uvO = 12*len(P); cO = uvO + 8*len(U) if C else 0; nO = (uvO + 8*len(U) + 4*len(C)) if N else 0
            hdr = struct.pack('>4I', uvO, cO, nO, 0x10 + len(vbb))
            gvb = gc.T(gl+0xC)
            st['VB bytes identical', gc.B[gvb:gvb+0x10+len(vbb)] == hdr + vbb] += 1
            pg = pc.groups(pl); gg = [gc.T(gl+8)+0x20*k for k in range(gc.u16(gl+2))]
            if len(pg) != len(gg): continue
            for a, b in zip(pg, gg):
                f = pc.u16(a+2); il = pc.idx(a)
                lit = (f & 0x16) and not (f & 0x81)
                body = b''.join(struct.pack('>3H', dp[ub(v['pos'][j])], dn[ub(v['nrm'][j])] if lit else dc[bytes(v['col'][j])], du[ub(v['uv'][j])]) for j in il)
                dl = bytes([0x90]) + struct.pack('>H', len(il)) + body
                dl += bytes(al(len(dl), 32) - len(dl))
                st['DL bytes identical', gc.B[gc.T(b+0x1C):gc.T(b+0x1C)+gc.u32(b+0x10)] == dl] += 1
for k, c in sorted(st.items(), key=str): print(c, k)
