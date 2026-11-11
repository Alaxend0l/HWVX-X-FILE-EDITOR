# The PS2 release's data formats, and how to convert them

What the retail PS2 release (`SLUS_204.12`, `XPS/ENGLISH/*.xps`) stores differently from the
GameCube and PC releases, as rules that bring it into the port's one in-memory layout
([`../port-format/README.md`](../port-format/README.md)). Done the way
[`../beta-cars/research/pc_formats.md`](../beta-cars/research/pc_formats.md) did the PC: pair
every PS2 pack with the GameCube's (and the PC's) of the same name, compare the structures, and
check the rules over the whole release.

* Data: `hwvx-x extract` of `orig/ps2/XPS/ENGLISH/<file>.xps` (all 512 packs; 105 archives, all
  named like the GameCube's). Paired with the GameCube (`.xgc`) and PC (`.xpc`) extractions.
* Code: the PS2 executable is not stripped; function names below are its own symbols
  (`HWVX-PS2-Decomp`, `symbol_addrs.txt`, `asm/nonmatchings/cod/*.s`).
* Scripts (`scripts/`, standard library only, run with `python3 -I`):

| script | what |
|---|---|
| `ps2_geom.py` | geometry decoder: VIF packets → triangles → the port's indexed layout (`to_indexed`); `--tree` runs it over a tree |
| `ps2_geom_check.py` | pairs every PS2 geometry with the GameCube's and compares headers, flags, triangles, attributes, winding, bones |
| `ps2_tex.py` | texture decoder (PSMT8 + CLUT, PSMCT32), PNG writer, GameCube/PC decoders; `--compare`, `--check` (all packs), `--survey` |
| `ps2_audio.py` | SPU-ADPCM decoder, SFX descriptors, `sndb` banks, WAV writer; `--check` pairs every sound with the PC's |

All byte orders are little-endian. Pack, archive and object-graph layout are the PC's (§4).

---------------------------------------------------------------------------------------------
## 1. Geometry

### 1.1 Header (0x40) and levels

Same as GameCube/PC except the magic. Levels always follow the header (at +0x40), like the PC's.

| off | type | PS2 | GameCube |
|---|---|---|---|
| 0x00 | u16 | **0x1005** | 0x2003 (PC 0x0002) |
| 0x02 | u16 | numLevels | same |
| 0x04 | u16 | skinned (7/8 when skinned) | same |
| 0x08 | ptr | levels (0x18 each) | same |
| 0x0C | ptr | f32 lodDistances | **equal in 21652/21652** |
| 0x10–0x3F | | bbox, radius, centre | bit-identical in 21615/21652 (37 world/car edits) |

Found the way `Port_Geometry.c` finds GameCube geometry (scan for the magic with pointer slots at
0x8 and 0xC): **21652 geometries in 512 packs, the same number and order as the GameCube's in
every pack** (pairing by index works everywhere).

**Level (0x18):**

| off | type | PS2 | indexed layout (port) |
|---|---|---|---|
| 0x00 | u16 | **kind**: 1 drawn (22734), 2 skinned (121 = the GameCube's skinned levels), 3 shadow (33) | layout mark |
| 0x02 | u16 | numGroups (equal to GC everywhere) | same |
| 0x04 | u32 | about the number of distinct vertices (exporter's count; not needed) | 0 |
| 0x08 | ptr | groups (0x20 each) | same |
| 0x0C | u32 | stale export-time address (`0x1F…`/`0x21…`, **not** a pointer slot) | vertex buffer |
| 0x10 | u32 | 0, or (skinned) a stale address | skinned: bone counts |
| 0x14 | ptr | texture-name list | same |

`DisplayList_AddGeometry` dispatches on the kind: 3 → shadow list, 1 → the normal lists;
skinned levels go through `BuildSkinGroups`/`SkinGroups`.

**Group (0x20):**

| off | type | PS2 | GameCube |
|---|---|---|---|
| 0x00 | u16 | 0 | 0 |
| 0x02 | u16 | flags without the class bits (table below) | flags |
| 0x04 | u8[4] | colour (same bytes) | same |
| 0x08 | u32 | 0 | 0 |
| 0x0C | ptr | **packet**: `{UVScroll (0x10), DMA packet}` | uvScroll* (247 groups) or 0 |
| 0x10 | u32 | **triangle count** (equals the decoded count in every group) | DL byte size |
| 0x14 | u16 | blend (equal but 1) | same |
| 0x16 | u16 | sort key (equal but 1) | same |
| 0x18 | u32 | texture, relocated as a u32 at 0x18 (pre-reloc `0x0012xxxx`/`0x0013xxxx`/`0x011Axxxx`, `0xFFFF0000` none) — **the PC's arrangement**; the PS2 code reads the u16 at 0x18 | u16 override 0x18, id 0x1A |
| 0x1C | u32 | stale address | DL pointer |

The packet's first 16 bytes are a **UVScroll** `{f32 u, f32 v, GXColor, u32 flags}`, the same
struct the GameCube points to from 0xC (`BuildGXRuntime` reads `u, v` and, when `flags & 2`, the
colour, before each group). It is zero in all data. The GameCube's exporter only gave one to
groups a shader animates (247), the PS2 to every group.

**Flags.** PS2 = GameCube with the level-class bits (`0x800/0x1000/0x2000`) removed, and two
oddities: alpha groups (`& 1`) may carry `0x04/0x02/0x10` (PS2 `0x5/0x3/0x7/0x15/0x45`, GC
`0x1001/0x2001/0x2041`), and `0x8000` is GC `0x9000`. Pairs counted over all 46449 groups:

| PS2 | GameCube | | PS2 | GameCube |
|---|---|---|---|---|
| 0000 | 1000 (27231) / 2000 (1092) | | 0010 | 0810 / 2010 |
| 0001 | 1001 / 2001 | | 0014 | 0814 / 2014 |
| 0002 | 0802 / 2002 | | 0041, 0045 | 1041 / 2041 |
| 0004 | 0804 (8078) / 2004 (680) | | 0005, 0003, 0007, 0015 | 1001 / 2001 |
| 0006 | 0806 / 2006 | | 8000, 8001 | 9000, 9001 |
| 4000 | 4000 (shadow) | | 0081 | 1081 |

Rule (`ps2_geom.gc_flags`): if `f & 1`, clear `0x16`; `0x8000` → `0x9000`; otherwise add the
level's class: `0x2000` if the level has both normal and colour groups, `0x800` if only normals,
`0x1000` if only colours. **Reproduces the GameCube's flags in 46449/46449 groups.** (The PS2
draw order tests `& 2`, `& 0x80`, `& 1`, `& 0x40` (skipped), `& 4|0x10`, `& 0x100`, else unlit,
so the extra bits on alpha groups change nothing.)

### 1.2 The packet (VIF1 stream for the VU1)

```
packet+0x00  UVScroll (16 bytes)
packet+0x10  DMA tag: id 6 (ret), QWC = the stream's size in qwords; upper 64 bits = VIF NOP NOP
             then batches, each:
  BASE 0, OFFSET 0x1DC/0x1F0       (VU double-buffering)
  STCYCL CL=4 WL=4
  UNPACK V4-32 x1 -> VU addr 0     GIF tag: NLOOP = vertex count (strip) or triangles (list),
                                   PRE=1, PRIM = 0x0E (strip, gouraud, textured) or 0x2E (+ alpha blend)
                                   / 0x0D (list, skinned), REGS = ST, RGBAQ, XYZ2 (x3 for lists)
  STCYCL CL=3 WL=1                 (interleave: vertex i at VU addr 1+3i, 2+3i, 3+3i)
  UNPACK V4-32 xN -> addr 1        position x, y, z (f32), w = flag word
  UNPACK V2-32 xN -> addr 2        s, t (f32)
  UNPACK V3-32 xN -> addr 3        normal (lit/env groups: flags & 0x16, not alpha)
     or  V4-8  xN -> addr 3        colour R, G, B, A (u8; every other group)
  MSCNT
```

The CPU calls it with a DMA `call` to `packet + 0x10` after uploading matrices and lights, then
`MSCAL` picks the microprogram by group kind (`VertexColorGroups`, `ParallelGroups` (lit),
`EnvMapGroups`, `SkinGroups`). No other VIF code occurs (survey of all 46449 packets). Batches
hold at most 79–81 vertices.

**Triangles (strips, PRIM 4).** Bit 15 of w is the GS ADC bit: a vertex with it set does not
draw; any other vertex *i* draws `(i-2, i-1, i)`. Count the drawing vertices since the last ADC
vertex ("run"): odd runs swap the first two, `(i-1, i-2, i)`. w is otherwise 0 (only `0` and
`0x8000` occur). Many "strips" are single triangles (`ADC, ADC, draw`).

**Skinned levels (kind 2, PRIM 3 lists).** `packet + 0x10` holds records
`{u8 bone[3], u8 0, ptr subpacket}` up to a null pointer (1 or 3 records per group); each
subpacket is a DMA `ret` packet as above with triangle lists. `SkinGroups` loads the matrices of
`bone[0..2]` (0xFF = none) into VU memory, and the vertex's w is a **signed slot −1, 0, 1: its
bone is `bone[w + 1]`**. Positions are bone-local, bit-identical to the GameCube's. All skinned
groups are lit (normals).

**Winding.** The PS2 does not cull (no `Render_StateCullMode`, nothing in the packet marks a
strip's direction) and its stripper wrote about 1% of strips the other way round, always whole
strips. The GameCube and the port cull back faces, so a converter must re-orient them.
`ps2_geom.orient`: for each strip, count the directed edges it shares with the group's other
strips walked the *same* way and the *opposite* way (a consistent mesh walks shared edges in
opposite directions); reverse the strip when "same" wins, unless its face normals agree with its
vertex normals (lit groups: Σ cos(face normal, n0+n1+n2) > 0). Normals alone are not used: the
GameCube's normals point inwards on ~1% of strips.

### 1.3 Attribute values against the GameCube

| attribute | rule | evidence (all packs) |
|---|---|---|
| position | bit-identical f32 | every triangle matched by positions |
| normal | bit-identical f32 | in 99% of triangles; the rest are content edits |
| UV | equal **up to a whole-number shift per triangle** (the exporter moved UVs near 0 for the GS); identical under wrap | 1,083,566 equal, 544,357 shifted, 953 other |
| colour | **R, G, B = gc >> 1** (GS 0x80 = 1.0), **A = gc >> 2** | 3.19M values: ≥ 98.6% per channel on the rule; the rest are content edits (e.g. world prelighting) |

Back to GameCube scale: `gc = round(ps2 * 255 / 127)` for RGB (max error 1), `min(255, a * 4 +
a / 16)` for alpha (255 → 63 → 255). The UV shift is harmless for repeating textures; a clamped
texture with a shifted triangle would sample its edge texel instead (not seen in tests).

### 1.4 Conversion into the indexed layout (`ps2_geom.read_geometry`, `to_indexed`)

1. Copy the header (magic → the GameCube's if wanted) and levels; level 0x0 = indexed mark,
   0x4 = 0, 0x0C = the new vertex buffer, 0x10 = bone counts for skinned levels.
2. Shadow levels (kind 3): copy as they are (positions + edges, §4).
3. Per group: flags by §1.1; 0x0C = pointer to the packet's UVScroll record (zero; keeps
   shader-driven scrolling working for every group) or 0; 0x10 = triangle count (already
   there); texture: relocated u32 at 0x18 like the PC's, so flag the reloc to store at 0x1A
   (`PORT_TEXRELOC_U16`) or move it; 0x1C = u16 index list.
4. Decode every batch (§1.2), orient the strips, then one vertex per distinct
   `(position, uv, normal, colour, bone)` in first-use order across the level's groups. A level
   with both normal and colour groups gets white colours for lit vertices and a zero normal for
   unlit ones (neither is read for that kind). Colours to GameCube scale (§1.3).
5. Skinned: stable-sort vertices by bone, renumber, write the eight counts; bone *k* = matrix 3k.
6. Limits: at most 8311 vertices in a level (< 0x7FFF), bones 0–7.

### 1.5 Validation

`ps2_geom.py --tree`: all **21652 geometries decode** (22734 drawn, 121 skinned, 33 shadow levels),
1,629,882 triangles, every group's 0x10 equals its decoded triangle count, max 8311 vertices.

`ps2_geom_check.py <ps2> <gc>` (whole release, 512 packs):

| | |
|---|---|
| geometries / levels / groups paired | 21652 / 22888 / 46449, no count mismatch |
| group flags reproduced (with class bits) | **46449 / 46449** |
| triangles GC / PS2 | 1,630,985 / 1,629,882 (2109 GC-only, 1006 PS2-only: edits; 93 groups changed count) |
| winding, packet order as stored | 1,611,544 same, **15,910 reversed (0.98%)** |
| winding after `orient` | 1,621,617 same, **5,837 reversed (0.36%)**, 1422 degenerate |
| skinned bones (`bone[w+1]` vs GC pnmtx/3) | all equal (car_29: 24/12/12 vertices on bones 0/1/3, as GC) |
| triangle attributes | 1,611,501 match, 17,375 differ (content) |

Cars only (34 packs): 7158/7158 flags, 435,710 GC triangles, 519 reversed after `orient`
(4061 before).

---------------------------------------------------------------------------------------------
## 2. Textures

### 2.1 Table, header, CLUTs

At `texture_data_offset`, `num_textures` **packed 0x10-byte headers** (the PC's stride), LE:

| off | type | meaning |
|---|---|---|
| 0x0 | u16 | format: **2 = PSMT8** (8-bit index + CLUT; 11486), **1 = PSMCT32** (bytes R, G, B, A; 273). No 4-bit or 16/24-bit textures |
| 0x2 | u16 | width |
| 0x4 | u16 | height |
| 0x6 | u16 | levels: 0 = one level, else the stored level count, **full chain to 1×1** (`log2(max)+1` in 8803/8803) — the PC's rule |
| 0x8 | ptr | pixels (pointer slot) |
| 0xC | u32 | CLUT index (< num_palettes in every PSMT8 texture; shared between textures in world packs: 124 textures / 65 CLUTs) |

* Pixels: **linear, row-major, not GS-swizzled**; levels back to back (`w*h*bpp/8` each), the
  chain padded to 16 bytes. Every texture's size matches this (11759/11759).
* CLUTs: at the pack header's `palette_offset`, `num_palettes × 0x400`: 256 entries of bytes
  R, G, B, A in **CSM1 order** (logical index *i* is stored at *i* with bits 3 and 4 swapped).
  The object graph continues after them.
* **Alpha is 0..255** (not the GS's 0x80 = 1.0): CLUT alpha histogram peaks at 255 (1.59M) and 254;
  only 1039 entries are 0x80.

These are the PS2 prototype's formats ([`../beta-cars/research/textures_sounds.md`](../beta-cars/research/textures_sounds.md) §2),
except that retail level counts go to 1×1.

### 2.2 Against the PC and GameCube

* **Colours are the PC's**: GameCube = PS2² / 255 per RGB, like PC → GC. 02_car_1: RGB error vs
  the PC's DXT 0.0–3.6 (8.1 for one cut-out); vs the GameCube 25–49 as is, **1.0–5.9 after squaring**.
* `ps2_tex.py --check` (every pack, textures paired by content on a 16×16 sample grid):
  PC: 9184 of 10970 paired with mean RGB error < 8 (6113 < 4); of those 8950 same size and 8962
  same level count, alpha error < 8 in 8948. GameCube (PS2 squared): 10567 of 11673 < 8. The rest
  are content edits, DXT/CMPR loss on noisy textures, and flat textures that pair ambiguously.
  PSMT8 became PC DXT1 (6608) or DXT5 (2351); PSMCT32 became DXT5.
* **Indices differ from the GameCube's in some packs**: the PS2 sometimes adds textures, e.g. an
  env-map texture at index 0 of 02_car_1 (its 6 others are GC 0–5 shifted by one). The PS2 texture
  relocations point at the PS2 table, so a converter keeps the PS2 table and relocations as a
  set. Env-mapped groups (flags & 2) carry a texture on the PS2 (2290 groups) where the GameCube
  has none and uses the global env map; the GameCube draw path ignores the group texture there.

### 2.3 Conversion

1. Decode level by level: PSMT8 → `clut[csm1(index)]`; PSMCT32 as is. Alpha unchanged.
2. Write the PC's raw format **1 (RGBA8888, row-major)**, `PORT_TEX_PC | 1` in the port, same
   width/height/levels; levels concatenated. (PS2 format 1 already *is* that byte layout.)
3. Colour setting: keep the PS2/PC colours, or square them for the GameCube look.
4. A compiler targeting the GPU encodes BC1 (all alpha 255) or BC3; the PC's choice (DXT5 for any
   alpha < 255) is the reference.

---------------------------------------------------------------------------------------------
## 3. Sounds and music

### 3.1 Sound effects

Descriptor (0x20, LE) at `sfx_offset`, `num_sounds` of them, data slots in `sound_pointers`:

| off | type | PS2 | GameCube/PC |
|---|---|---|---|
| 0x00 | ptr | sample data | same |
| 0x04 | u32 | length in **VAG bytes** (multiple of 16) | PCM bytes |
| 0x08 | u32 | loop: **byte offset of the loop block** (16 in every looping car sound), ~0 = one-shot | PCM bytes (0) or ~0 |
| 0x0C | u32 | rate, pitch-tuned (11075, 22104, …) | 11025, 22050 |
| 0x10 | u16 | **0x1000** (SPU pitch 1.0; bytes `00 10`) | u8 0, u8 bits 16 — the same bytes |
| 0x12 | u16 | volume 0x7FFF | same |

Samples: headerless **PS2 SPU-ADPCM** (VAG bodies): 16-byte blocks `[shift | filter<<4][flags][14
bytes, 28 nibbles low first]`, filters (0,0) (60,0) (115,−52) (98,−55) (122,−60),
`s = (nib<<12 >> shift) + (s1*f0 + s2*f1 + 32 >> 6)`. Block 0 is always silent (4723/4723); a
looping sound's loop block has flag 6 and its last block 3 (218/218), a one-shot's last block 1
(4505/4505). The encoder resampled each sound so its loop starts on a block boundary, hence the
odd rates. ~3.2× smaller than PCM16.

**Conversion** (to the PC's PCM16 descriptor): decode, **drop block 0** (28 samples), length =
2·samples, loop = `(loop/16 − 1) · 28 · 2` bytes (or ~0), keep the PS2 rate, `u8 0, u8 16` at
0x10, samples LE, 16-aligned. No block-0 drop if the loop offset is kept as `loop/16·28·2`.

**Validation** (`ps2_audio.py --check <ps2> <pc>`, 4723 sounds in 306 packs): sample counts match
the PC's × rate ratio within one block in 4345/4347 pairs, loop kinds 4347/4347; correlation after
resampling **median 0.994, ≥ 0.95 in 4220/4347** (35 < 0.8: content edits). 313 sounds pair with
a different PC index (interface packs reorder sounds); 376 PS2 sounds are in packs or slots the
PC lacks.

### 3.2 Songs and instrument banks

* Songs: `RIFX` 'MIDI', **byte-identical** to the PC's (HW_music 3/3).
* Banks: `RIFF` 'sndb' at `song_bank_offset` (HW_music and 57 world packs), the PC's LE header and
  0x20-byte entries, with: **u8 bits = 4** at entry +0x11 (PC 16), offsets/lengths/loops in VAG
  bytes, samples in VAG; the table size at 0x14 includes the 0x20 header (samples at bank +
  table size). Header words 0x18/0x1C are junk (`0xC0000000/0x28`, `0x7C310030/0x7C337C00`).
* Over 1370 paired entries: key ranges and envelopes equal the PC's in all; VAG samples − 28 =
  PC samples × rate ratio (within 2 blocks) in all; rates equal in 753 (the others resampled for
  block-aligned loops); correlation median 0.98 (lossy high-frequency instruments lower).
* Conversion: as for SFX per entry (drop block 0, bits 16, PCM offsets/lengths/loops), then the
  PC's bank form.

### 3.3 Audio archives

`audio_/HWB_Mxx.xps` are ordinary packs of SFX (briefing voices) in the format above.

---------------------------------------------------------------------------------------------
## 4. Everything else

* **Archives and packs**: the PC's TOC and pack header (LE, version 0xB7); `pack.json` adds
  `num_palettes`/`palette_offset` (0 on GC/PC). Texture relocations target group+0x18 like the PC's.
* **Shadow volumes** (kind-3 levels, groups `0x4000`): 33 levels, 622 groups, not drawn through
  packets; left as they are (not compared field by field).
* **Object graph** (CarAInfo, worlds, collision, interfaces): same structs, LE; not compared
  word by word here. Texture names in level name lists differ in 2222/22888 levels.
* Not checked: collision packs, interface content differences.

## 5. Movies

`fmv_/*.pss`: MPEG-2 program streams (pack header 0xBA, MPEG-2 marker), video stream 0xE0 (MPEG-2),
audio in private stream 1 (0xBD) sub-stream 0xA0 with an `SShd` header: type 1 (PCM s16),
48000 Hz, 2 channels, interleaved blocks (`SSbd` data). Decodable with any MPEG-2 decoder plus a
small PSS audio demuxer.

## 5.5 Save files

On the memory card: directory **`BASLUS-20412`** (the US release's product code) holding `icon.sys`,
`hw.ico` (the card's 3D icon) and the save itself, **`BASLUS-20412`**, 2300 bytes.
[`scripts/ps2_mcd.py`](scripts/ps2_mcd.py) lists and extracts files from a PCSX2 `.ps2` card image
(the card file system with ECC, documented in the script).

The save is the **GameCube's layout exactly, in little-endian**: `SAVE_FILE_MAGIC` 4, size 0x8F0,
the game's checksum (sum of byte + 1 over the 0x8F0 bytes from 0x00C; verified), the 0x150-byte
`GeneralGameSettings` (volumes, four keyboard bindings in Windows key codes, as the PC has, and the
slot entries "HW VEL X 1" ...), then 8 × 0xF4 `HotWheelsGameData` slots from 0x15C. The PC's file is
the same plus 0x38 bytes of settings (`../pc-version/README.md` "Save files").

Cross-checked: a PS2 save and a PC save made after the same progress (Challenge 1) hold the same
unlocks byte for byte: cars 0–5, items 0–3, tracks 0, 5, 19–22, 29–32, 39, 40.

## 6. PS2 data in HWVX-SDL

HWVX-SDL (branch `ps2-data`) boots from the PS2 disc's files (`--data` the folder holding
`XPS/ENGLISH/`, or that folder) and converts each pack as it loads, by these rules:
`Port_Data.c` (dispatch), `Port_PS2.c` (textures → raw RGBA8888 `PORT_TEX_PC | 1`, SPU-ADPCM SFX
and `sndb` banks → PCM16), `Port_Geometry.c` (`PortGeometry_ConvertPS2`: VIF strips and skinned
lists → the indexed layout, group flags, texture relocations at 0x18). One thing §4 doesn't say:
a standard pack's **header and sound data are padded to 2048-byte sectors** in the file (the
loader skips the padding), and the header word `num_palettes` is a count, not a pointer. The
PS2's `GameSettings_GetCodeVariable(0x258)` (platform) returns 1 where the GameCube's returns 2;
HW_IPack's title shows its "Maximum Justice" subtitle only on 1, so the port returns 1 with PS2 data.

Status: the attract demos of seeds 1–4 run with every geometry converted (skinned levels too) and
textures, sounds and music decoded (audio zero-crossing rate per audible second 0.02–0.07, as the
GameCube's); races follow the PC's frame for frame (PS2 and PC have the same world edits, which
make seeds 2 and 4 drift from the GameCube's). Not done: shadow volumes (kind 3 levels are off:
their positions are not found), movies (`.pss`). Levels are drawn without culling; `HWVX_PS2CULL=1`
re-orients strips with `orient` (same strips turned as `ps2_geom.py`) and culls, but a wall of
`bat2_l9`'s quadrant geometry then goes missing (§7.1).

## 7. Uncertainties

1. **Winding**: 0.36% of triangles stay reversed after `orient` (strips with no shared edges, mixed
   neighbourhoods). The alternative is drawing PS2 geometry without culling, as the PS2 does.
2. UVScroll on every group: harmless (identity), but it switches the GameCube draw path to the
   texture-matrix variant for groups the GameCube never scrolled.
3. Vertex-colour low bits are lost (`gc >> 1`); alpha keeps 6 bits.
4. Level 0x4 (vertex count) and the VU microcode were not read; the format was derived from the
   data and the CPU-side functions (`SkinGroups`, `VertexColorGroups`, `BuildGXRuntime`,
   `DisplayList_AddGeometry`).
5. PS2-only textures (env maps) and reordered sounds mean shared-pack indices are the PS2's own;
   do not mix PS2 packs with another release's shared packs.
6. PSS audio type/interleave from the header only (not decoded).
