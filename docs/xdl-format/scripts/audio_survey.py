#!/usr/bin/env python3
"""Sound effects and instrument banks of the GameCube, PC and PS2 releases, side by side.

    python3 audio_survey.py SURVEY_ROOT          prints the tables of ../ultimate.md §3
    python3 audio_survey.py SURVEY_ROOT --content  + distinct samples by content hash, and every
                                                 distinct GameCube sound against the PS2's in
                                                 its slot (correlation; needs numpy)

SURVEY_ROOT holds `hwvx-x extract` trees gc/, pc/, ps2/ (survey_common.py). Standard library
only (ps2_audio.py for the PS2 formats).

* per release: sounds, sample rates, bits (descriptor byte 0x11; PS2 = 4-bit SPU-ADPCM),
  total duration;
* GameCube vs PC: every sound paired by index, samples compared byte for byte after the byte
  swap (GameCube PCM16 is big-endian), descriptor fields too; banks likewise;
* sounds and packs only one release has (by pack and slot), and the briefing archives
  (audio_/HWB_Mxx).
"""

import collections
import struct
import sys

import survey_common as S
import ps2_audio as A


def sounds(rel, folder):
    if not (folder / "pack.json").exists():
        return [], None, b""
    h, sd = A.load(folder)
    out = A.sfx(h, sd, be=(rel == "gc"))
    for s in out:
        if rel == "ps2":
            s["samples"] = (s["len"] // 16 - 1) * 28        # block 0 is silent
            s["bits"] = 4
        else:
            s["samples"] = s["len"] // 2
            s["bits"] = s["b11"]
    bo = h.get("song_bank_offset", 0)
    b = None
    if bo and sd[bo:bo + 4] in (b"RIFF", b"RIFX"):
        try:
            b = A.bank(sd, bo)
        except (ValueError, struct.error):
            b = None
    return out, b, sd


def pcm_equal(gc_sd, g, pc_sd, p):
    if g["len"] != p["len"]:
        return False
    a = gc_sd[g["ptr"]:g["ptr"] + g["len"]]
    b = pc_sd[p["ptr"]:p["ptr"] + p["len"]]
    n = len(a) // 2
    return struct.unpack(">%dh" % n, a[:2 * n]) == struct.unpack("<%dh" % n, b[:2 * n])


def main():
    root = S.survey_root(sys.argv[1:])
    arcs = {r: S.archives(root / r) for r in S.RELEASES}
    C = collections.Counter
    tot = collections.defaultdict(C)
    rates = collections.defaultdict(C)
    st = C()
    extra = collections.defaultdict(C)       # (release, archive) -> sounds without a partner slot
    briefs = collections.defaultdict(dict)
    for a in sorted(set().union(*arcs.values())):
        per = {}
        for r in S.RELEASES:
            if a in arcs[r]:
                for name, folder, _ in S.packs(arcs[r][a]):
                    per.setdefault(name, {})[r] = folder
        for name, folders in per.items():
            got = {r: sounds(r, f) for r, f in folders.items()}
            for r, (ss, b, _) in got.items():
                for s in ss:
                    tot[r]["sounds"] += 1
                    tot[r]["seconds"] += s["samples"] / s["rate"] if s["rate"] else 0
                    rates[r][(s["rate"], s["bits"])] += 1
                if b:
                    tot[r]["banks"] += 1
                    for e in b["entries"]:
                        tot[r]["bank entries"] += 1
                        rates[r + " bank"][(e["rate"], e["bits"])] += 1
                if a.startswith("hwb_m"):
                    n0, t0, r0 = briefs[a].get(r, (0, 0.0, set()))
                    briefs[a][r] = (n0 + len(ss), round(t0 + sum(s["samples"] / s["rate"] for s in ss if s["rate"]), 1),
                                    r0 | {s["rate"] for s in ss})
            n = {r: len(got[r][0]) for r in got}
            if len(n) > 1:
                m = min(n.values())
                for r in got:
                    if n[r] > m:
                        extra[(r, a)][f"+{n[r] - m}"] += 1
            for r in S.RELEASES:
                if r not in got and any(len(got[x][0]) for x in got):
                    st[f"packs with sounds missing from {r}"] += 1
            if "gc" in got and "pc" in got:
                (G, gb, gsd), (Pc, pb, psd) = got["gc"], got["pc"]
                for g, p in zip(G, Pc):
                    st["GC-PC sounds paired"] += 1
                    st["GC-PC samples byte-identical (after swap)"] += pcm_equal(gsd, g, psd, p)
                    st["GC-PC same rate"] += g["rate"] == p["rate"]
                    st["GC-PC same loop"] += g["loop"] == p["loop"]
                    st["GC-PC same volume"] += g["vol"] == p["vol"]
                if len(G) != len(Pc):
                    st["GC-PC packs with different sound counts"] += 1
                if gb and pb:
                    for e, f in zip(gb["entries"], pb["entries"]):
                        st["GC-PC bank entries paired"] += 1
                        same = e["len"] == f["len"] and e["rate"] == f["rate"] and e["loop"] == f["loop"]
                        if same:
                            x = gsd[gb["data"] + e["off"]:gb["data"] + e["off"] + e["len"]]
                            y = psd[pb["data"] + f["off"]:pb["data"] + f["off"] + f["len"]]
                            k = len(x) // 2
                            same = struct.unpack(">%dh" % k, x[:2 * k]) == struct.unpack("<%dh" % k, y[:2 * k])
                        st["GC-PC bank samples byte-identical (after swap)"] += same
    print("## totals")
    for k in ("sounds", "seconds", "banks", "bank entries"):
        print(f"  {k:14s} " + "  ".join(f"{r} {tot[r][k]:10.0f}" for r in S.RELEASES))
    print("\n## (rate, bits) per release")
    for r in sorted(rates):
        print(f"  {r}: " + ", ".join(f"{k[0]} Hz/{k[1]}b x{v}" for k, v in rates[r].most_common(8))
              + (f", ... ({len(rates[r])} distinct)" if len(rates[r]) > 8 else ""))
    print("\n## GameCube vs PC")
    for k in sorted(st):
        print(f"  {k}: {st[k]}")
    print("\n## sounds a release has beyond the others' count in the same pack (by archive)")
    agg = collections.defaultdict(C)
    for (r, a), v in extra.items():
        agg[r][a] += sum(int(k[1:]) * c for k, c in v.items())
    for r in agg:
        print(f"  {r}: total {sum(agg[r].values())}: " + ", ".join(f"{a} {n}" for a, n in agg[r].most_common()))
    print("\n## briefing archives (sounds, seconds, rates)")
    for a in sorted(briefs):
        print(f"  {a}: " + "  ".join(f"{r} {briefs[a][r][0]} sounds {briefs[a][r][1]} s {sorted(briefs[a][r][2])}"
                                     for r in S.RELEASES if r in briefs[a]))
    if "--content" in sys.argv:
        content(root)
    return 0


def content(root):
    """Distinct samples by content, PC coverage of the GameCube's, PS2 slot correlation."""
    import hashlib
    import numpy as np

    def digest(sd, s, be):
        return hashlib.md5(np.frombuffer(sd[s["ptr"]:s["ptr"] + s["len"]], ">i2" if be else "<i2")
                           .astype("<i2").tobytes()).hexdigest()
    arcs = {r: S.archives(root / r) for r in S.RELEASES}
    pc = set()
    for a in arcs["pc"]:
        for _, f, _ in S.packs(arcs["pc"][a]):
            ss, _, sd = sounds("pc", f)
            pc |= {digest(sd, s, False) for s in ss}
    gc, seen, res = {}, set(), []
    for a in sorted(arcs["gc"]):
        q = {n: f for n, f, _ in S.packs(arcs["ps2"][a])} if a in arcs["ps2"] else {}
        for n, f, _ in S.packs(arcs["gc"][a]):
            G, _, gs = sounds("gc", f)
            Q, _, qs = sounds("ps2", q[n]) if n in q else ([], None, b"")
            for i, g in enumerate(G):
                k = digest(gs, g, True)
                gc.setdefault(k, f"{a}/{n}#{g['i']}")
                if i >= len(Q) or (k, Q[i]["len"]) in seen:
                    continue
                seen.add((k, Q[i]["len"]))
                x = np.frombuffer(gs[g["ptr"]:g["ptr"] + g["len"]], ">i2").astype(float)
                y = np.array(A.vag_decode(qs, Q[i]["ptr"], Q[i]["len"])[0][28:], float)
                if len(y) < 2:
                    continue
                yr = np.interp(np.arange(len(x)) * Q[i]["rate"] / g["rate"], np.arange(len(y)), y)
                best = -1.0
                for lag in range(-8, 9):
                    xs, ys = x[max(0, lag):len(x) + min(0, lag)], yr[max(0, -lag):len(yr) + min(0, -lag)]
                    m = min(len(xs), len(ys))
                    if m >= 32 and xs[:m].std() and ys[:m].std():
                        best = max(best, float(np.corrcoef(xs[:m], ys[:m])[0, 1]))
                res.append((best, f"{a}/{n}#{g['i']}"))
    print(f"\n## content: {len(gc)} distinct GameCube samples, {sum(k in pc for k in gc)} of them in the PC release;"
          f" not in PC: {[v for k, v in gc.items() if k not in pc]}")
    v = np.array([r[0] for r in res])
    print(f"## PS2 vs GameCube, distinct (sample, PS2 slot) pairs: {len(v)}, correlation median {np.median(v):.4f},"
          f" < 0.95: {(v < 0.95).sum()}")
    for c, w in sorted(res)[:5]:
        print(f"  {c:.3f} {w}")


if __name__ == "__main__":
    sys.exit(main())
