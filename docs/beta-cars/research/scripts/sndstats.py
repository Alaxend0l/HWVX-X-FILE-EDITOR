# survey SFX descriptors in all car packs (beta LE / GC BE)
import glob, os, json, struct, collections
def descs(d, e):
    j = json.load(open(d + '/pack.json'))['header']
    if not j['num_sounds']: return j, None, []
    sd = open(d + '/sound_data.bin', 'rb').read()
    sp = open(d + '/sound_pointers.bin', 'rb').read()
    ptrs = struct.unpack(e + f'{len(sp)//4}I', sp)
    out = []
    for i in range(j['num_sounds']):
        o = j['sfx_offset'] + 32 * i
        data, ln, loop, rate = struct.unpack_from(e + '4I', sd, o)
        b10, b11 = sd[o + 0x10], sd[o + 0x11]
        h10 = struct.unpack_from(e + 'H', sd, o + 0x10)[0]
        vol = struct.unpack_from(e + 'H', sd, o + 0x12)[0]
        pad = sd[o + 0x14:o + 0x20]
        out.append(dict(i=i, data=data, len=ln, loop=loop, rate=rate, b10=b10, b11=b11, h10=h10, vol=vol, padnz=any(pad)))
    return j, (sd, ptrs), out
if __name__ == '__main__':
    for tag, e in (('gc_cars', '>'), ('beta_cars', '<')):
        c = collections.Counter(); print('==', tag)
        for d in sorted(glob.glob(f'../{tag}/*_*')):
            j, x, ds = descs(d, e)
            if not ds: continue
            sd, ptrs = x
            first = min(t['data'] for t in ds)
            print(os.path.basename(d), 'n', len(ds), 'sdlen', len(sd), 'ptrs', ptrs, 'song', j['song_offset'], j['song_bank_offset'], 'firstdata', hex(first),
                  'ends', [hex(t['data'] + t['len']) for t in ds][-1:], 'nz 0x%x..first' % (32 * len(ds)), sum(1 for b in sd[32 * len(ds):first] if b))
            for t in ds:
                c[('bits/h10', t['b10'], t['b11'], t['h10'], 'vol', t['vol'], 'pad', t['padnz'])] += 1
                c[('rate', t['rate'])] += 1
                c[('loop', 'none' if t['loop'] == 0xFFFFFFFF else ('0' if t['loop'] == 0 else 'mid'))] += 1
                if e == '<': c[('len%16', t['len'] % 16, 'data%16', t['data'] % 16)] += 1
                else: c[('len%2', t['len'] % 2, 'data%32', t['data'] % 32)] += 1
        for k, v in sorted(c.items(), key=str): print(' ', k, v)
