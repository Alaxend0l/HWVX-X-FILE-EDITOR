"""Per-byte swap plan for a car pack blob (prototype for the beta->GC converter).

usage: python3 research/swapmap.py <packdir> [<packdir>...]   (beta dirs: '<', gc dirs: '>')
       python3 research/swapmap.py --selftest gc_cars/02_car_1

Actions per byte (written to <out>/<name>.swapmap, one byte per blob byte):
  1 W32   part of a 32-bit word to reverse (u32/s32/f32/pointer/reloc slot)
  2 H16   part of a 16-bit half to reverse
  3 KEEP  u8 field or string byte: copy as is
  4 GEOM  geometry (delegated: rebuilt by the geometry converter)
  5 TEX   texture headers/pixels/palettes (delegated)
Rules mirror Port_Swap.c Swapper_Finish: a word with no claim is a 32-bit
value; a half with a U16 claim, or two unclaimed bytes beside claimed ones,
is a 16-bit value.
"""
import sys, os, collections
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import classify
from walk import DEFAULT, PTR, U16, BYTES, U32, TEXREF, SNDREF, REFREL, GEOM, TEX, STR

W32, H16, KEEP, DGEOM, DTEX = 1, 2, 3, 4, 5
WORDISH = (PTR, U32, TEXREF, SNDREF, REFREL)


def plan(s):
    act = bytearray(s.size)
    c = s.claim
    for i in range(0, s.size - s.size % 4, 4):
        q = c[i:i + 4]
        if all(x == GEOM for x in q) or (q[0] == GEOM and all(x in (GEOM, DEFAULT) for x in q)):
            act[i:i + 4] = bytes([DGEOM]) * 4; continue
        if all(x == TEX for x in q):
            act[i:i + 4] = bytes([DTEX]) * 4; continue
        if all(x == DEFAULT for x in q) or q[0] in WORDISH:
            act[i:i + 4] = bytes([W32]) * 4; continue
        for h in (0, 2):
            a, b = q[h], q[h + 1]
            if a == U16 or (a == DEFAULT and b == DEFAULT):
                act[i + h:i + h + 2] = bytes([H16]) * 2
            else:
                for k in (0, 1):
                    x = q[h + k]
                    act[i + h + k] = {GEOM: DGEOM, TEX: DTEX}.get(x, KEEP)
    for i in range(s.size - s.size % 4, s.size):
        act[i] = KEEP
    return act


def apply(blob, act):
    out = bytearray(blob)
    n = len(blob)
    i = 0
    while i < n:
        a = act[i]
        if a == W32 and i % 4 == 0 and i + 4 <= n:
            out[i:i + 4] = blob[i:i + 4][::-1]; i += 4
        elif a == H16 and i % 2 == 0:
            out[i:i + 2] = blob[i:i + 2][::-1]; i += 2
        else:
            i += 1
    return out


def summary(d):
    e = "<" if "beta" in d else ">"
    r = classify(d, e); s = r["pack"]
    act = plan(s)
    cnt = collections.Counter(act)
    # how much of the W32 data is inside walker-typed objects (vs default rule)
    typed = sum(1 for i in range(s.size) if act[i] == W32 and s.label[i] and not s.label[i].startswith("ptd:") and not s.label[i].endswith("~"))
    tail = sum(1 for i in range(s.size) if act[i] == W32 and s.label[i] and s.label[i].endswith("~"))
    ptd = sum(1 for i in range(s.size) if act[i] == W32 and s.label[i] and s.label[i].startswith("ptd:"))
    obj = cnt[W32] + cnt[H16] + cnt[KEEP]
    print(f"{d}: {s.size} bytes")
    print(f"   delegated: geometry {cnt[DGEOM]} ({cnt[DGEOM]/s.size*100:.1f}%), texture {cnt[DTEX]} ({cnt[DTEX]/s.size*100:.1f}%)")
    print(f"   object data {obj} ({obj/s.size*100:.1f}%): W32 {cnt[W32]}, H16 {cnt[H16]}, KEEP(u8/str) {cnt[KEEP]}")
    print(f"   W32 bytes in walker-typed structs {typed} ({typed/max(1,cnt[W32])*100:.1f}%), "
          f"struct tails {tail} ({tail/max(1,cnt[W32])*100:.1f}%), reached-but-untyped {ptd} ({ptd/max(1,cnt[W32])*100:.1f}%)")
    return s, act


if __name__ == "__main__":
    if sys.argv[1] == "--selftest":
        # GC pack -> little-endian with the plan -> walk again as '<': claims must agree
        import tempfile, shutil, struct
        d = sys.argv[2]
        s, act = summary(d)
        le = apply(s.blob, act)
        tmp = tempfile.mkdtemp()
        for f in os.listdir(d):
            src = f"{d}/{f}"
            if f.endswith("relocs.bin") or f == "pointers.bin":
                b = open(src, "rb").read()
                n = len(b) // 4
                open(f"{tmp}/{f}", "wb").write(struct.pack(f"<{n}I", *struct.unpack(f">{n}I", b)))
            elif f == "data.bin":
                open(f"{tmp}/{f}", "wb").write(le)
            else:
                shutil.copy(src, tmp)
        r2 = classify(tmp, "<")["pack"]
        diff = sum(1 for i in range(s.size) if s.claim[i] != r2.claim[i])
        print("selftest: claim differences after GC->LE round trip:", diff)
        sys.exit(0)
    for d in sys.argv[1:]:
        s, act = summary(d)
        os.makedirs("research/out", exist_ok=True)
        open(f"research/out/{os.path.basename(d)}.swapmap", "wb").write(act)
