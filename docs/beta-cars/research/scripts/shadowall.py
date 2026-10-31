import glob, os, struct, collections, math, json
from betalib import *
from gclib import GC
R = lambda t, n=4: tuple(round(x, n) for x in t)
def sub(a,b): return [a[i]-b[i] for i in range(3)]
def cross(a,b): return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
def norm(v): l = math.sqrt(sum(x*x for x in v)); return [x/l for x in v] if l else v
# pairs from bboxmatch (car body = geom at beta 0x20 / gc 0x40)
pairs = [('01_car_0','01_car_0'),('03_car_2','06_car_5'),('04_car_3','04_car_3'),('07_car_6','10_car_9'),('09_car_8','03_car_2'),('12_car_11','09_car_8'),('16_car_15','16_car_15'),('31_car_30','28_car_27')]
st = collections.Counter()
for bn, gn in pairs:
    b = Beta('../beta_cars/'+bn); c = GC('../gc_cars/'+gn)
    bs = b.u32(b.u32(b.u32(4)) + 0x54); gs = c.u32(c.u32(c.u32(4)) + 0x54)
    blod = b.T(bs+8); glod = c.T(gs+8)
    st['gc is shadow', bool(c.isshadow(glod))] += 1
    st['beta flags', tuple(sorted(set(hex(b.u16(b.T(blod+8)+32*j+2)) for j in range(b.B[blod+2]))))] += 1
    st['ng equal', b.B[blod+2] == c.u16(glod+2)] += 1
    st['beta nLOD', b.u16(bs+2)] += 1
    verts = c.T(glod+0xC)
    tot = 0; creases = 0; allshared = 0; bnd = 0; maxv = 0
    for j in range(c.u16(glod+2)):
        gr = c.T(glod+8)+32*j; ne = c.u32(gr+0x10); tot += ne
        e = c.T(gr+0x1C)
        for k in range(ne): maxv = max(maxv, *struct.unpack_from('>2H', c.B, e+32*k+12))
        tr = group_tris(b, b.T(blod+8)+32*j)
        und = collections.defaultdict(list)
        for t in tr:
            p = [R(v['pos']) for v in t]; fn = norm(cross(sub(p[1], p[0]), sub(p[2], p[0])))
            for a_, b_ in ((p[0], p[1]), (p[1], p[2]), (p[2], p[0])): und[tuple(sorted((a_, b_)))].append(fn)
        for k, fns in und.items():
            if len(fns) == 2:
                allshared += 1
                if sum(x*y for x, y in zip(*fns)) < 0.99999: creases += 1
            elif len(fns) == 1: bnd += 1
            else: st['nonmanifold edge'] += 1
    nvb = len(set(R(v['pos']) for j in range(b.B[blod+2]) for t in group_tris(b, b.T(blod+8)+32*j) for v in t))
    print(bn, gn, 'gc edges', tot, 'beta creases(dot<0.99999)', creases, 'all shared', allshared, 'boundary', bnd, 'gc nverts', maxv+1, 'beta uniq pos', nvb)
print(st)
