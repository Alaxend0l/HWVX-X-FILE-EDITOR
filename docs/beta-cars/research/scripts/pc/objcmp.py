# byte-class comparison of a PC/GC pair outside geometry & textures
import sys, json
sys.path.insert(0, '.')
from footprint import *
from pctex import pc_total
from texlib import gc_level_size
def gc_tsize(fmt, w, h, m):
    n = 1 if fmt == 1 else max(1, m); s = 0
    for _ in range(n): s += gc_level_size(fmt, w, h); w //= 2; h //= 2
    return s
def run(name, verbose=True):
    pc = PC(SCR+'/pc_cars/'+name); gc = GC(SCR+'/gc_cars/'+name)
    ph = pc.p.json['header']; gh = gc.p.json['header']
    PR = pc_geo_ranges(pc) + tex_ranges(pc.B, ph, '<', 0x10, lambda f, w, h, m: pc_total(dict(fmt=f, w=w, h=h, mips=m)))
    GR = gc_geo_ranges(gc) + tex_ranges(gc.B, gh, '>', 0x20, gc_tsize)
    pch, _ = others(pc.B, PR, 16); gch, _ = others(gc.B, GR, 32)
    # drop all-zero chunks (alignment padding)
    pnz = [(a, b) for a, b in pch if any(pc.B[a:b])]; gnz = [(a, b) for a, b in gch if any(gc.B[a:b])]
    # trim leading/trailing zeros
    def trim(B, a, b):
        while B[a] == 0: a += 1
        while B[b-1] == 0: b -= 1
        return a, b
    pnz = [trim(pc.B, a, b) for a, b in pnz]; gnz = [trim(gc.B, a, b) for a, b in gnz]
    byt = sum(b-a for a, b in pnz), sum(b-a for a, b in gnz)
    if verbose: print(name, 'non-geo/tex non-zero chunks PC', len(pnz), 'GC', len(gnz), 'bytes', byt)
    return pc, gc, pnz, gnz
if __name__ == '__main__':
    pc, gc, pnz, gnz = run(sys.argv[1] if len(sys.argv) > 1 else '01_car_0')
    for i in range(max(len(pnz), len(gnz))):
        p = pnz[i] if i < len(pnz) else None; g = gnz[i] if i < len(gnz) else None
        print(i, 'PC', p and (hex(p[0]), p[1]-p[0]), 'GC', g and (hex(g[0]), g[1]-g[0]))
