# PS2 SPU ADPCM ("VAG", headerless) decoder + GC PCM16BE reader + WAV writer
import struct, wave
F = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)]
def vag_decode(buf, stop_at_end=True):
    out = []; s1 = s2 = 0; flags = []
    for blk in range(len(buf) // 16):
        b = buf[blk * 16:blk * 16 + 16]
        pf, fl = b[0], b[1]; sh = pf & 15; f = pf >> 4
        flags.append(fl)
        if f > 4: f = 0
        f0, f1 = F[f]
        for k in range(28):
            byte = b[2 + k // 2]; n = (byte >> (4 * (k & 1))) & 15
            if n >= 8: n -= 16
            s = (n << 12) >> sh if sh <= 12 else 0
            s = s + ((s1 * f0 + s2 * f1 + 32) >> 6)
            s = max(-32768, min(32767, s))
            out.append(s); s2, s1 = s1, s
        if stop_at_end and (fl & 1) and blk != 0 and False: break
    return out, flags
def gc_pcm16(buf): return list(struct.unpack(f'>{len(buf)//2}h', buf[:len(buf)//2*2]))
def wav(path, samples, rate):
    w = wave.open(path, 'wb'); w.setnchannels(1); w.setsampwidth(2); w.setframerate(rate)
    w.writeframes(struct.pack(f'<{len(samples)}h', *samples)); w.close()
