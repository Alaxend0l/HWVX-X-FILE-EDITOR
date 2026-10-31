#!/usr/bin/env python3
"""In-place patch of an extracted HW_IPack.xgc (GC, big endian) for 3 extra cars (ids 33-35).
usage: python3 patch_ipack.py <extracted HW_IPack dir>   (modifies data.bin files in that dir)
- every car-select interface (IMHeader id 2): carousel vars whose max == 32 -> 35
- car-name string list entries 33..35 (shared strings) -> new names
- unlock-hint string list entries 33..35 -> hint text
No sizes change, so pointers.bin / relocation tables stay valid."""
import sys, os, struct
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ifwalk import W
from strlists import lists

NEW_MAX = 35
NAMES = {33: 'Fandango*', 34: 'Sweet 16 II*', 35: 'Jester*'}
HINTS = {33: ' ', 34: ' ', 35: ' '}   # shown under a locked car; keep ' ' if they start unlocked

def put_str(blob, w, texts, k, s):
    slot = w.ptr(texts + 4*k); p = w.ptr(slot)
    end = w.b.index(b'\0', p); z = end
    while z < len(w.b) and w.b[z] == 0 and z not in w.p.ptrs: z += 1
    room = z - p - 1          # bytes available before the next non-zero / pointer word
    data = s.encode('latin1')
    assert len(data) <= room, (s, room)
    blob[p:p+room] = data + b'\0' * (room - len(data))

root = sys.argv[1]
for sub in sorted(os.listdir(root)):
    d = os.path.join(root, sub)
    if not os.path.isdir(d): continue
    w = W(d); w.headers(w.u32(w.u32(4)))
    blob = bytearray(w.b); changed = 0; done_strings = set()
    for h in sorted(w.seen):
        if w.u16(h) != 2: continue
        for v in w.vars(h):
            if v['type'] == 0 and v['min'] == 0 and v['max'] == 32:
                a = int(v['at'], 16) + 8
                struct.pack_into('>i', blob, a, NEW_MAX); changed += 1
        for (i, o, s, lst, cnt, texts, b8, var, names) in lists(w, h):
            if cnt != 36: continue
            table = NAMES if names[0].endswith('*') else HINTS
            for k, txt in table.items():
                key = w.ptr(w.ptr(texts + 4*k))
                if key in done_strings: continue
                put_str(blob, w, texts, k, txt); done_strings.add(key); changed += 1
    if changed:
        open(os.path.join(d, 'data.bin'), 'wb').write(blob)
        print(sub, 'patched', changed, 'fields')
