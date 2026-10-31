"""Side-by-side comparison of a beta car and its GC counterpart, object type by type.
usage: python3 research/pair.py beta_cars/08_car_7 gc_cars/02_car_1 [type]"""
import sys, collections
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import classify
from walk import PTR, REFREL

def inst(s):
    d = collections.defaultdict(list)
    for name, off, n in s.order:
        d[name].append((off, n))
    return d

def mask(s, off, n):
    m = []
    for o in range(off, off + n, 4):
        if o + 4 > s.size: break
        c = s.claim[o]
        m.append("P" if c == PTR else "R" if c == REFREL else ".")
    return "".join(m)

def main():
    b = classify(sys.argv[1], "<")["pack"]; g = classify(sys.argv[2], ">")["pack"]
    ib, ig = inst(b), inst(g)
    only = sys.argv[3] if len(sys.argv) > 3 else None
    for t in sorted(set(ib) | set(ig)):
        if only and t != only: continue
        lb, lg = ib.get(t, []), ig.get(t, [])
        mism = 0; ex = None; valdiff = 0; words = 0
        for (ob, nb), (og, ng) in zip(lb, lg):
            n = min(nb, ng)
            mb, mg = mask(b, ob, n), mask(g, og, n)
            if mb.replace("R", "P") != mg.replace("R", "P"):
                mism += 1
                if ex is None: ex = (ob, og, mb, mg)
            for k in range(0, n, 4):
                if ob + k + 4 > b.size or og + k + 4 > g.size: break
                if mb[k // 4] == ".":
                    words += 1
                    if b.word(ob + k) != g.word(og + k): valdiff += 1
        print(f"{t:28s} beta {len(lb):4d} gc {len(lg):4d}  ptr-mask mismatches {mism:3d}  value-diff words {valdiff}/{words}")
        if ex and (only or True):
            print(f"      e.g. beta {ex[0]:X} {ex[2][:64]}\n           gc   {ex[1]:X} {ex[3][:64]}")
main()
