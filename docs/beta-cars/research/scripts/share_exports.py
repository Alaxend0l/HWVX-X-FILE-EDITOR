"""List a shared pack's exports (ref_relocs with pack 0 at the start of the blob):
target offset, header words, bbox floats, strings inside the target's closure."""
import sys, re
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from walk import Pack, NONE
def strings(blob, a, b):
    return [m.group().decode() for m in re.finditer(rb"[ -~]{4,}", blob[a:b]) if b"." in m.group() or b"_" in m.group()]
def exports(d, e):
    s = Pack(d, e)
    ex = sorted((v & 0xFFFF, o, s.word(o)) for o, v in s.ref if (v >> 16) == 0 and o < 0x400)
    tgts = sorted({x[2] for x in ex})
    out = []
    for idx, slot, t in ex:
        nxt = min([x for x in tgts if x > t] or [s.size])
        bb = [round(s.f32(t + 0x10 + 4 * k), 2) for k in range(8)]
        out.append((idx, t, nxt - t, s.blob[t:t+8].hex(), bb, strings(s.blob, t, nxt)[:6]))
    return s, out
if __name__ == "__main__":
    for d, e in ((sys.argv[1], "<"), (sys.argv[2], ">")):
        print(d)
        for r in exports(d, e)[1]:
            print("  ", r)
