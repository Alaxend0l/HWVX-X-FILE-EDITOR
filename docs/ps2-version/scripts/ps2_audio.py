#!/usr/bin/env python3
"""Decode the PS2 release's sound effects and instrument banks, and compare them with the PC's.

    python3 -I ps2_audio.py PS2_PACK [--wav OUT_DIR]                  list (and decode) sounds / bank
    python3 -I ps2_audio.py --check PS2_TREE PC_TREE [PACK_SUBSTRING ...]

The PC's samples equal the GameCube's (PCM16, LE on PC), so the PC is the reference.
Rules (../README.md §3):
* SFX descriptor (0x20, LE): ptr data (sound_pointers slot), u32 length in bytes (VAG, a
  multiple of 16), u32 loop (byte offset of the loop-start block, or ~0), u32 rate,
  u16 0x1000 (SPU pitch 1.0), u16 volume, 12 zero bytes;
* samples are headerless PS2 SPU-ADPCM ("VAG" bodies): 16-byte blocks, 28 samples each;
* instrument banks are `RIFF` 'sndb' like the PC's, with u8 bits = 4 (ADPCM) at entry +0x11
  and the sample offsets/lengths/loops in VAG bytes;
* songs are `RIFX` 'MIDI', byte-identical to the other releases.
"""

import collections
import json
import math
import struct
import sys
import wave
from pathlib import Path

FILTERS = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)]


def vag_decode(buf, start=0, length=None):
    """SPU-ADPCM -> list of s16, plus the per-block flag bytes."""
    end = len(buf) if length is None else start + length
    out, flags = [], []
    s1 = s2 = 0
    for p in range(start, end - 15, 16):
        shift, filt = buf[p] & 15, buf[p] >> 4
        flags.append(buf[p + 1])
        f0, f1 = FILTERS[filt] if filt < 5 else (0, 0)
        for k in range(28):
            n = (buf[p + 2 + k // 2] >> (4 * (k & 1))) & 15
            if n >= 8:
                n -= 16
            s = ((n << 12) >> shift) if shift <= 12 else 0
            s += (s1 * f0 + s2 * f1 + 32) >> 6
            s = max(-32768, min(32767, s))
            out.append(s)
            s2, s1 = s1, s
    return out, flags


def load(folder):
    f = Path(folder)
    h = json.loads((f / "pack.json").read_text())["header"]
    sd = (f / "sound_data.bin").read_bytes() if (f / "sound_data.bin").exists() else b""
    return h, sd


def sfx(h, sd, be=False):
    e = ">" if be else "<"
    out = []
    for i in range(h.get("num_sounds", 0)):
        o = h["sfx_offset"] + 0x20 * i
        ptr, length, loop, rate = struct.unpack_from(e + "4I", sd, o)
        out.append({"i": i, "ptr": ptr, "len": length, "loop": loop, "rate": rate,
                    "b10": sd[o + 0x10], "b11": sd[o + 0x11], "w10": struct.unpack_from(e + "H", sd, o + 0x10)[0],
                    "vol": struct.unpack_from(e + "H", sd, o + 0x12)[0]})
    return out


def bank(sd, off):
    """A 'sndb' bank at `off`: (endian, header words, entries)."""
    tag = sd[off:off + 4]
    e = "<" if tag == b"RIFF" else ">"
    size, kind, version, count, table = struct.unpack_from(e + "I4sIII", sd, off + 4)
    if kind != b"sndb":
        raise ValueError("not a sndb bank")
    w18, w1c = struct.unpack_from(e + "II", sd, off + 0x18)
    ents = []
    for i in range(count):
        o = off + 0x20 + 0x20 * i
        so, sl, lp, rate = struct.unpack_from(e + "4I", sd, o)
        ents.append({"i": i, "off": so, "len": sl, "loop": lp, "rate": rate, "b10": sd[o + 0x10], "bits": sd[o + 0x11],
                     "w12": struct.unpack_from(e + "H", sd, o + 0x12)[0], "rest": sd[o + 0x14:o + 0x20]})
    data = off + table                 # the table size counts the 0x20-byte header
    return {"endian": e, "size": size, "version": version, "count": count, "table": table, "w18": w18, "w1c": w1c,
            "entries": ents, "data": data}


def ncc_best(a, b, max_lag=64):
    """Best normalised cross-correlation of a against b over small lags."""
    best = (-2.0, 0)
    n = min(len(a), len(b)) - max_lag
    if n < 64:
        return best
    for lag in range(-max_lag, max_lag + 1):
        sab = saa = sbb = 0
        for k in range(max_lag, max_lag + n, 2):
            x, y = a[k + lag] if 0 <= k + lag < len(a) else 0, b[k]
            sab += x * y
            saa += x * x
            sbb += y * y
        if saa and sbb:
            c = sab / math.sqrt(saa * sbb)
            if c > best[0]:
                best = (c, lag)
    return best


def resample(x, ratio, n):
    """Linear resampling: y[k] = x[k * ratio]."""
    out = []
    for k in range(n):
        t = k * ratio
        i = int(t)
        if i + 1 >= len(x):
            break
        f = t - i
        out.append(x[i] * (1 - f) + x[i + 1] * f)
    return out


def write_wav(path, samples, rate):
    w = wave.open(str(path), "wb")
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(rate)
    w.writeframes(struct.pack("<%dh" % len(samples), *samples))
    w.close()


def check(ps2_root, pc_root, filt):
    st = collections.Counter()
    nccs, nccb = [], []
    for pj in sorted(Path(ps2_root).rglob("pack.json")):
        rel = pj.parent.relative_to(ps2_root)
        if filt and not any(f in str(rel) for f in filt):
            continue
        hp, sp = load(pj.parent)
        if not sp:
            continue
        has_pc = (Path(pc_root) / rel / "pack.json").exists()
        hc, sc = load(Path(pc_root) / rel) if has_pc else ({}, b"")
        P, C = sfx(hp, sp), sfx(hc, sc) if has_pc else []
        st["packs with sounds"] += bool(P)
        st["sounds"] += len(P)
        for s in P:
            st["descriptor u16 0x10 = 0x%X" % s["w10"]] += 1
            st["length multiple of 16"] += s["len"] % 16 == 0
            pcm, flags = vag_decode(sp, s["ptr"], s["len"])
            st["block 0 silent"] += not any(sp[s["ptr"] + 2:s["ptr"] + 16])
            if s["loop"] == 0xFFFFFFFF:
                st["one-shot"] += 1
                st["one-shot: last block flag 1"] += flags[-1] & 1 == 1
            else:
                st["looping"] += 1
                lb = s["loop"] // 16
                st["looping: loop block flag 6 (start)"] += lb < len(flags) and flags[lb] & 4 == 4
                st["looping: last block flag 3"] += flags[-1] & 3 == 3
            if has_pc and C:
                # pair by index, or (content edits move sounds) by the best-correlating PC sound
                # whose length matches once the rate ratio is applied
                body = pcm[28:]                     # block 0 is 28 silent samples

                def score(c):
                    ratio = s["rate"] / c["rate"]
                    ref = list(struct.unpack_from("<%dh" % min(c["len"] // 2, 8000), sc, c["ptr"]))
                    y = resample(body, ratio, len(ref))
                    return ncc_best(y, ref[:len(y)], 8)[0]
                same = C[s["i"]] if s["i"] < len(C) else None
                best, cc = same, score(same) if same else -2
                if cc < 0.95:
                    for c in C:
                        if c is same or abs(len(body) - c["len"] // 2 * s["rate"] / c["rate"]) > 56:
                            continue
                        k = score(c)
                        if k > cc:
                            best, cc = c, k
                st["PC pair: same index" if best is same else "PC pair: other index"] += 1
                c = best
                st["PC pair: same loop kind"] += (c["loop"] == 0xFFFFFFFF) == (s["loop"] == 0xFFFFFFFF)
                st["PC pair: VAG samples - 28 == PC samples * rate ratio (within one block)"] += \
                    abs(len(body) - c["len"] // 2 * s["rate"] / c["rate"]) <= 28
                st["PC pair: same rate"] += s["rate"] == c["rate"]
                nccs.append(cc)
                st["PC pair: NCC >= 0.95" if cc >= 0.95 else "PC pair: NCC 0.8-0.95" if cc >= 0.8 else "PC pair: NCC < 0.8"] += 1
        bo = hp.get("song_bank_offset", 0)
        if bo and sp[bo:bo + 4] in (b"RIFF", b"RIFX"):
            b = bank(sp, bo)
            st["banks"] += 1
            st["bank entries"] += b["count"]
            st["bank header 0x18/0x1C = 0x%X/0x%X" % (b["w18"], b["w1c"])] += 1
            co = hc.get("song_bank_offset", 0) if has_pc else 0
            if has_pc and co and sc[co:co + 4] == b"RIFF":
                cb = bank(sc, co)
                if not hp.get("num_sounds"):
                    st["bank: song bytes equal to PC (music packs)"] += sp[:bo] == sc[:co]
                st["bank: same instrument count as PC"] += b["count"] == cb["count"]
                for e, f in zip(b["entries"], cb["entries"]):
                    st["bank entry bits %d (PC %d)" % (e["bits"], f["bits"])] += 1
                    st["bank entry rate equal to PC"] += e["rate"] == f["rate"]
                    st["bank entry key range, envelope equal to PC"] += e["rest"] == f["rest"]
                    ratio = e["rate"] / f["rate"]
                    st["bank entry: VAG samples - 28 == PC samples * rate ratio (within 2 blocks)"] += \
                        abs(e["len"] // 16 * 28 - 28 - f["len"] // 2 * ratio) < 56
                    if e["loop"] != 0xFFFFFFFF:
                        st["bank loop: (block - 1) * 28 == PC loop sample * rate ratio (within 2 blocks)"] += \
                            abs((e["loop"] // 16 - 1) * 28 - f["loop"] // 2 * ratio) < 56
                    else:
                        st["bank one-shot (PC too)"] += f["loop"] == 0xFFFFFFFF
                    pcm, _ = vag_decode(sp, b["data"] + e["off"], e["len"])
                    ref = list(struct.unpack_from("<%dh" % min(f["len"] // 2, 20000), sc, cb["data"] + f["off"]))
                    y = resample(pcm[28:], ratio, min(len(ref), 8000))
                    cc, lag = ncc_best(y, ref[:len(y)], 8)
                    nccb.append(cc)
                    st["bank NCC >= 0.95" if cc >= 0.95 else "bank NCC 0.8-0.95" if cc >= 0.8 else "bank NCC < 0.8"] += 1
    for k in sorted(st):
        print(f"  {k}: {st[k]}")
    if nccs:
        nccs.sort()
        print(f"  SFX NCC median {nccs[len(nccs) // 2]:.4f}, 10th percentile {nccs[len(nccs) // 10]:.4f}")
    if nccb:
        nccb.sort()
        print(f"  bank NCC median {nccb[len(nccb) // 2]:.4f}, 10th percentile {nccb[len(nccb) // 10]:.4f}, min {nccb[0]:.4f}")


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if "--check" in sys.argv:
        check(Path(args[0]), Path(args[1]), args[2:])
        return 0
    out = None
    if "--wav" in sys.argv:
        out = Path(args[1])
        out.mkdir(parents=True, exist_ok=True)
    h, sd = load(args[0])
    for s in sfx(h, sd):
        pcm, flags = vag_decode(sd, s["ptr"], s["len"])
        print(f"sfx {s['i']}: {s['len']} B VAG, {len(pcm)} samples @ {s['rate']} Hz, loop "
              f"{'none' if s['loop'] == 0xFFFFFFFF else s['loop']}, 0x10 = 0x{s['w10']:X}, vol 0x{s['vol']:X}")
        if out:
            write_wav(out / f"sfx{s['i']}.wav", pcm, s["rate"])
    bo = h.get("song_bank_offset", 0)
    if bo and sd[bo:bo + 4] in (b"RIFF", b"RIFX"):
        b = bank(sd, bo)
        print(f"bank @0x{bo:X}: {b['count']} instruments, version 0x{b['version']:X}, 0x18 0x{b['w18']:X}, 0x1C 0x{b['w1c']:X}")
        for e in b["entries"]:
            print(f"  {e['i']}: off 0x{e['off']:X} len {e['len']} loop {e['loop']:#x} rate {e['rate']} bits {e['bits']}")
            if out:
                pcm, _ = vag_decode(sd, b["data"] + e["off"], e["len"])
                write_wav(out / f"bank{e['i']}.wav", pcm, e["rate"])
    return 0


if __name__ == "__main__":
    sys.exit(main())
