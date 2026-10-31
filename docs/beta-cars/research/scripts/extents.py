"""Per object type: distribution of the distance from an object's start to the
next object start / pointer target (its 'extent' in the blob), beta vs GC.
A type whose extent differs between builds changed size."""
import sys, os, collections, bisect
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import classify
from walk import pointer_targets
def run(root, e, only=None):
    H = collections.defaultdict(collections.Counter)
    for d in sorted(os.listdir(root)):
        if not d[0].isdigit() or "share" in d or (only and d not in only): continue
        s = classify(f"{root}/{d}", e)["pack"]
        starts = sorted(set(pointer_targets(s)) | set(s.objs))
        for name, off, n in s.order:
            if name.endswith("[]") or name.startswith("keys") or name == "string": continue
            i = bisect.bisect_right(starts, off)
            nxt = starts[i] if i < len(starts) else s.size
            H[name][nxt - off] += 1
    return H
B = run("beta_cars", "<"); G = run("gc_cars", ">")
for t in sorted(set(B) | set(G)):
    fb = ", ".join(f"{k:#x}:{v}" for k, v in B[t].most_common(5))
    fg = ", ".join(f"{k:#x}:{v}" for k, v in G[t].most_common(5))
    mb = B[t].most_common(1)[0][0] if B[t] else None; mg = G[t].most_common(1)[0][0] if G[t] else None
    mark = "" if mb == mg else "   <== DIFF"
    print(f"{t:26s} beta [{fb}]\n{'':26s} gc   [{fg}]{mark}")
