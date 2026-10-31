import glob, struct, collections, math
from gclib import GC
st = collections.defaultdict(collections.Counter); ex = []
for d in sorted(glob.glob('../gc_cars/*_*')):
    c = GC(d)
    for g in c.geoms():
        f = struct.unpack_from('>12f', c.B, g+0x10)
        mn, mx, ctr = f[0:3], f[4:7], f[8:11]
        lods = c.T(g+8); pts = []; pts0 = []
        for i in range(c.u16(g+2)):
            lod = lods + 0x18*i
            if c.isshadow(lod):
                vs = c.T(lod+0xC); continue
            p = c.vb(lod)['pos']; pts += p
            if i == 0: pts0 = p
        if not pts: st['shadow-only'][1] += 1; continue
        amn = [min(p[k] for p in pts0) for k in range(3)]; amx = [max(p[k] for p in pts0) for k in range(3)]
        st['bbox==LOD0 aabb'][max(abs(a-b) for a, b in zip(amn + amx, mn + mx)) < 1e-3] += 1
        st['ctr==mid'][max(abs((mn[k]+mx[k])/2 - ctr[k]) for k in range(3)) < 1e-3] += 1
        r0 = max(math.sqrt(sum(x*x for x in p)) for p in pts0)
        rc = max(math.sqrt(sum((p[k]-ctr[k])**2 for k in range(3))) for p in pts0)
        st['min.w==max|v| LOD0'][abs(r0 - f[3]) < 1e-3] += 1
        st['max.w==max|v-ctr| LOD0'][abs(rc - f[7]) < 1e-3] += 1
        if abs(rc - f[7]) >= 1e-3 and len(ex) < 8: ex.append((d, hex(g), f[3], f[7], r0, rc, math.dist(mn, mx)/2, math.sqrt(sum(x*x for x in mn)), math.sqrt(sum(x*x for x in mx)) ))
        st['ctr.w'][f[11]] += 1
[print(k, dict(v)) for k, v in st.items() if k != 'ctr.w']; [print(e) for e in ex]
st2 = collections.Counter()
for d in sorted(glob.glob('../gc_cars/*_*')):
    c = GC(d)
    for g in c.geoms():
        f = struct.unpack_from('>12f', c.B, g+0x10)
        mn, mx = f[0:3], f[4:7]
        a = max(math.sqrt(sum(x*x for x in mn)), math.sqrt(sum(x*x for x in mx)))
        st2['min.w~max(|min|,|max|)', abs(a - f[3]) < 1e-3 * max(1, a)] += 1
        st2['max.w~|max-min|/2', abs(math.dist(mn, mx)/2 - f[7]) < 1e-3 * max(1, f[7])] += 1
print(st2)
