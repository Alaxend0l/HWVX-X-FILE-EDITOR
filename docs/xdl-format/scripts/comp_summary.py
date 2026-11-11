#!/usr/bin/env python3
"""Tables for ../components.md from comp_survey.py's output.

    python3 comp_summary.py SURVEY_ROOT [--rel pc] [--kind LABEL]   (--kind: field detail of one kind)
"""

import collections
import json
import pickle
import statistics
import sys

import survey_common as S
from comp_survey import KINDS


def load(root, rel):
    d = root / "components"
    X = [json.loads(l) for l in open(d / f"{rel}_instances.jsonl")]
    with open(d / f"{rel}_variants.pkl", "rb") as f:
        V = pickle.load(f)
    for x in X:
        x["pk"] = x["arch"] + "/" + x["pack"]
        x["cat"] = S.category(x["arch"])
    return X, V


def diff(a, b):
    """(differing fields, fields, [(label, offset)] of the differences)"""
    n = min(len(a), len(b))
    where = [(a[i][0], a[i][1]) for i in range(n) if a[i][2] != b[i][2]]
    return len(where) + abs(len(a) - len(b)), max(len(a), len(b)), where


def kind_stats(xs, V, detail=False):
    vfams = collections.defaultdict(set)
    for x in xs:
        vfams[x["deep"]].add(x["fam"])
    vpacks = collections.defaultdict(set)
    vcount = collections.Counter()
    vbytes = {}
    for x in xs:
        vpacks[x["deep"]].add(x["pk"]); vcount[x["deep"]] += 1; vbytes[x["deep"]] = x["bytes"]
    total = sum(x["bytes"] for x in xs)
    unique = sum(vbytes.values())
    xshared = [d for d in vpacks if len(vpacks[d]) > 1]
    inst_x = sum(vcount[d] for d in xshared)
    # within-pack copies: same variant twice in one pack
    perpack = collections.Counter((x["pk"], x["deep"]) for x in xs)
    inpack = sum(c - 1 for c in perpack.values())
    # near duplicates
    shapes = collections.defaultdict(list)
    for d in vcount:
        shapes[next(x["shape"] for x in xs if x["deep"] == d)].append(d)
    nd_vars = 0; diffs = []; fields = []; where = collections.Counter(); rel = []
    for sh, ds in shapes.items():
        if len(ds) < 2:
            continue
        nd_vars += len(ds)
        rep = max(ds, key=lambda d: (vcount[d], d))
        for d in ds:
            if d == rep or d not in V or rep not in V:
                continue
            k, n, w = diff(V[rep], V[d])
            diffs.append(k); fields.append(n); rel.append(k / max(n, 1))
            where.update(set(w))
    return {
        "n": len(xs), "packs": len({x["pk"] for x in xs}), "fams": len({x["fam"] for x in xs}),
        "xfam": sum(1 for d in vfams if len(vfams[d]) > 1),
        "inst_xfam": sum(vcount[d] for d in vfams if len(vfams[d]) > 1),
        "maxfams": max(len(p) for p in vfams.values()),
        "cats": sorted({x["cat"] for x in xs}),
        "variants": len(vcount), "shapes": len(shapes),
        "xvariants": len(xshared), "inst_x": inst_x,
        "maxpacks": max(len(p) for p in vpacks.values()),
        "inpack": inpack,
        "bytes": total, "saved": total - unique,
        "nd_vars": nd_vars,
        "nd_median": statistics.median(diffs) if diffs else None,
        "nd_fields": statistics.median(fields) if fields else None,
        "nd_rel": statistics.median(rel) if rel else None,
        "nd_le4": sum(1 for k in diffs if k <= 4), "nd_n": len(diffs),
        "where": where.most_common(8),
        "xcat": sum(1 for d in vpacks if len({p.split("/")[0] for p in vpacks[d]}) > 1 and
                    len({S.category(p.split("/")[0]) for p in vpacks[d]}) > 1),
    }


def kb(b):
    return f"{b / 1024:.0f}" if b >= 10240 else f"{b / 1024:.1f}"


def summary(root, rel):
    X, V = load(root, rel)
    B = json.loads((root / "components" / f"{rel}_bytes.json").read_text())
    print(f"## {rel}: {len({x['pk'] for x in X})} packs walked, {len(X)} instances")
    for cat in (None, "cars", "worlds", "items/share", "interface", "audio"):
        xs = [x for x in X if x["top"] and (cat is None or x["cat"] == cat)]
        if not xs:
            continue
        print(f"\n### {cat or 'all packs'}\n")
        print("| group | label | inst | packs | families | variants | shapes | variants in 2+ families |"
              " inst in such | max families | in-pack copies | KB stored | KB dedup in family | KB dedup all |"
              " near-dup variants | median diff / fields | diff<=4 |")
        print("|" + "---|" * 17)
        by = collections.defaultdict(list)
        for x in xs:
            by[x["label"]].append(x)
        for lab, ys in sorted(by.items(), key=lambda kv: (KINDS[kv[0]][0], -sum(y["bytes"] for y in kv[1]))):
            st = kind_stats(ys, V)
            nd = f"{st['nd_median']:.0f} / {st['nd_fields']:.0f}" if st["nd_median"] is not None else "-"
            le4 = f"{st['nd_le4']}/{st['nd_n']}" if st["nd_n"] else "-"
            b = B["kinds"].get(lab, {}).get(cat or "all", {"total": 0, "dedup_family": 0, "dedup_all": 0})
            print(f"| {KINDS[lab][0]} | {lab} | {st['n']} | {st['packs']} | {st['fams']} | {st['variants']} | "
                  f"{st['shapes']} | {st['xfam']} | {100 * st['inst_xfam'] / st['n']:.0f}% | {st['maxfams']} | "
                  f"{st['inpack']} | {kb(b['total'])} | {kb(b['dedup_family'])} | {kb(b['dedup_all'])} | "
                  f"{st['nd_vars']} | {nd} | {le4} |")
    print("\n### variants shared across categories")
    vc = collections.defaultdict(set)
    vl = {}
    for x in X:
        if x["top"]:
            vc[x["deep"]].add(x["cat"]); vl[x["deep"]] = x["label"]
    c = collections.Counter(vl[d] for d in vc if len(vc[d]) > 1)
    print(dict(c))
    print("\n### coverage (bytes of the walked packs' blobs)\n")
    print("| category | packs | blob KB | textures | geometry | objects under a kind | other |")
    print("|---|---|---|---|---|---|---|")
    for cat, c in B["coverage"].items():
        o = c["blob"] - c["tex"] - c["geom"] - c["obj"]
        print(f"| {cat} | {c['packs']} | {c['blob'] // 1024} | " + " | ".join(
            f"{100 * v / c['blob']:.0f}%" for v in (c["tex"], c["geom"], c["obj"], o)) + " |")
    print("\nback edges (cycles) per pack, max:", max(x["cycles"] for x in X))


def kind_detail(root, rel, lab):
    X, V = load(root, rel)
    xs = [x for x in X if x["top"] and x["label"] == lab]
    st = kind_stats(xs, V, True)
    print(json.dumps({k: v for k, v in st.items()}, indent=1, default=str))
    vp = collections.defaultdict(set)
    for x in xs:
        vp[x["deep"]].add(x["pk"])
    for d, ps in sorted(vp.items(), key=lambda kv: -len(kv[1]))[:12]:
        print(d, len(ps), sorted(ps)[:4])


if __name__ == "__main__":
    r = S.survey_root(sys.argv[1:])
    rl = sys.argv[sys.argv.index("--rel") + 1] if "--rel" in sys.argv else "pc"
    if "--kind" in sys.argv:
        kind_detail(r, rl, sys.argv[sys.argv.index("--kind") + 1])
    else:
        summary(r, rl)
