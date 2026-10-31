import sys
from PIL import Image, ImageDraw
# rows: "b:dir:idx g:dir:idx g:dir:idx ..."
S = 72; rows = [r.split() for r in sys.argv[2:]]
n = max(len(r) for r in rows)
img = Image.new("RGB", (n * (S + 6), len(rows) * (S + 14)), (40, 40, 40)); dr = ImageDraw.Draw(img)
for y, r in enumerate(rows):
    for x, it in enumerate(r):
        tag, d, i = it.split(":")
        try: t = Image.open(f"{d}/{tag}{int(i):03d}.png").convert("RGBA").resize((S, S))
        except Exception: continue
        bg = Image.new("RGBA", (S, S), (255, 0, 255, 255)); bg.alpha_composite(t)
        img.paste(bg.convert("RGB"), (x * (S + 6), y * (S + 14) + 12))
        dr.text((x * (S + 6), y * (S + 14)), f"{tag}{d[-5:]}{i}", fill=(255, 255, 255))
img.save(sys.argv[1])
