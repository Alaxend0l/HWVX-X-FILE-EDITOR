# compare PC vs GC sound descriptors and samples
import struct, glob, os, collections
import numpy as np
SCR = 'SCRATCH'
def sounds(d, e):
    sd = open(d+'/sound_data.bin', 'rb').read(); sp = open(d+'/sound_pointers.bin', 'rb').read()
    import json; h = json.load(open(d+'/pack.json'))['header']
    n = h['num_sounds']; base = h['sfx_offset']; out = []
    for i in range(n):
        o = base + 0x20*i
        ptr, ln, loop, rate = struct.unpack_from(e+'4I', sd, o); u10, bits = sd[o+0x10], sd[o+0x11]
        vol = struct.unpack_from(e+'H', sd, o+0x12)[0]
        out.append(dict(ptr=ptr, len=ln, loop=loop, rate=rate, u10=u10, bits=bits, vol=vol, pad=sd[o+0x14:o+0x20], raw=sd[ptr:ptr+ln]))
    return out, sd, sp
st = collections.Counter()
for d in sorted(glob.glob(SCR+'/pc_cars/*')):
    n = os.path.basename(d)
    if n.startswith('man'): continue
    P, psd, psp = sounds(d, '<'); G, gsd, gsp = sounds(SCR+'/gc_cars/'+n, '>')
    st['nsounds same', len(P) == len(G)] += 1
    st['sound_pointers same', struct.unpack(f'<{len(psp)//4}I', psp) == struct.unpack(f'>{len(gsp)//4}I', gsp)] += 1
    if P: st['first sample off same', P[0]['ptr'] == G[0]['ptr']] += 1
    for i, (p, g) in enumerate(zip(P, G)):
        for k in ('len', 'loop', 'rate', 'u10', 'bits', 'vol', 'pad'):
            st[(k, 'same', p[k] == g[k])] += 1
        st[('pc bits', p['bits'])] += 1
        st[('pc ptr align', 32 if p['ptr'] % 32 == 0 else 16 if p['ptr'] % 16 == 0 else p['ptr'] % 16)] += 1
        if i: st['pc ptr == roundup16(prev end)', p['ptr'] == (P[i-1]['ptr']+P[i-1]['len']+15) & ~15] += 1
        if i: st['gc ptr == roundup32(prev end)', g['ptr'] == (G[i-1]['ptr']+G[i-1]['len']+31) & ~31] += 1
        a = np.frombuffer(p['raw'], '<i2'); b = np.frombuffer(g['raw'], '>i2')
        st['samples identical (PC LE == GC BE)', a.shape == b.shape and (a == b).all()] += 1
    # tail after last sample
    if P:
        e = P[-1]['ptr']+P[-1]['len']; st[('pc sound_data tail after last sample', len(psd)-e)] += 1
        e = G[-1]['ptr']+G[-1]['len']; st[('gc sound_data tail after last sample', len(gsd)-e)] += 1
        # zero gap between descriptors and first sample?
        st['pc gap zero', psd[0x20*len(P):P[0]['ptr']] == bytes(P[0]['ptr']-0x20*len(P))] += 1
        st['gc gap == pc gap', psd[0x20*len(P):P[0]['ptr']] == gsd[0x20*len(G):G[0]['ptr']]] += 1
        st['pc inter-sample pad zero', all(psd[x['ptr']+x['len']:((x['ptr']+x['len']+15)&~15)] == bytes(((x['ptr']+x['len']+15)&~15)-x['ptr']-x['len']) for x in P)] += 1
for k, c in sorted(st.items(), key=str): print(c, k)
