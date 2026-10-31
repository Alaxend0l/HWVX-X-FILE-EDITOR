import sys, glob
from ifwalk import W
for d in sorted(glob.glob('ipack/0*')):
    w = W(d); root = w.u32(w.u32(4))
    w.headers(root)
    for h,(par,dep) in sorted(w.seen.items()):
        vs = w.vars(h)
        print(d.split('/')[-1], hex(h), 'id', w.u16(h), w.u16(h+2), 'objs', w.u32(h+0xC), 'vars', len(vs), 'depth', dep)
