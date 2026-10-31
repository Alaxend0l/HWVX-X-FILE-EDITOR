"""Classify every byte of a car pack blob.
usage: python3 research/classify.py <packdir> <'<'|'>'> [-v]
"""
import sys, bisect, collections
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from walk import *


def classify(d, e, verbose=False):
    s = walk(d, e)
    h = s.h
    # texture area: texture_data_offset up to the next labelled/pointer-target thing
    # (headers + pixels; beta adds palettes at palette_offset)
    tg = pointer_targets(s)
    tex_lo = h["texture_data_offset"] if h["num_textures"] else None
    regions = []
    if tex_lo is not None:
        # textures are at the end of the object data in every pack seen:
        hi = s.size
        for t in tg:
            if t > tex_lo and s.label[t] is not None and s.label[t] != "string":
                hi = min(hi, t)
        regions.append((tex_lo, hi))
    for a, b in regions:
        for i in range(a, b):
            if s.claim[i] not in (PTR,):
                s.claim[i] = TEX
            if s.label[i] is None:
                s.label[i] = "texture"
    stop = {o for o, n in s.objs.items() if n not in ("string",)}
    gch = geometry_closure(s, s.geom_roots, stop)
    for a, b in gch:
        for i in range(a, b):
            if s.label[i] is None:
                s.label[i] = "geometry"
                if s.claim[i] == DEFAULT:
                    s.claim[i] = GEOM
    # object tails: a chunk between consecutive object starts / pointer targets
    # whose start is labelled inherits that label (struct sizes in walk.py are
    # lower bounds)
    starts = sorted(set(tg) | set(s.objs) | {0})
    who = collections.defaultdict(list)
    for o in list(s.pointers) + [o for o, v in s.ref if (v >> 16) == 0]:
        t_ = s.word(o)
        if t_ < s.size:
            who[t_].append(o)
    for k, a in enumerate(starts):
        b = starts[k + 1] if k + 1 < len(starts) else s.size
        base = s.label[a] if a < s.size else None
        if base is None:
            # labelled bytes inside the chunk (an inline object) also extend
            for i in range(a, b):
                if s.label[i] is not None:
                    base = s.label[i]
                elif base is not None:
                    s.label[i] = base.rstrip("~") + "~"
            continue
        for i in range(a, b):
            if s.label[i] is None:
                s.label[i] = base.rstrip("~") + "~"
            else:
                base = s.label[i]
    # generic: chunks only reached through pointer slots of labelled objects
    # (fields the walker does not type): "ptd:<parent>"; repeat to a fixpoint
    changed = True
    while changed:
        changed = False
        for k, a in enumerate(starts):
            if a >= s.size or s.label[a] is not None:
                continue
            b = starts[k + 1] if k + 1 < len(starts) else s.size
            par = None
            for o in who.get(a, []):
                if s.label[o] is not None:
                    par = s.label[o]; break
            if par is None:
                continue
            nm = par if par.startswith("ptd:") else "ptd:" + par.rstrip("~")
            for i in range(a, b):
                if s.label[i] is None:
                    s.label[i] = nm
            changed = True
    # coverage
    lab = collections.Counter()
    for x in s.label:
        lab[x] += 1
    kinds = collections.Counter(s.claim)
    known = s.size - lab[None]
    res = dict(size=s.size, known=known, pct=100.0 * known / s.size,
               labels=lab, kinds={KNAMES[k]: v for k, v in kinds.items()})
    # unknown chunks: between pointer targets, with who points at them
    who = collections.defaultdict(list)
    for o in s.pointers:
        t = s.word(o)
        if t < s.size:
            who[t].append(o)
    unk = []
    starts = sorted(set(tg) | set(s.objs))
    i = 0
    while i < s.size:
        if s.label[i] is None:
            j = i
            while j < s.size and s.label[j] is None:
                j += 1
            refs = [(f"{s.label[o] or '?'}@{o - max([x for x in s.objs if x <= o] or [0]):X}") for o in who.get(i, [])][:3]
            unk.append((i, j - i, refs))
            i = j
        else:
            i += 1
    res["unknown"] = unk
    res["pack"] = s
    # pointer slots outside any label
    res["ptr_unlabelled"] = sum(1 for o in s.pointers if s.label[o] is None)
    return res


def label_rel(s, o):
    st = max(x for x in s.objs if x <= o)
    return s.objs[st], o - st


if __name__ == "__main__":
    r = classify(sys.argv[1], sys.argv[2])
    print(f"size {r['size']}  labelled {r['known']} ({r['pct']:.2f}%)  unlabelled ptr slots {r['ptr_unlabelled']}")
    print("claims:", r["kinds"])
    for k, v in sorted(r["labels"].items(), key=lambda x: -x[1]):
        print(f"  {v:9d}  {k}")
    if "-v" in sys.argv:
        print("unknown chunks:")
        for a, n, refs in r["unknown"][:200]:
            print(f"  {a:06X} +{n:X}  from {refs}")
