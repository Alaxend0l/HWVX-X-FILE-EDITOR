"""Cross-pack imports of car packs: (kind, pack, index) -> contexts."""
import sys, os, collections
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import classify
from relocs import where
def imports(d, e):
    s = classify(d, e)["pack"]
    out = collections.defaultdict(collections.Counter)
    for kind, tab in (("tex", s.tex), ("snd", s.snd), ("ref", s.ref)):
        for o, v in tab:
            if v >> 16:
                w = where(s, o)
                if w.startswith("geometry"): w = "geometry"
                out[(kind, v >> 16, v & 0xFFFF)][w] += 1
    return out
if __name__ == "__main__":
    for d in sys.argv[1:]:
        e = "<" if "beta" in d else ">"
        im = imports(d, e)
        print(d)
        for k in sorted(im):
            print(f"  {k[0]} pk{k[1]} #{k[2]:3d}: " + ", ".join(f"{w}x{n}" for w, n in im[k].most_common()))
