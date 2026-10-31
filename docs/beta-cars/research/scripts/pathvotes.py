"""Map shared-pack import indices beta -> GC by structural position.
Every chunk (blob range between consecutive pointer targets) gets a path from
the root: the relative offsets of the pointer slots followed to reach it (BFS,
slots visited in offset order).  A reloc slot's key is (path of its chunk,
offset in chunk).  Keys present in a beta car and its GC twin give votes
beta index -> GC index, per (kind, pack)."""
import sys, os, collections, bisect, json
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from walk import Pack, pointer_targets

def paths(s):
    tg = sorted(set(pointer_targets(s)) | {0})
    local = [o for o in s.pointers] + [o for o, v in s.ref if (v >> 16) == 0]
    slots = sorted(set(local))
    def chunk(o):
        i = bisect.bisect_right(tg, o) - 1
        return tg[i]
    P = {0: ()}
    q = collections.deque([0])
    while q:
        c = q.popleft()
        i = bisect.bisect_right(tg, c)
        end = tg[i] if i < len(tg) else s.size
        j = bisect.bisect_left(slots, c)
        while j < len(slots) and slots[j] < end:
            o = slots[j]; t = s.word(o)
            if t < s.size:
                ct = chunk(t)
                if ct not in P:
                    P[ct] = P[c] + ((o - c, t - ct),)
                    q.append(ct)
            j += 1
    return P, chunk

def keys(d, e):
    s = Pack(d, e)
    P, chunk = paths(s)
    out = {}
    for kind, tab in (("tex", s.tex), ("snd", s.snd), ("ref", s.ref)):
        for o, v in tab:
            c = chunk(o)
            if c in P:
                out[(kind, P[c], o - c)] = v
    return out

PAIRS = [("01_car_0", "01_car_0"), ("04_car_3", "04_car_3"), ("05_car_4", "05_car_4"),
         ("07_car_6", "10_car_9"), ("08_car_7", "02_car_1"), ("09_car_8", "03_car_2"),
         ("10_car_9", "07_car_6"), ("11_car_10", "08_car_7"), ("14_car_13", "11_car_10"),
         ("16_car_15", "16_car_15"), ("20_car_19", "17_car_16"), ("23_car_22", "20_car_19"),
         ("26_car_25", "23_car_22"), ("29_car_28", "26_car_25"), ("31_car_30", "28_car_27"),
         ("32_car_31", "29_car_28")]

if __name__ == "__main__":
    V = collections.defaultdict(collections.Counter)
    hit = miss = 0
    for b, g in PAIRS:
        kb = keys(f"beta_cars/{b}", "<"); kg = keys(f"gc_cars/{g}", ">")
        for k, vb in kb.items():
            if (vb >> 16) == 0: continue
            vg = kg.get(k)
            if vg is None: miss += 1; continue
            hit += 1
            V[(k[0], vb >> 16, vb & 0xFFFF)][(vg >> 16, vg & 0xFFFF)] += 1
    print(f"matched keys {hit}, unmatched {miss}")
    res = {}
    for k in sorted(V):
        tot = sum(V[k].values()); (best, n) = V[k].most_common(1)[0]
        res[f"{k[0]}:{k[1]}:{k[2]}"] = [best[0], best[1], n, tot]
        print(f"{k[0]} pk{k[1]} #{k[2]:3d} -> pk{best[0]} #{best[1]:3d}  votes {n}/{tot}   others {list(V[k].most_common(4))[1:]}")
    json.dump(res, open("research/pathvotes.json", "w"), indent=0)
