# dump string-list text objects (sub+0x10 == 1) of a header
import sys
from ifwalk import W
def lists(w, h):
    n = w.u32(h+0xC); objs = w.ptr(h+0x10); out = []
    for i in range(n):
        o = objs + i*0x6C; sub = w.ptr(o+0x14)
        if sub is None or w.b[o+1] not in (1, 10, 12): continue
        if w.b[sub+0x10] != 1: continue
        lst = w.ptr(sub+8)
        if lst is None: continue
        cnt = w.u32(lst); texts = w.ptr(lst+4)
        names = []
        for k in range(cnt):
            tx = w.ptr(texts+4*k); s = w.ptr(tx) if tx is not None else None
            names.append(w.cstr(s) if s is not None else None)
        out.append((i, o, sub, lst, cnt, texts, w.b[lst+8], w.u16(lst+0xA), names))
    return out
if __name__ == '__main__':
    w = W(sys.argv[1])
    for h in sys.argv[2:]:
        for (i, o, sub, lst, cnt, texts, b8, var, names) in lists(w, int(h, 16)):
            print('hdr %s obj%d @%x sub@%x list@%x count=%d texts@%x u8_8=%d var=%d' % (h, i, o, sub, lst, cnt, texts, b8, var))
            print('   ', names)
