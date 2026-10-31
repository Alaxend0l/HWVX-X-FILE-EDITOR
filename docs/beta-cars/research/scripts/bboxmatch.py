import glob, struct, os
from betalib import Beta
from gclib import GC
bb = []
for d in sorted(glob.glob('../beta_cars/*')):
    try: b = Beta(d)
    except Exception: continue
    for g in b.geoms():
        f = struct.unpack_from('<6f', b.B, g+0x10); f = f[0:3] + struct.unpack_from('<3f', b.B, g+0x20)
        bb.append((os.path.basename(d), g, f))
gg = []
for d in sorted(glob.glob('../gc_cars/*')):
    try: c = GC(d)
    except Exception: continue
    for g in c.geoms():
        f = struct.unpack_from('>3f', c.B, g+0x10) + struct.unpack_from('>3f', c.B, g+0x20)
        gg.append((os.path.basename(d), g, f))
print(len(bb), len(gg))
pairs = {}
for bd, bg, bf in bb:
    best = min(gg, key=lambda x: max(abs(x[2][i]-bf[i]) for i in range(6)))
    err = max(abs(best[2][i]-bf[i]) for i in range(6))
    if err < 0.01:
        pairs.setdefault((bd, best[0]), []).append((hex(bg), hex(best[1]), round(err, 5)))
for k, v in sorted(pairs.items()): print(k, len(v), v[:4])
