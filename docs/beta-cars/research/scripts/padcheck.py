"""For each GC object: is the part past the beta extent of the same type zero
(alignment padding) or not (possible new fields)?  Also the beta side: is the
part past the walker's struct size zero?"""
import sys, os, collections, bisect
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import classify
from walk import pointer_targets
def objs(root, e):
    for d in sorted(os.listdir(root)):
        if not d[0].isdigit() or "share" in d: continue
        s = classify(f"{root}/{d}", e)["pack"]
        starts = sorted(set(pointer_targets(s)) | set(s.objs))
        for name, off, n in s.order:
            i = bisect.bisect_right(starts, off)
            yield s, name, off, n, (starts[i] if i < len(starts) else s.size) - off
Bext = collections.defaultdict(collections.Counter)
for s, name, off, n, ext in objs("beta_cars", "<"):
    Bext[name][ext] += 1
res = collections.defaultdict(lambda: [0, 0, None])
for s, name, off, n, ext in objs("gc_cars", ">"):
    if name not in Bext or name.endswith("[]") or name.startswith("keys") or name == "string": continue
    eb = Bext[name].most_common(1)[0][0]
    if ext <= eb: continue
    tail = s.blob[off + eb: off + ext]
    r = res[name]
    if any(tail): r[1] += 1; r[2] = r[2] or (hex(off), tail[:16].hex())
    else: r[0] += 1
for k, (z, nz, ex) in sorted(res.items()):
    print(f"{k:26s} zero-tail {z:5d}  nonzero {nz:4d} {ex or ''}")
