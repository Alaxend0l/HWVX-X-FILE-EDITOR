import sys, glob, collections
from betalib import *
def sub(a,b): return [a[i]-b[i] for i in range(3)]
def cross(a,b): return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
def dot(a,b): return sum(a[i]*b[i] for i in range(3))
st = collections.Counter()
for d in sys.argv[1:]:
    b = Beta(d)
    for g in b.geoms():
        lods = b.T(g+8)
        for i in range(b.u16(g+2)):
            lod = lods + 24*i
            ng = b.B[lod+2]; grp = b.T(lod+8)
            for j in range(ng):
                gr = grp + 32*j
                try: tris = group_tris(b, gr)
                except Exception as e: st['err'] += 1; continue
                st['cnt match', len(tris) == b.u32(gr+0x10)] += 1
                for t in tris:
                    if t[0]['nrm'] is None: st['nonrm'] += 1; continue
                    n = cross(sub(t[1]['pos'], t[0]['pos']), sub(t[2]['pos'], t[0]['pos']))
                    vn = [t[0]['nrm'][k]+t[1]['nrm'][k]+t[2]['nrm'][k] for k in range(3)]
                    dp = dot(n, vn)
                    st['ccw' if dp > 0 else ('cw' if dp < 0 else 'deg')] += 1
print(st)
