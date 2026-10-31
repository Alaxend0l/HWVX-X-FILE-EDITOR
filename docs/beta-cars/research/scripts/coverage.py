import sys, collections
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import classify
def cat(l):
    if l is None: return "unreached"
    if l == "geometry": return "geometry"
    if l == "texture": return "texture"
    if l == "string": return "string"
    if l.startswith("ptd:"): return "reached-untyped"
    if l.endswith("~"): return "typed-tail"
    return "typed"
for d, e in [(a, "<" if "beta" in a else ">") for a in sys.argv[1:]]:
    r = classify(d, e); s = r["pack"]
    c = collections.Counter(cat(l) for l in s.label)
    kinds = r["kinds"]
    print(d, s.size, " ".join(f"{k}={v/s.size*100:.1f}%" for k, v in sorted(c.items())))
    print("   claims:", " ".join(f"{k}={v}" for k, v in sorted(kinds.items())))
    pt = collections.Counter(l for l in s.label if l and l.startswith("ptd:"))
    print("   untyped by parent:", ", ".join(f"{k[4:]}={v}" for k, v in pt.most_common(12)))
    tl = collections.Counter(l for l in s.label if l and l.endswith("~"))
    print("   tails:", ", ".join(f"{k}={v}" for k, v in tl.most_common(12)))
