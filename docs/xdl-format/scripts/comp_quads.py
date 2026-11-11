#!/usr/bin/env python3
"""Untyped repetition in the packs comp_survey.py does not walk: world quadrant geometry
(`w*_geo`, `bat*` geometry) and collision packs.

    python3 comp_quads.py SURVEY_ROOT [--rel pc]

Every pointer-target node gets the same deep fingerprint as in comp_survey.py (bytes with
pointer slots replaced by their targets' fingerprints, texture slots by texture content). Bytes are
summed as stored, deduplicated within an archive, and deduplicated across all of them.
"""

import collections
import json
import sys

import survey_common as S
import comp_survey as C


def main():
    root = S.survey_root(sys.argv[1:])
    rel = sys.argv[sys.argv.index("--rel") + 1] if "--rel" in sys.argv else "pc"
    e = ">" if rel == "gc" else "<"
    sys.setrecursionlimit(1000000)
    shared = C.shared_tokens(root, rel, e)
    stats = collections.defaultdict(lambda: {"packs": 0, "total": 0, "arch": set(), "all": {}, "whole": collections.Counter()})
    for a, folder in sorted(S.archives(root / rel).items()):
        for name, f, ent in S.packs(folder):
            if ent["type"] not in ("geometry", "collision") or not (f / "pointers.bin").exists():
                continue
            w = C.W.Pack(str(f), e)
            w.geom_roots = set()
            g = C.Graph(w, [], [], shared)
            g.targets_set = set(g.targets)
            st = stats[ent["type"]]
            st["packs"] += 1
            st["whole"][C.md5(w.blob)] += 1
            for t in g.targets:
                ln = min(g.extent(t), g.size - t)
                d = g.fp(t)
                st["total"] += ln
                st["arch"].add((a, d, ln))
                st["all"][d] = ln
    for k, st in stats.items():
        fam = sum(ln for _, _, ln in st["arch"])
        print(f"{k}: {st['packs']} packs, {st['total'] // 1024} KB in nodes, dedup within archive "
              f"{fam // 1024} KB, across all {sum(st['all'].values()) // 1024} KB; "
              f"byte-identical packs: {sum(c - 1 for c in st['whole'].values())}")


if __name__ == "__main__":
    import threading
    threading.stack_size(512 * 1024 * 1024)
    t = threading.Thread(target=main)
    t.start(); t.join()
