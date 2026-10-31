"""Where texture / sound / ref relocs sit (object type + field), per build."""
import sys, os, collections
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import classify
def where(s, o):
    st = max([x for x in s.objs if x <= o] or [0])
    if s.label[o] is None: return "?"
    return f"{s.label[o]}+{o - st:X}" if s.objs.get(st) == s.label[o] else f"{s.label[o]}(+{o-st:X})"
def run(root, e, cars=None):
    T = collections.Counter()
    for d in sorted(os.listdir(root)):
        if not d[0].isdigit() or "share" in d or (cars and d not in cars): continue
        s = classify(f"{root}/{d}", e)["pack"]
        for kind, tab in (("tex", s.tex), ("snd", s.snd), ("ref", s.ref)):
            for o, v in tab:
                w = where(s, o)
                if w in ("geometry",) or w.startswith("geometry"): w = "geometry"
                T[(kind, w, "pk%d" % (v >> 16))] += 1
    return T
if __name__ == "__main__":
    B = run("beta_cars", "<"); G = run("gc_cars", ">")
    for k in sorted(set(B) | set(G)):
        print(f"{k[0]} {k[2]} {k[1]:40s} beta {B[k]:5d}  gc {G[k]:5d}")
