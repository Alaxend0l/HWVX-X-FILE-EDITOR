"""Union of pointer-slot offsets per object type over all cars of each build,
plus offsets that hold non-zero non-pointer data.  Structural differences show
up as offsets that are pointers in one build and plain data in the other."""
import sys, os, collections, pickle
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import classify
from walk import PTR, REFREL, TEXREF, SNDREF

def stats(root, e):
    ptr = collections.defaultdict(collections.Counter)
    dat = collections.defaultdict(collections.Counter)
    tex = collections.defaultdict(collections.Counter)
    snd = collections.defaultdict(collections.Counter)
    cnt = collections.Counter(); sizes = {}
    for d in sorted(os.listdir(root)):
        if not d[0].isdigit() or "share" in d: continue
        s = classify(f"{root}/{d}", e)["pack"]
        for name, off, n in s.order:
            if name.endswith("[]") or name.startswith("keys") or name == "string": 
                continue  # arrays: element stride differs; skip
            cnt[name] += 1; sizes[name] = n
            for k in range(0, n, 4):
                o = off + k
                if o + 4 > s.size: break
                c = s.claim[o]
                if c in (PTR, REFREL): ptr[name][k] += 1
                elif c == TEXREF: tex[name][k] += 1
                elif c == SNDREF: snd[name][k] += 1
                elif s.word(o) != 0: dat[name][k] += 1
    return dict(ptr=ptr, dat=dat, tex=tex, snd=snd, cnt=cnt, sizes=sizes)

B = stats("beta_cars", "<"); G = stats("gc_cars", ">")
pickle.dump((B, G), open("research/typemasks.pkl", "wb"))
for t in sorted(set(B["cnt"]) | set(G["cnt"])):
    n = max(B["sizes"].get(t, 0), G["sizes"].get(t, 0))
    bad = []
    for k in range(0, n, 4):
        def cls(X):
            p, d, tx, sn = X["ptr"][t][k], X["dat"][t][k], X["tex"][t][k], X["snd"][t][k]
            return ("P" if p else "") + ("T" if tx else "") + ("S" if sn else "") + ("d" if d else "")
        cb, cg = cls(B), cls(G)
        # structural conflict: pointer/tex/snd in one, only plain data in other
        kb = set(cb) - {"d"}; kg = set(cg) - {"d"}
        if kb != kg and (kb and kg or (kb and "d" in cg) or (kg and "d" in cb)):
            bad.append(f"+{k:X}: beta[{cb}] gc[{cg}]")
    print(f"{t:26s} n={B['cnt'][t]:4d}/{G['cnt'][t]:4d} size {n:#x}  " + ("OK" if not bad else "; ".join(bad)))
