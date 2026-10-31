"""Report-only port of src/roster.rs: car-select screens, carousel variables,
per-car string lists and car thumbnails, for an extracted pack folder.
usage: carlists.py <le|be> <pack dir> [<pack dir> ...]"""
import json, os, struct, sys

CAR_SELECT_ID = 2


class Blob:
    def __init__(self, d, e):
        self.e = '<' if e == 'le' else '>'
        self.data = open(os.path.join(d, 'data.bin'), 'rb').read()
        p = os.path.join(d, 'pointers.bin')
        pb = open(p, 'rb').read() if os.path.exists(p) else b''
        self.pointers = set(struct.unpack(self.e + '%dI' % (len(pb) // 4), pb))
        t = os.path.join(d, 'texture_relocs.bin')
        tb = open(t, 'rb').read() if os.path.exists(t) else b''
        self.tex = dict(struct.unpack(self.e + 'II', tb[i:i + 8]) for i in range(0, len(tb), 8))
        self.hdr = json.load(open(os.path.join(d, 'pack.json')))['header']

    def u32(self, o):
        if o + 4 > len(self.data): return 0
        return struct.unpack_from(self.e + 'I', self.data, o)[0]

    def s32(self, o):
        return struct.unpack_from(self.e + 'i', self.data, o)[0]

    def u16(self, o):
        return struct.unpack_from(self.e + 'H', self.data, o)[0]

    def b(self, o):
        return self.data[o] if o < len(self.data) else 0

    def ptr(self, slot):
        return self.u32(slot) if slot in self.pointers else None

    def cstr(self, o):
        n = self.data.index(0, o)
        return self.data[o:n].decode('latin1')


def walk(bl):
    seen = set()

    def header(off):
        if off is None or off in seen: return
        seen.add(off)
        n = min(bl.u32(off + 0xC), 500)
        objs = bl.ptr(off + 0x10)
        if objs is not None:
            for i in range(n):
                o = objs + i * 0x6C
                if bl.b(o + 1) in (6, 7):
                    h = bl.ptr(o + 0x3C)
                    if h is not None: header(bl.ptr(h))
                data = bl.ptr(o + 0x44)
                if data is not None: events(bl.u32(data + 0x10), bl.ptr(data + 0x14))
        events(bl.u32(off + 0x14), bl.ptr(off + 0x18))

    def events(n, lst):
        if lst is None: return
        for i in range(min(n, 0x1000)):
            d = bl.ptr(lst + i * 0xC + 8)
            if d is not None and bl.b(d + 5) in (1, 0x20):
                header(bl.ptr(d))

    table = bl.ptr(4)
    if table is not None:
        for i in range(min(bl.u32(0), 64)):
            header(bl.ptr(table + 8 * i))
    walked = set(seen)
    shaped = set()
    n = len(bl.data)
    for t in {bl.u32(s) for s in bl.pointers}:
        if t + 0x40 > n: continue
        c = bl.u32(t + 0xC)
        if not (1 <= c <= 500) or (t + 0x10) not in bl.pointers: continue
        objs = bl.ptr(t + 0x10)
        if objs + c * 0x6C > n: continue
        if all(bl.b(objs + i * 0x6C + 1) < 16 for i in range(c)) and t not in walked:
            shaped.add(t)
    return walked, shaped


def report(bl, name):
    walked, shaped = walk(bl)
    for h in sorted(walked | shaped):
        cs = h in walked and bl.u16(h) == CAR_SELECT_ID
        vp = bl.ptr(h + 0x1C)
        vars_ = []
        if vp is not None:
            vn = min(bl.u32(vp), 0x1000)
            lst = bl.ptr(vp + 4)
            if lst is not None: vars_ = [lst + i * 0x34 for i in range(vn)]
        lines = []
        if cs:
            for k, e in enumerate(vars_):
                if bl.b(e) == 0 and bl.s32(e + 4) == 0 and 20 <= bl.s32(e + 8) < 64:
                    lines.append('  carousel var #%d @0x%x type=%d min=%d max=%d' % (k, e, bl.b(e), bl.s32(e + 4), bl.s32(e + 8)))
        n = min(bl.u32(h + 0xC), 500)
        objs = bl.ptr(h + 0x10)
        if objs is None: continue
        for i in range(n):
            o = objs + i * 0x6C
            if o + 0x6C > len(bl.data) or bl.b(o + 1) not in (1, 10, 12): continue
            sub = bl.ptr(o + 0x14)
            if sub is None or bl.b(sub + 0x10) != 1: continue
            lst = bl.ptr(sub + 8)
            if lst is None: continue
            texts = bl.ptr(lst + 4)
            if texts is None: continue
            cnt = bl.u32(lst)
            if not (30 <= cnt < 0x400): continue
            strs = []
            for k in range(cnt):
                t = bl.ptr(texts + 4 * k)
                s = bl.ptr(t) if t is not None else None
                strs.append(bl.cstr(s) if s is not None else '')
            is_names = all(x.endswith('*') for x in strs)
            if not is_names and not cs: continue
            var = bl.u16(lst + 0xA)
            vinfo = ''
            if var < len(vars_):
                e = vars_[var]
                vinfo = 'var #%d type=%d min=%d max=%d' % (var, bl.b(e), bl.s32(e + 4), bl.s32(e + 8))
            lines.append('  %s list @0x%x obj %d: count=%d idxvar(u16@+0xA)=%d %s; [0]=%r [32]=%r [33..35]=%r' % (
                'NAMES' if is_names else 'HINTS', lst, i, cnt, var, vinfo, strs[0], strs[32] if cnt > 32 else None, strs[33:36]))
        if cs:
            # thumbnails: object type 0, sub type 3 -> anim with u8 +0xC == 1
            for i in range(n):
                o = objs + i * 0x6C
                if bl.b(o + 1) != 0: continue
                sub = bl.ptr(o + 0x14)
                if sub is None or bl.b(sub) != 3: continue
                anim = bl.ptr(sub + 0x10)
                if anim is None or bl.b(anim + 0xC) != 1: continue
                nk = bl.u32(anim)
                keys = bl.ptr(anim + 4)
                if keys is None: continue
                ids = [bl.tex.get(keys + 4 * k) for k in range(nk)]
                lines.append('  THUMB anim @0x%x obj %d: numKeys=%d key texture values ids0..2=%s ids32..35=%s' % (
                    anim, i, nk, [hex(x) if x is not None else None for x in ids[:3]],
                    [hex(x) if x is not None else None for x in ids[32:36]]))
        if lines:
            print('%s header @0x%x id=%d %s' % (name, h, bl.u16(h), 'CAR-SELECT' if cs else ('walked' if h in walked else 'shaped')))
            print('\n'.join(lines))


if __name__ == '__main__':
    e = sys.argv[1]
    for d in sys.argv[2:]:
        report(Blob(d, e), os.path.basename(d.rstrip('/')))
