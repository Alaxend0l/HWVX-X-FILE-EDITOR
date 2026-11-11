#!/usr/bin/env python3
"""The releases' movies: container facts, and a frame-quality comparison of the shared ones.

    python3 movie_survey.py GC_FMV_DIR PC_FMV_DIR PS2_FMV_DIR [--frames N] [--scratch DIR]

Needs ffprobe/ffmpeg (THP, Bink and MPEG-2 decoders) and numpy. Per movie and release: codec,
size, frame rate, duration, bit rate (ffprobe). For movies all three releases have, N frames
spread over the movie are decoded at 640x480 grey (PS2's 720x480 is scaled to the 4:3 frame the
others show), each PC and PS2 frame aligned to the GameCube's by the best of +-6 frames around the
same time, and the three-cornered hat (as in tex_survey.py) gives each release's own RMSE; plus
the in-frame gradient energy (sharpness) relative to the median of the three.
"""

import json
import subprocess
import sys
from pathlib import Path

import numpy as np

W, H = 640, 480


def probe(path):
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries",
                          "format=duration,bit_rate:stream=codec_type,codec_name,width,height,avg_frame_rate,sample_rate,channels",
                          "-of", "json", str(path)], capture_output=True, text=True).stdout
    return json.loads(out)


def frames(path, t, n, fps):
    """n grey frames starting at time t, 640x480."""
    # output-side seek: decodes from the start, but the THP demuxer's input seek lands elsewhere
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), "-ss", f"{t:.3f}", "-frames:v", str(n),
                          "-vf", f"scale={W}:{H}:flags=bicubic,format=gray", "-f", "rawvideo", "-"],
                         capture_output=True).stdout
    k = len(raw) // (W * H)
    return np.frombuffer(raw[:k * W * H], np.uint8).reshape(k, H, W).astype(np.float32)


def grad(f):
    return float(np.abs(np.diff(f, axis=1)).mean() + np.abs(np.diff(f, axis=0)).mean())


def norm(name):
    return name.lower().split(".")[0]


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    nfr = int(sys.argv[sys.argv.index("--frames") + 1]) if "--frames" in sys.argv else 8
    rels = dict(zip(("gc", "pc", "ps2"), (Path(a) for a in args[:3])))
    files = {r: {norm(p.name): p for p in sorted(d.iterdir()) if p.is_file()} for r, d in rels.items()}
    names = sorted(set().union(*files.values()))
    print("## movies")
    info = {}
    for n in names:
        for r in rels:
            p = files[r].get(n)
            if not p:
                continue
            j = probe(p)
            v = next((s for s in j.get("streams", []) if s["codec_type"] == "video"), {})
            a = next((s for s in j.get("streams", []) if s["codec_type"] == "audio"), {})
            fmt = j.get("format", {})
            info[(n, r)] = float(fmt.get("duration", 0))
            print(f"  {n:9s} {r:3s} {v.get('codec_name', '?'):10s} {v.get('width')}x{v.get('height')} "
                  f"{eval(v.get('avg_frame_rate', '0/1')):.3f} fps {float(fmt.get('duration', 0)):7.2f} s "
                  f"{int(fmt.get('bit_rate', 0)) / 1e6:5.2f} Mbit/s {p.stat().st_size / 1e6:6.1f} MB  audio "
                  + (f"{a.get('codec_name')} {a.get('sample_rate')} Hz x{a.get('channels')}" if a else "not found by ffprobe"))
    print("\n## frame quality (three-cornered hat, grey 640x480; RMSE in 8-bit levels)")
    for n in names:
        if not all(n in files[r] for r in rels):
            continue
        dur = min(info[(n, r)] for r in rels)
        d2, gr = [], []
        for k in range(nfr):
            t = dur * (k + 1) / (nfr + 1)
            g = frames(files["gc"][n], t, 1, 30)
            if not len(g):
                continue
            g = g[0]
            best = {}
            for r in ("pc", "ps2"):
                fs = frames(files[r][n], max(0.0, t - 0.2), 13, 30)
                if not len(fs):
                    break
                errs = [float(((f - g) ** 2).mean()) for f in fs]
                best[r] = fs[int(np.argmin(errs))]
            if len(best) < 2:
                continue
            gp = ((g - best["pc"]) ** 2).mean()
            gs = ((g - best["ps2"]) ** 2).mean()
            ps = ((best["pc"] - best["ps2"]) ** 2).mean()
            if max(gp, gs, ps) > 400:      # a scene cut between the aligned frames
                continue
            d2.append([(gp + gs - ps) / 2, (gp + ps - gs) / 2, (gs + ps - gp) / 2])
            e = [grad(g), grad(best["pc"]), grad(best["ps2"])]
            if np.median(e) < 1:            # black or flat frame
                d2.pop()
                continue
            gr.append([x / float(np.median(e)) for x in e])
        if not d2:
            print(f"  {n}: no aligned frames")
            continue
        m = np.sqrt(np.maximum(np.mean(d2, 0), 0))
        gm = np.mean(gr, 0)
        print(f"  {n:9s} frames {len(d2):2d}  own RMSE gc {m[0]:5.2f} pc {m[1]:5.2f} ps2 {m[2]:5.2f}   "
              f"sharpness vs median gc {gm[0]:.3f} pc {gm[1]:.3f} ps2 {gm[2]:.3f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
