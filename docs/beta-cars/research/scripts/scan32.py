# for every interface header: vars with max>=30, texture anims (sub type 3) with >=30 keys and the var that drives them,
# string lists with >=30 entries, code-var 0x64 conditions
import glob
from ifwalk import W
for d in sorted(glob.glob('ipack/0*')):
    w = W(d); w.headers(w.u32(w.u32(4)))
    for h in sorted(w.seen):
        out = []
        for v in w.vars(h):
            if v['max'] in range(28, 40) or (v['code'] and v['code'][0] in ('0x64', '0xcb')):
                out.append('  var%d type%d min%d max%d off%d code%s @%s' % (v['i'], v['type'], v['min'], v['max'], v['off'], v['code'], v['at']))
        n = w.u32(h+0xC); objs = w.ptr(h+0x10)
        c64 = 0
        for i in range(n):
            o = objs + i*0x6C; sub = w.ptr(o+0x14)
            if w.b[o+1] == 0 and sub is not None and w.b[sub] == 3:
                a = w.ptr(sub+0x10)
                if a is not None and w.u32(a) >= 20:
                    out.append('  obj%d texanim@%x keys=%d u8C=%d var(u16 E)=%d' % (i, a, w.u32(a), w.b[a+0xC], w.u16(a+0xE)))
            t = w.objtext(o)
            if isinstance(t, tuple) and len(t[2]) >= 20:
                out.append('  obj%d strlist@%x %s %s' % (i, t[3], t[1], t[2]))
            c = w.conds(o) or []
            for cc in c[1:]:
                if cc[0] == 'code' and cc[1] in ('0x64',): c64 += 1
        if c64: out.append('  %d conditions on code var 0x64' % c64)
        if out:
            print(d.split('/')[-1], 'hdr %x id %d objs %d' % (h, w.u16(h), n)); print('\n'.join(out))
