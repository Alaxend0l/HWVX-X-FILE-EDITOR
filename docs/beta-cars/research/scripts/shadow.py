import sys, struct, collections, math
from betalib import *
from gclib import GC
b = Beta(sys.argv[1]); c = GC(sys.argv[2]); bg = int(sys.argv[3], 16); gg = int(sys.argv[4], 16)
R = lambda t, n=4: tuple(round(x, n) for x in t)
blod = b.T(bg+8); glod = c.T(gg+8)
verts = c.T(glod+0xC)
ng = c.u16(glod+2)
# vertex count: up to next object; derive from max index
def sub(a,b): return [a[i]-b[i] for i in range(3)]
def cross(a,b): return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
def norm(v): l = math.sqrt(sum(x*x for x in v)); return [x/l for x in v] if l else v
maxv = 0; st = collections.Counter()
for j in range(ng):
    gr = c.T(glod+8) + 32*j; ne = c.u32(gr+0x10); e = c.T(gr+0x1C)
    edges = []
    for k in range(ne):
        o = e + 32*k
        n0 = struct.unpack_from('>3f', c.B, o); v0, v1 = struct.unpack_from('>2H', c.B, o+12); n1 = struct.unpack_from('>3f', c.B, o+16); ty = struct.unpack_from('>i', c.B, o+28)[0]
        p0 = struct.unpack_from('>3f', c.B, verts+12*v0); p1 = struct.unpack_from('>3f', c.B, verts+12*v1)
        edges.append((R(p0), R(p1), R(n0, 3), R(n1, 3), ty, struct.unpack_from('>I', c.B, o+12+0)[0], c.B[o+0xC:o+0x10].hex(), c.B[o+0x1C:o+0x20].hex()))
        maxv = max(maxv, v0, v1)
    btr = group_tris(b, b.T(blod+8) + 32*j)
    # build beta edges
    emap = collections.defaultdict(list)
    for t in btr:
        p = [R(v['pos']) for v in t]
        fn = R(norm(cross(sub(p[1], p[0]), sub(p[2], p[0]))), 3)
        for a_, b_ in ((p[0], p[1]), (p[1], p[2]), (p[2], p[0])):
            emap[(a_, b_)].append(fn)
    und = collections.defaultdict(list)
    for (a_, b_), fns in emap.items():
        und[tuple(sorted((a_, b_)))].append(((a_, b_), fns))
    shared = {k: v for k, v in und.items() if len(v) == 2}
    if j < 3:
        print(f'G{j}: gc edges={ne} beta tris={len(btr)} beta undirected edges={len(und)} shared(2 dirs)={len(shared)} boundary={sum(1 for v in und.values() if len(v)==1)}')
        for ed in edges[:4]: print('   gc', ed)
    for ed in edges:
        key = tuple(sorted((ed[0], ed[1])))
        if key in und:
            dirs = und[key]
            st['edge found', len(dirs)] += 1
            d = dict(dirs)
            # which orientation is (p0,p1)?
            f01 = d.get((ed[0], ed[1])); f10 = d.get((ed[1], ed[0]))
            allf = [f for _, fs in dirs for f in fs]
            def has(lst, n, sgn): return bool(lst) and any(max(abs(sgn*a-b) for a, b in zip(f, n)) < 2e-3 for f in lst)
            for nm, n in (('n0', ed[2]), ('n1', ed[3])):
                for sgn in (1, -1):
                    st[(nm, 'sign', sgn, 'p0->p1 face', has(f01, n, sgn), 'p1->p0 face', has(f10, n, sgn))] += 1
            # coplanarity of omitted edges
            st['n0 = face with p0->p1', bool(f01) and any(max(abs(a-b) for a, b in zip(f, ed[2])) < 2e-3 for f in f01)] += 1
            st['n1 = face with p1->p0', bool(f10) and any(max(abs(a-b) for a, b in zip(f, ed[3])) < 2e-3 for f in f10)] += 1
        else:
            st['edge not in beta'] += 1
    st['count == shared edges'][0:0] if False else None
    st['gc edges == beta shared edges', ne == len(shared)] += 1
    gkeys = set(tuple(sorted((ed[0], ed[1]))) for ed in edges)
    for k, v in shared.items():
        fa = v[0][1][0]; fb = v[1][1][0]
        cop = max(abs(a-b) for a, b in zip(fa, fb)) < 2e-3
        st[('beta shared edge', 'coplanar' if cop else 'crease', 'in gc', k in gkeys)] += 1
print('max vertex index', maxv, 'verts array ends?', hex(verts), hex(verts + 12*(maxv+1)))
for k, v in st.items(): print(k, v)
