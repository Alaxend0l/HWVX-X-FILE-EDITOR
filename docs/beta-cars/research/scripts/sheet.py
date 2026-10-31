import sys, glob, os
from PIL import Image, ImageDraw
d = sys.argv[1]; out = sys.argv[2]
rows = []
for tag in ("b", "g"):
    fs = sorted(glob.glob(f"{d}/{tag}*.png"))
    rows.append(fs)
n = max(len(r) for r in rows); S = 64
img = Image.new("RGB", (n * (S + 4), len(rows) * (S + 14)), (40, 40, 40))
dr = ImageDraw.Draw(img)
for y, r in enumerate(rows):
    for x, f in enumerate(r):
        t = Image.open(f).convert("RGBA").resize((S, S))
        bg = Image.new("RGBA", (S, S), (255, 0, 255, 255)); bg.alpha_composite(t)
        img.paste(bg.convert("RGB"), (x * (S + 4), y * (S + 14) + 12))
        dr.text((x * (S + 4), y * (S + 14)), os.path.basename(f)[:-4], fill=(255, 255, 255))
img.save(out)
