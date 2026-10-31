import sys, bisect
sys.path.insert(0, __file__.rsplit("/", 1)[0])
from classify import *
r = classify(sys.argv[1], sys.argv[2]); s = r["pack"]
ins = sorted((s.word(o), o) for o in s.pointers if s.word(o) < s.size)
tk = [t for t, o in ins]
def lr(o):
    if s.label[o] is None: return "?"
    st = max(x for x in s.objs if x <= o); return f"{s.objs[st]}+{o-st:X}"
for a, n, _ in r["unknown"]:
    i = bisect.bisect_left(tk, a); j = bisect.bisect_left(tk, a + n)
    srcs = [(t, o) for t, o in ins[i:j]]
    print(f"{a:06X} +{n:X}: {len(srcs)} refs in; first: " + ", ".join(f"{t:X}<-{o:X}[{lr(o)}]" for t, o in srcs[:4]))
