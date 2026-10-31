"""Paired-car check of u8/u16 claimed fields and of key-frame data."""
import sys, collections
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import classify
from walk import U16, BYTES, PTR
b = classify(sys.argv[1], "<")["pack"]; g = classify(sys.argv[2], ">")["pack"]
def inst(s):
    d = collections.defaultdict(list)
    for name, off, n in s.order: d[name].append((off, n))
    return d
ib, ig = inst(b), inst(g)
tot = collections.Counter()
for t in sorted(set(ib) & set(ig)):
    if len(ib[t]) != len(ig[t]): continue
    same = diff = 0; ex = []
    for (ob, nb), (og, ng) in zip(ib[t], ig[t]):
        for k in range(min(nb, ng)):
            if ob + k >= b.size or og + k >= g.size: break
            cb, cg = b.claim[ob + k], g.claim[og + k]
            if cg == U16 and k % 2 == 0:
                vb, vg = b.half(ob + k), g.half(og + k)
                if vb == vg: same += 1
                else: diff += 1; ex.append(f"+{k:X}:{vb:#x}/{vg:#x}")
            elif cg == BYTES:
                if b.blob[ob + k] == g.blob[og + k]: same += 1
                else: diff += 1; ex.append(f"+{k:X}b:{b.blob[ob+k]:#x}/{g.blob[og+k]:#x}")
    if same + diff:
        print(f"{t:26s} small fields same {same:5d} diff {diff:4d}  {' '.join(ex[:6])}")
# key frame data words
for (ob, _), (og, _) in zip(ib["KeyAnim"], ig["KeyAnim"]):
    fr = b.word(ob + 0x28); nb_ = b.word(ob + 0x10)
    print("KeyAnim beta", hex(ob), "bones", nb_, "frames", fr, "| gc bones", g.word(og + 0x10), "frames", g.word(og + 0x28))
    for k in (0x20, 0x24):
        tb, tg = b.target(ob + k), g.target(og + k)
        n = 64
        wb = [b.word(tb + 4 * i) for i in range(n)]; wg = [g.word(tg + 4 * i) for i in range(n)]
        print(f"  +{k:X}: first {n} words equal: {sum(x == y for x, y in zip(wb, wg))}/{n}")
        print("   beta", " ".join(f"{x:08X}" for x in wb[:8]))
        print("   gc  ", " ".join(f"{x:08X}" for x in wg[:8]))
