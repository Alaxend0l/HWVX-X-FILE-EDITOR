# The "ultimate" build: which release has the best copy of each asset

Measurements behind the choice an `.xdl` compiler ([`README.md`](README.md)) makes when the player
owns more than one release: for every asset, take the copy that carries the most of the original
art. Releases: GameCube (`orig/gc/files`, `.xgc`), PC (the CD as the HWVX-SDL launcher imports it,
`.xpc`), PS2 retail (`orig/ps2/XPS/ENGLISH`, `.xps`); the PS2 prototype only for content it alone
has (§5). Scripts in [`scripts/`](scripts/) (§7); all numbers below come from them.

There is no ground truth (the source art is not available), so quality is measured with proxies:
resolution, lossless vs lossy storage, error against a lossless copy where one release has one,
and, where every copy is lossy, each copy's own error estimated from the three pairwise
differences (three-cornered hat, §1.4).

## Recommendation

| asset | source | rule |
|---|---|---|
| texture, PS2 stored it PSMCT32 (245) | **PS2** | lossless |
| texture, GameCube stored it RGBA8, PS2 PSMT8 (836) | **PS2** (PC look) / **GameCube** (GameCube look) | PS2 PSMT8 beats PC DXT5 against the lossless GameCube copy in 717 of 836 textures (PC 32, tie 87); GameCube RGBA8 is exact in its own colour space |
| texture with alpha, otherwise (GC CMPR / PC DXT5 / PS2 PSMT8, 2,969) | **PS2**, but **PC** if the DXT5 copy has > 1024 colours | PS2 own RGB error 4.6 (cars), 1.1 (items), 5.4 (worlds) vs PC 7.3–9.2; the > 1024-colour ones are HW_brief's portrait frames, where PC wins (8.5 vs 11.8). GameCube CMPR alpha is 1-bit: alpha error 18 vs PC 2.3, PS2 0.9 |
| opaque texture (GC CMPR / PC DXT1 / PS2 PSMT8, 6,730) | **PC** (PC look) / **GameCube** (GameCube look) | own RGB error PC 3.5, PS2 4.2, GC 7.6 in the PC colour space; GC 3.3, PC 3.7, PS2 3.7 in the GameCube's. PC DXT1 passes through to BC1 unchanged, GameCube CMPR re-tiles to BC1 unchanged |
| texture in one release only | that release | controller/keyboard glyphs per platform (pick by input device), logo variants, PS2 env maps (§1.7) |
| resolution, mip chains | any | no texture changes size between releases; mips are regenerated (GameCube chains stop at 16×16, PC/PS2 go to 1×1) |
| geometry | **GameCube** (= PS2 triangles, full colour precision) | identical LOD counts, attributes and skinning everywhere; PS2 loses 1,103 triangles, 1 colour bit and 2 alpha bits; PC differs only in content edits (car_21 better LODs, car_25 coarser body, world props: §2) |
| sound effects, briefing voices, instrument banks | **GameCube** | PCM16, a superset of the PC's (byte-identical samples), PS2 is the same audio as 4-bit ADPCM |
| music (MIDI songs) | GameCube (or PS2) | byte-identical on all three except one song the PC edited (7 world-pack copies, 31224 vs 31218 bytes; [`pairing.md`](pairing.md)) |
| movies, video | **GameCube** THP | 9.3–9.6 Mbit/s MJPEG, lowest own error (0.6–2.1 levels vs PC 1.0–2.4, PS2 3.2–5.5) |
| movies, audio | **PS2** (from the header: PCM s16 48 kHz) | THP is 4-bit ADPCM, Bink is lossy DCT; PS2 PCM not decoded here (§4) |
| movies only one release has | PC `hw_gal`, `hw_mark`; PS2's longer `hwboot` | §4 |
| race*l1 (multi-player drag worlds) | GameCube | PC lacks the six archives; PS2 has them too |
| menus, and their textures (HW_IPack, mcp, HW_brief) | per release (the base's own) | data, not quality; see [`../menus/README.md`](../menus/README.md). Until 2026-11-12 the texture rule swapped about 2,000 of them for the PC's and PS2's copies, which changed the GameCube menus' look |

Texture rule as code: `tex_survey.rule()`. Scored against the per-texture hat on the 10,780
textures all three releases have (PC look): its picks average an own error of **5.00** (RMSE over
all triples), all-PC **5.65**, the per-texture best (which a compiler owning all three releases can
compute itself with the same hat) **4.58**.

---------------------------------------------------------------------------------------------
## 1. Textures

### 1.1 Pairing

`tex_survey.py` decodes the top level of every texture of every pack (GameCube CMPR/RGBA8, PC
DXT1/DXT5, PS2 PSMT8/PSMCT32) into the PC/PS2 colour space (GameCube RGB → `sqrt(255·v)`,
[`../ps2-version/README.md`](../ps2-version/README.md) §2.2), and pairs textures per pack by content: 8×8 thumbnails of
premultiplied RGB (transparent texels' colour differs freely between releases), greedy, accepted
under 24 levels. 12,154 groups: 10,780 in all three releases, 1,016 in two, 687 in one. Thumbnail
distance median 1.8 (PC), 2.0 (PS2).

| category | GC+PC+PS2 | two releases | one release | packs in 2+ releases, texture in one |
|---|---|---|---|---|
| cars | 169 | 4 | 6 | PS2 6 |
| interface (HW_IPack, mcp, HW_brief) | 2,486 | 22 | 39 | GC 11, PC 4, PS2 24 |
| items/share | 165 | 2 | 32 | GC 14, PS2 18 |
| worlds | 7,960 | 660 (555 GC+PS2: race*l1) | 609 | GC 196, PC 64, PS2 349 |

### 1.2 Formats, sizes, mips

| GameCube | PC | PS2 | triples |
|---|---|---|---|
| CMPR | DXT1 | PSMT8 | 6,730 |
| CMPR | DXT5 | PSMT8 | 2,969 |
| RGBA8 | DXT5 | PSMT8 | 836 |
| RGBA8 | DXT5 | PSMCT32 | 245 |

* **Size:** equal in 10,774 of 10,780 triples. The 6 others are 3 pairs of look-alike textures
  swapped by the pairing (sizes per pack match as multisets). No release has a higher-resolution
  copy of anything.
* **Mips:** PC and PS2 have the same level count everywhere (full chain to 1×1). GameCube chains
  stop at 16×16 (fewer levels in 8,004 triples, equal in 2,768). Mips are rebuilt by the compiler
  either way.
* PS2 PSMCT32 textures are near-flat (unique colours: median 18 in worlds, 1 in interface); the
  PS2 kept them raw for their alpha.

### 1.3 Against a lossless copy

Where one release is lossless, the others' RGB RMSE against it (texels with alpha ≥ 128; GameCube
RGBA8 references are compared in the GameCube's squared space, where they are exact):

| reference | category | PC | PS2 | GameCube | head-to-head |
|---|---|---|---|---|---|
| GC RGBA8 | worlds | DXT5 median 8.25 (n 645) | PSMT8 **2.28** (581) | — | PS2 466, PC 19, tie 68 |
| GC RGBA8 | interface | DXT5 10.23 (277) | PSMT8 **3.40** (271) | — | PS2 238, PC 13, tie 15 |
| GC RGBA8 | cars | DXT5 4.89 (8) | PSMT8 **2.23** (8) | — | PS2 6, tie 2 |
| GC RGBA8 | items/share | DXT5 11.49 (10) | PSMT8 **3.30** (9) | — | PS2 7, tie 2 |
| PS2 PSMCT32 | worlds | DXT5 median 0.00, mean 2.00, p90 7.99 | — | RGBA8 1.00, mean 1.87 | GC 51, PC 153 (flat textures) |
| PS2 PSMCT32 | interface | DXT5 0.00, mean 1.30 | — | RGBA8 0.00, mean 0.63 | GC 8, PC 9, tie 19 |

So for the textures the GameCube kept uncompressed (its alpha textures), the PS2's 256-colour
palette is about 3× closer to the original than the PC's DXT5.

### 1.4 All copies lossy: three-cornered hat

With three independent copies of one image, `d(a,b)² = sₐ² + s_b²` per pair, so each copy's own
error `s` follows from the three pairwise RMSEs. Per texture, then `sqrt(mean s²)` per class
(triples whose pairs all differ by < 30, i.e. no content edit):

| class (GC/PC/PS2) | n | RGB, PC space: GC / PC / PS2 | RGB, GameCube space | alpha |
|---|---|---|---|---|
| CMPR/DXT1/PSMT8, all | 6,723 | 7.60 / **3.54** / 4.22 | **3.30** / 3.70 / 3.70 | 0 / 0 / 0 |
| — worlds | 6,349 | 7.55 / **3.52** / 4.23 | **3.25** / 3.65 / 3.71 | |
| — cars | 114 | 7.56 / 3.87 / **3.51** | 3.69 / 4.20 / **3.30** | |
| — interface | 129 | 7.62 / **3.23** / 4.40 | **3.25** / 3.39 / 3.91 | |
| CMPR/DXT5/PSMT8, HW_brief | 2,025 | 14.02 / **8.50** / 11.84 | **7.18** / 9.39 / 11.09 | 18.26 / 2.49 / **1.09** |
| — worlds | 802 | 9.36 / 7.30 / **5.36** | 5.53 / 8.20 / 5.57 | 18.61 / 1.27 / **0.00** |
| — cars | 44 | 11.45 / 8.99 / **4.56** | 6.83 / 11.09 / **4.54** | 20.31 / 2.08 / **0.00** |
| — items/share | 47 | 8.66 / 9.17 / **1.05** | 7.88 / 11.88 / **2.14** | 19.20 / 4.57 / **0.00** |
| RGBA8/DXT5/PSMT8 | 808 | 4.75 / 8.37 / **2.37** | 2.88 / 9.30 / **2.44** | 3.15 / 2.07 / **0.49** |
| RGBA8/DXT5/PSMCT32 (check) | 243 | 2.05 / 3.75 / **0.89** | 1.65 / 2.51 / **0.29** | 0 / 2.41 / **0** |

* **Check of the method:** on the PSMCT32 class the PS2 copy is lossless and the hat gives it
  0.29–0.89; on RGBA8/DXT5/PSMT8 it ranks PS2 < PC as the direct comparison of §1.3 does.
* **Bias:** PC DXT and GameCube CMPR are both 4×4 block codecs and probably err alike on the same
  blocks; correlated errors read as signal, so the hat flatters PC and GameCube against PS2. PS2's
  numbers are conservative.
* **Colour space decides the GameCube:** in the PC look its error doubles (7.6 vs 3.3) because
  `sqrt` of an 8-bit squared value loses the darks (PC 0–11 all become GameCube 0). In its own
  look the GameCube's CMPR is the best opaque copy, narrowly.
* Per texture, opaque class, PC space: PC lowest by ≥ 2 in s² in 3,097, PS2 1,495, GC 52, tie 2,079.
  Predictors that don't help much: size, colour count (PC's colours ≤ 256: PC 2.74 vs PS2 2.71;
  > 1024: 4.43 vs 5.70).

### 1.5 Detail and banding

* **Unique colours** (same-size triples): PS2 PSMT8 ≤ 256 by format (median 106 in worlds, 88
  cars, 255 interface); in 7,854 of its 10,529 textures the PC copy has more than 256 colours
  (median 525 for DXT1 in worlds), i.e. the palette merged colours (banding), mostly in the
  interface (2,338 of 2,450).
* **In-block gradient energy** (mean |luma gradient| between neighbours inside a 4×4 block, so
  DXT/CMPR block seams don't count), relative to the median of the copies: PSMT8 1.06–1.12,
  CMPR 0.96–0.97, DXT 0.98–1.00. Block codecs smooth inside a block (4 colours on a line); the
  palette keeps per-texel variation, part of it quantisation noise. This agrees with PS2 winning on
  detailed alpha textures and losing on smooth many-colour ones (briefing portraits).

### 1.6 The rule

`tex_survey.rule(group, look)`: single-release → that release; size differs → largest; PS2
PSMCT32 → PS2; GameCube RGBA8 → PS2 (PC look) or GameCube (GameCube look); alpha (any copy
with alpha < 255, or PC DXT5) → PS2, unless the PC copy has > 1024 colours → PC; opaque → PC (PC
look) or GameCube (GameCube look).

| category | PC look: GC / PC / PS2 | GameCube look: GC / PC / PS2 |
|---|---|---|
| cars | 0 / 138 / 41 | 125 / 21 / 33 |
| interface | 26 / 2,161 / 360 | 454 / 2,004 / 89 |
| items/share | 16 / 115 / 68 | 128 / 12 / 59 |
| worlds | 737 / 6,636 / 1,856 | 7,667 / 287 / 1,275 |

PC look, on the triples: rule 5.00, all-PC 5.65, per-texture best 4.58 (own RGB RMSE). GameCube
look, RGB only: rule 5.27, all-GameCube 4.48, best 3.86 — the rule loses there because it moves
alpha textures off the GameCube for their 1-bit alpha, which an RGB score doesn't see.

**Storage.** PC DXT copies go into `.xdl` BC1/BC3 bit for bit, GameCube CMPR into BC1 bit for bit
(GameCube look only; the PC look needs decode, `sqrt`, re-encode, which is where the GameCube
loses). A PS2 copy has to be stored as RGBA8 (or palette-expanded at load) to keep its advantage:
re-encoding it to BC3 adds block error of the same kind the PC's DXT5 already has (not measured).

### 1.7 Single-release textures

Mostly per-world-pack duplicates. Distinct images: PS2 22 (397 copies), GameCube 15 (221), PC 10 (68).

* **Platform glyphs**: DualShock buttons and pad (PS2, ×62/×59/×59 in battle packs), GameCube
  pad and buttons (×45/×17/×14), keyboard keys (PC ×55). A build picks by input device.
* **Hot Wheels logo**: the PS2's chrome-blue logo (×56/×9) vs the GameCube's flame logo (×7).
* **Item ring**: blue on PS2, yellow on GameCube (hw_items, ×13 each).
* **PS2 env maps and extras**: gradients and env maps the PS2 gives env-mapped groups (2,290
  groups, [`../ps2-version/README.md`](../ps2-version/README.md) §2.2), 4 extra 256×256 car textures (car_3, car_7,
  car_20) and a 32×32 env map in car_1; not checked whether any group references the extra car
  textures.
* **PC world edits**: new textures in bat3_l6 and race2l2 (go with the PC-only props of §2), a
  w5_tex sign.
* race*l1 textures (555) exist on GameCube and PS2 only, since the PC lacks those archives.

---------------------------------------------------------------------------------------------
## 2. Geometry

`geom_survey.py`: geometries found by each release's header scan, paired within a pack by
bounding box (bit-equal across releases unless edited), per drawn level: groups, triangles,
distinct positions, vertices, normals/colours present, skinned.

| | GameCube | PC | PS2 |
|---|---|---|---|
| geometries | 21,652 | 20,467 | 21,652 |
| drawn / shadow levels | 22,855 / 33 | 21,670 / 33 | 22,855 / 33 |
| levels with normals / colours | 7,461 / 16,494 | 7,097 / 15,641 | 7,461 / 16,494 |
| skinned | 120 | 106 | 120 |
| triangles | 1,630,985 | 1,588,354 | 1,629,882 |
| triangles outside race*l1 | 1,593,124 | 1,588,354 | |

* **No release has extra LODs, attributes or skinning**: in every paired geometry (GC–PC 20,380,
  GC–PS2 21,652) the level count, level kinds, normal/colour presence and skinning are equal. The
  PC's lower totals are the six race*l1 archives (14 skinned, 37,861 triangles).
* **PS2 vs GameCube**: distinct-position counts equal except in 17 levels; 93 levels have fewer triangles
  on PS2 (−1,103; cars 24 levels −84 in car_0/3/7/22/23/32, worlds 69 −1,019 in battle and race
  packs). Vertex colours lose a bit (RGB `gc>>1`, alpha `gc>>2`), 0.36% of triangles stay
  wound the wrong way after re-orienting (§1 of the PS2 notes). Strictly worse than GameCube.
* **PC vs GameCube** (content edits, not encoding):
  * car_21: four geometries (wheels) with denser LOD 1/2 on PC (282/256/120 vs 282/120/36 triangles);
  * car_25: body LOD 0/1 coarser on PC (549/533 vs 857/831);
  * car_26: GameCube has one geometry more (323 triangles);
  * world props: PC has 10–17 other small geometries in bat3_l6, race2l2, world3 (its new textures,
    §1.7); world4 has 11 GameCube/PS2 geometries the PC lacks; ~10 quadrant levels in w2/w3/w5
    geo packs with a few triangles more on PC.
* Recommendation: GameCube as the base, the PC's car_21 wheel LODs as the one detail gain a
  build could take. The world edits are design changes, not quality; they belong to a "PC
  content" option rather than to the best-quality choice.

---------------------------------------------------------------------------------------------
## 3. Audio

`audio_survey.py --content`:

| | GameCube | PC | PS2 |
|---|---|---|---|
| sound slots (all packs) | 4,723 | 4,347 | 4,723 |
| seconds | 4,770 | 4,491 | 4,774 |
| SFX format | PCM16 BE | PCM16 LE | SPU-ADPCM 4-bit |
| SFX rates | 22050 (3,328), 11025 (1,258), 8000 (91), 44100 (46) | the same | the same, 29 distinct (resampled for block-aligned loops: 11029, 22104, …) |
| instrument banks / entries | 60 / 1,492 | 54 / 1,370 | 60 / 1,492 (4-bit, 71 distinct rates) |

* **GameCube = PC, byte for byte.** 4,723 slots hold only 419 distinct samples; 418 of them are in
  the PC release (4,345 in the same pack, 7 elsewhere: the interface packs reorder sounds). The
  one GameCube-only sample is `HW_IPack/05_interface_6#17` (also in 06_interface_7), in the
  car-select headers the PC merges (7 extra sounds in HW_IPack on GameCube and PS2). Bank samples: 1,370/1,370 identical.
  By slot: 4,134 identical, 4,339 same rate (the rest are the reordered slots).
* **The "376 PS2-only sounds" are the six race*l1 packs**, which the GameCube has too: PS2 and
  GameCube have the same sound count in every pack. Against the GameCube's samples in the same
  slot (421 distinct pairs) the PS2's correlate at median **0.998**; below 0.95 only 3: two swapped
  slots in bat1_l1 (#19/#20) and one different 0.4 s sound in bat2_l9 #30 (correlation 0.37 with
  its slot, 0.22 with anything else in the pack). PS2 has no audio of its own worth taking.
* **Briefing voices** (`audio_/HWB_M01–14`): the same on all three retail releases, 233 sounds,
  1,250.7 s (counts and durations equal per archive; GameCube = PC bytes, PS2 ADPCM). The PS2
  prototype has different dialogue: 174 sounds, 1,238.9 s ([`../beta-audio/README.md`](../beta-audio/README.md)).
* Songs (`RIFX` MIDI): byte-identical on all three, except one song the PC edited (7 world-pack copies; found by the pairing table, [`pairing.md`](pairing.md)).

---------------------------------------------------------------------------------------------
## 4. Movies

`movie_survey.py` (ffprobe/ffmpeg; frame comparison: 10 frames per movie, grey 640×480, PC and
PS2 aligned to the GameCube's frame within ±6 frames, hat as in §1.4, flat frames skipped):

| movie | GameCube THP | PC Bink | PS2 PSS (MPEG-2) | own RMSE GC / PC / PS2 |
|---|---|---|---|---|
| bglogo | 10.62 s, 5.41 Mbit/s | 10.61 s, 3.57 | 10.64 s, 3.69 | **0.68** / 2.00 / 3.51 |
| thqlogo | 8.05 s, 5.02 | 8.04 s, 3.52 | 8.04 s, 2.79 | 2.07 / **1.05** / 5.54 |
| hwboot | 22.54 s, 9.32 | 22.52 s, 4.52 | **27.99 s**, 7.03 | **0.98** / 2.37 / 5.37 (1 frame) |
| hw_intro | 137.26 s, 9.44 | 137.14 s, 4.72 | 137.10 s, 5.95 | **0.63** / 1.98 / 3.17 |
| hw_end | 61.02 s, 9.56 | 60.96 s, 4.69 | 60.69 s, 5.50 | **1.92** / 1.99 / 3.43 |
| hw_gal | — | 69.50 s, 4.70 | — | PC only |
| hw_mark | — | 105.91 s, 4.62 | — | PC only |

* Video: GameCube 640×480 THP (per-frame JPEG), PC 640×480 Bink, PS2 720×480 MPEG-2 (NTSC
  non-square pixels; scaled to 640×480 for the comparison, which adds a little of its own blur to
  the PS2's number); all 29.97 fps. GameCube has about twice the bit rate and the lowest error in
  four of five movies.
* Audio: GameCube THP-ADPCM 48 kHz stereo (4-bit), PC Bink audio (DCT, lossy) 48 kHz stereo, PS2
  PCM s16 48 kHz stereo per its `SShd` header ([`../ps2-version/README.md`](../ps2-version/README.md) §5). ffmpeg doesn't
  demux the PS2's audio stream, so its quality was not measured; by format it is the only lossless
  one.
* PS2's `hwboot` is 5.5 s longer: a different cut, not inspected. PC-only movies: `hw_gal`
  (69.5 s) and `hw_mark` (105.9 s).
* PS2 prototype only: `HWTRAIL.PSS` (112.9 s trailer), `M1INTRO.PSS` (139.7 s, an earlier intro),
  and its own BGLOGO/THQLOGO encodes.

---------------------------------------------------------------------------------------------
## 5. Content in one release only

| | GameCube | PC | PS2 |
|---|---|---|---|
| archives / packs | 105 / 512 | 99 / 506 | 105 / 512 |
| cars / items | 33 / 49 | 33 / 49 | 33 / 49 |
| race1l1–race6l1 (MP drag worlds) | yes | **no** | yes |
| multi-player menus | 4-player split car select | none | 2-player split car select |

* Every archive the three share holds the same packs (type, id, order). Beyond the race*l1
  archives the differences are inside packs: world props and textures (PC, §2, §1.7), car_21/25/26
  geometry, platform glyphs and logos (§1.7), interface screens ([`../menus/README.md`](../menus/README.md)),
  7 car-select sounds (GameCube and PS2, §3), one bat2_l9 sound (PS2), movies (§4).
* **PS2 extras over the GameCube**: none in sound or geometry; textures: DualShock glyphs, the
  chrome logo, the blue item ring, env maps for env-mapped groups, 4 extra car textures.
* **PS2 prototype only** (not a quality source; see [`../beta-cars`](../beta-cars/README.md),
  [`../beta-worlds`](../beta-worlds/README.md), [`../beta-audio`](../beta-audio/README.md)): 77
  archives / 1,237 packs; 36 cars (Fandango, Sweet 16 II, Jester beyond retail), 45 items; the
  CH1_L2 prototype level; different briefing dialogue (§3); archives retail lacks: `HW_IT_PC` and
  `HW_MUSAK` (music, 2 and 1 songs), `HW_SSEQ` (6 audio packs); the trailer and early intro
  movies (§4). It has no `w*_geo/tex/col` archives, nor ch3_l1/2, race6l1/2.

---------------------------------------------------------------------------------------------
## 6. Not measured

* **Textures:** mip levels (regenerated anyway); the effect of re-encoding a PS2 pick to BC3; the
  hat's bias from correlated DXT/CMPR errors (no class has a lossless copy alongside both block
  codecs to calibrate it); whether the PS2's extra car textures are referenced.
* **Geometry:** normals and UVs value by value across PC/GameCube (the PS2/GameCube comparison in
  the PS2 notes covers them); collision packs; shadow volumes beyond their counts.
* **Movies:** PS2 PSS audio (no demuxer for its `SShd` stream); the PS2 `hwboot`'s extra 5.5 s;
  per-frame colour range (MPEG-2 limited range vs JPEG full range can add a constant to the PS2
  error).
* **Interface and object graph:** wording and layout are design, not quality (menus doc).

---------------------------------------------------------------------------------------------
## 7. Reproduce

```sh
B=target/release/hwvx-x; S=HWVX-SDL/build-survey
$B extract orig/gc/files $S/gc
$B extract "$HOME/Library/Application Support/HWVX-SDL/HotWheelsVelocityX/pc" $S/pc
$B extract orig/ps2/XPS/ENGLISH $S/ps2
$B extract orig/ps2-beta/XPS/ENGLISH $S/beta
cd docs/xdl-format/scripts
python3 tex_survey.py   ../../../$S && python3 tex_survey.py ../../../$S --summary     # §1, ~15 s
python3 geom_survey.py  ../../../$S && python3 geom_survey.py ../../../$S --summary    # §2, ~10 s
python3 audio_survey.py ../../../$S --content                                         # §3
python3 movie_survey.py ../../../orig/gc/files/fmv_ "<pc>/FMV_" ../../../orig/ps2/XPS/ENGLISH/fmv_
```

| script | what |
|---|---|
| `survey_common.py` | archive/pack index across releases (the PC import is flat, the prototype upper-case), categories |
| `tex_survey.py` | numpy decoders for all six formats, content pairing, lossless-reference errors, hat, gradients, colours, `rule()` |
| `geom_survey.py` | per-level counts for all three releases (reuses `ps2_geom.py`, `ps2_geom_check.py`, `geom_check.py`), bounding-box pairing |
| `audio_survey.py` | SFX and bank formats, GameCube/PC byte comparison, content hashes, PS2 correlation (reuses `ps2_audio.py`) |
| `movie_survey.py` | ffprobe table, aligned-frame hat and sharpness |

Needs numpy (`tex_survey`, `audio_survey --content`, `movie_survey`) and ffmpeg (`movie_survey`);
run without `-I` so the user site-packages are found.

## As built (HWVX-SDL, step 4b)

* **Commands:** `hwvx --mix-build ultimate` converts every pack into `<pref>/xdl/mix-ultimate/`.
  `hwvx --mix ultimate` then plays from it. `HWVX_MIX` and `HWVX_MIX_BUILD` do the same from the
  environment.
* **Recipes:** `recipes/ultimate.jsonc` is built in; any other `.jsonc` file can be passed instead.
* **Which release supplies what:** the releases come from `sources.cfg`, overridden by
  `HWVX_MIX_GC`, `HWVX_MIX_PC` and `HWVX_MIX_PS2`. Each archive's packs load whole from the base
  release the recipe names. Each texture or sound then comes from the release its rule picks,
  through the pairing table, generated as `src/game/Port_MixPairs.c` by
  `tools/gen_mix_pairs.py`.
* **Code:** the substitution runs as a pack loads (`src/game/Port_Mix.c`). The recipe and the
  releases' files are in `src/port/mix.c`.
* **Cache:** a mixed pack is marked in the cache header (`XDL_SOURCE_MIXED`, with a stamp of the
  recipe and releases), so changing either rebuilds it.

**Measured on the three retail releases:**

* 369 packs are mixed, 365 of them converted.
* Textures: 2,252 from the PC, 1,052 from the PS2, and 8,397 kept from the base. None looked
  unlike its pair.
* Sounds: all 4,773 are the base's own.
* The cache is 664 MB.
* The attract demo plays from it at about the speed of a plain GameCube run.
* Single-release play is unchanged on all three releases (attract demo frame-identical).

**Not done yet:** custom recipes in the launcher (it offers "ultimate" only).

### The launcher


**Building.** `hwvx --mix-build RECIPE` makes a first pass over every archive's table of contents to count the packs it will load. Before each pack it prints `@convert DONE TOTAL ARCHIVE`, the same progress lines `--convert-xdl` prints, and on failure it prints `@error TEXT`. It writes into `<xdl>/mix-NAME.part`, which it empties first. When the build is complete it writes `built.cfg` there:

```
stamp=XXXXXXXX
xdl=VERSION.COMPILER.SETTINGS
packs=N
failed=N
```

`stamp` is Port_MixStamp: the recipe text, the release folders, and the sizes and times of their HW_share files. The builder then renames the old `mix-NAME` folder to `.old`, renames `.part` to `mix-NAME`, deletes `.old` and prints `@convert N N done`. If a build is cancelled or fails, the last complete build is left as it was. Playing with `--mix` still fills `mix-NAME` as packs load, but it never writes `built.cfg`.

**The launcher, Convert page.** The "Mixed game (ultimate)" row shows one of four states:
- **not built**: there is no `built.cfg`, and playing mixes each pack the first time it loads;
- **built**: N packs, N MB;
- **out of date**: the stamp or the cache version in `built.cfg` differs from what Port_MixProbe and Xdl_Stamp give now;
- **needs two or more of the GC/PC/PS2 copies**: fewer of the recipe's releases are imported (a PC or PS2 disc image doesn't count).

The buttons are Build / Build again and Delete. Build runs the program as a child process with `--mix-build ultimate`, using the same progress bar and Cancel as Convert. When a build is cancelled, the launcher removes the `.part` folder. Port_MixProbe reads the recipe and works out its stamp without starting the build.

**The launcher, Play page.** When two or more releases are imported, a "Mixed game (ultimate)" choice appears below the releases. It is saved in sources.cfg as `play=mix:ultimate`. An older launcher that reads this line sees no release chosen. If the mixed game can't be played, the launcher falls back to the first imported release. With this choice, Play (or skip=1, or HWVX_NO_DIALOG) starts the game as if run with `--mix ultimate` (Port_LauncherMix, main_sdl.c).
