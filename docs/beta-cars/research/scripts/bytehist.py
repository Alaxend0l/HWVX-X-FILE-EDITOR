"""Histogram of claimed u8 fields per object type, beta vs GC (all cars)."""
import sys, os, collections
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import classify
from walk import BYTES, U16
types = sys.argv[1].split(",")
def run(root, e):
    H = collections.defaultdict(collections.Counter)
    for d in sorted(os.listdir(root)):
        if not d[0].isdigit() or "share" in d: continue
        s = classify(f"{root}/{d}", e)["pack"]
        for name, off, n in s.order:
            if name not in types: continue
            for k in range(n):
                if off + k < s.size and s.claim[off + k] == BYTES:
                    H[(name, k)][s.blob[off + k]] += 1
                if off + k < s.size and s.claim[off + k] == U16 and k % 2 == 0:
                    H[(name, k)][s.half(off + k)] += 1
    return H
B = run("beta_cars", "<"); G = run("gc_cars", ">")
for k in sorted(set(B) | set(G)):
    fb = dict(B[k].most_common(8)); fg = dict(G[k].most_common(8))
    print(f"{k[0]}+{k[1]:X}: beta {fb}  gc {fg}")
