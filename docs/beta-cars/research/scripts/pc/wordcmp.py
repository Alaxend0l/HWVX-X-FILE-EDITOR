# word-level alignment of the non-geometry/non-texture object regions of a PC/GC pair
import sys, difflib, struct, collections
sys.path.insert(0, '.')
from objcmp import run
def words(o, a, b, e):
    a &= ~15; b = (b + 3) & ~3; out = []
    for x in range(a, b, 4):
        if x in o.p.ptrs: out.append(('P',)); continue
        out.append(('V', struct.unpack_from(e+'I', o.B, x)[0]))
    return a, out
def classify(pc, gc, pa, ga, pw, gw):
    st = collections.Counter(); diffs = []
    sm = difflib.SequenceMatcher(None, pw, gw, autojunk=False)
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag == 'equal':
            for k in range(i1, i2):
                st['equal(ptr slot)' if pw[k][0] == 'P' else 'equal(value after byteswap)'] += 1
            continue
        if tag == 'replace' and i2 - i1 == j2 - j1:
            for k, l in zip(range(i1, i2), range(j1, j2)):
                rp = pc.B[pa+4*k:pa+4*k+4]; rg = gc.B[ga+4*l:ga+4*l+4]
                if rp == rg: st['raw bytes equal (not swapped on GC)'] += 1
                elif rp[:2][::-1] + rp[2:][::-1] == rg[:2] or (rp[1::-1] == rg[0:2] and rp[3:1:-1] == rg[2:4]): st['u16-pair swap equal'] += 1
                else:
                    st['different'] += 1; diffs.append((hex(pa+4*k), rp.hex(), hex(ga+4*l), rg.hex()))
        else:
            st[f'unaligned {tag}'] += max(i2-i1, j2-j1)
            diffs.append((tag, hex(pa+4*i1), i2-i1, hex(ga+4*j1), j2-j1))
    return st, diffs
if __name__ == '__main__':
    name = sys.argv[1] if len(sys.argv) > 1 else '01_car_0'
    pc, gc, pnz, gnz = run(name)
    tot = collections.Counter()
    for (p0, p1), (g0, g1) in zip(pnz, gnz):
        pa, pw = words(pc, p0, p1, '<'); ga, gw = words(gc, g0, g1, '>')
        st, diffs = classify(pc, gc, pa, ga, pw, gw); tot += st
        print(hex(pa), hex(ga), dict(st))
        for d in diffs[:40]: print('   ', d)
    print('TOTAL', dict(tot))
