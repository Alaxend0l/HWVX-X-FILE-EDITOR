# HWVX car packs: PC (v183, little-endian, OpenGL) vs GameCube (v183, big-endian)

Scope: HW_Cars car packs (`SCRATCH/pc_cars/NN_car_K` vs `SCRATCH/gc_cars/NN_car_K`, 33 cars + `00_share_2`).
GC formats are in `docs/beta-cars/research/geometry.md` and `textures_sounds.md` ("GC doc" below).
All offsets are blob offsets. PC pointer slots hold blob offsets and are listed in `pointers.bin`, like on GC.

Scripts are in `SCRATCH/research/pc/`. Run them from that folder with `python3 <script>`:

| script | purpose |
|---|---|
| `pclib.py` | PC geometry parser (geoms / lods / groups / index lists / unified VB) |
| `geocmp.py` | per-group triangle comparison of PC and GC (all cars). Output: `geocmp_all.txt` |
| `attrcheck.py`, `bijection.py`, `arrorder.py` | which PC vertex attributes GC carries; vertex order and de-duplication rules |
| `hdrcmp.py`, `shadowcmp.py` | header, LOD and shadow-volume field comparison |
| **`gc2pc_geo.py`** | **reference GC->PC geometry builder**, with a byte comparison against the real PC packs (`--oracle` adds hidden attributes, see §1.8). Outputs: `gc2pc_default.txt`, `gc2pc_oracle.txt` |
| `gc2pc_classify.py` | explains every geometry that does not match byte for byte. Output: `gc2pc_classify.txt` |
| **`pc2gc_check.py`** | PC->GC direction: rebuilds the GC VB and display lists from PC data and compares bytes |
| `pctex.py` | PC texture headers, DXT1/DXT5 decoders, GC CMPR->DXT1 re-tiler |
| `texcmp.py`, `gamma.py` | PC vs GC texture decode comparison (`texcmp.txt`, `gamma.txt`, `png/*_pc_vs_gc.png`) |
| **`gc2pc_tex.py`** | **reference GC->PC texture converter** (gamma fix, DXT1/DXT5 encoder, mip chain) + validation (`png/*_realPC_ours_GC.png`) |
| `sndcmp.py` | sound descriptor and sample comparison |
| `footprint.py`, `objcmp.py`, `wordcmp.py`, `objswapcmp.py` | byte-class comparison of everything outside geometry and textures (`objswap_all.txt`) |
| `exe.asm` | `objdump -d -M intel "Hot Wheels.exe"` (image base 0x400000, OpenGL IAT at 0x6d7650..) |

(Other files in `research/pc/`, such as `orig.exe`, `patched.exe`, `patch_exe.py`, `run/` and `wine*.log`, are not part of this work.)

---------------------------------------------------------------------------------------------
## 0. Summary

* **Geometry:** the header and LOD layout are the same as GC. Groups are the same except +0x10, which is the **triangle count**,
  and +0x1C, which points to a **u16 triangle list** (`glDrawElements(GL_TRIANGLES, 3n, GL_UNSIGNED_SHORT)`).
  The vertex buffer is a **unified (de-indexed) vertex array**: one index per vertex, and pos / col / uv / nrm arrays
  with strides 16 / 4 / 8 / 16. Triangles are the same triangles **in the same order** as GC (6883/6936 groups exact; the rest are art or float edits).
  GC VB arrays = unique-in-order of the PC arrays, and PC vertex order = first appearance in the triangle lists.
  So **PC->GC is fully derivable** (99.0% of DLs and 97.7% of VBs byte-identical; the rest are content edits). **GC->PC** is derivable except
  for attributes GC drops (normals of alpha/unlit vertices, colours of lit vertices) and a few exporter quirks. The reference builder
  reproduces **481/551 geometries byte-for-byte** when those values are supplied, and 316/551 with defaults (§1.8).
* **Shadow volume:** same as GC, plus extra boundary edges (n1 = 0), colour 0 and no texture reloc.
* **Textures:** 0x10-byte headers, packed (not 0x20 slots). Format **4 = DXT1 (RGB, uploaded as GL_COMPRESSED_RGB_S3TC_DXT1)**,
  **5 = DXT5**, 0/1 = raw RGB/RGBA. Full mip chain down to 1x1. Pixels are 32-aligned.
  **GC colours = PC colours squared** (`gc = pc*pc/255` per RGB channel, on 129 of 129 non-flat textures), so GC->PC needs `pc = sqrt(255*gc)`.
* **Sounds:** identical descriptors and samples (PCM16), byte-swapped. Samples are 16-aligned on PC (32 on GC).
* **Everything else** (CarAInfo graph, anims, HUD): same struct layouts, byte-swapped. **Allocation alignment is 16 on PC, 32 on GC.**
  Pack header fields are the same except `texture_data_offset`.

---------------------------------------------------------------------------------------------
## 1. PC geometry

### 1.1 Header (0x40, 16-aligned)
Same as GC doc §1.1, little-endian, with one difference:

| off | type | PC | GC |
|---|---|---|---|
| 0x00 | u16 | **0x0002** | 0x2003 |
| 0x02 | u16 | numLODs | same |
| 0x04 | u16 | skinned flag (7 for car_29 @0x20, else 0) | same |
| 0x08 | ptr | LOD array, always at geom+0x40 | same |
| 0x0C | ptr | f32 lodDistances[numLODs] | same values (583/585) |
| 0x10..0x3F | | bbox min, +0x1C radius, bbox max, +0x2C radius, center, 0 | **bit-identical to GC in 585/585** (including the odd +0x1C values) |

### 1.2 LOD (0x18)
| off | type | meaning |
|---|---|---|
| 0x00 | u16 | 0 |
| 0x02 | u16 | numGroups (same as GC in 1764/1768; 4 are art changes) |
| 0x04 | u32 | 0 |
| 0x08 | ptr | group array (numGroups x 0x20) |
| 0x0C | ptr | **unified vertex buffer header** (§1.4). Shadow LOD: f32[3] positions (§1.6) |
| 0x10 | u32/ptr | 0. Skinned: pointer to the per-bone vertex-count table (§1.7) |
| 0x14 | ptr | texture-name list, same strings as GC. **Empty list -> points at the LOD itself** (its first byte is 0). Shadow LODs always point at themselves |

### 1.3 Group (0x20)
| off | type | PC meaning | GC |
|---|---|---|---|
| 0x00 | u16 | 0 | same |
| 0x02 | u16 | flags. **Identical to GC** (0x804/0x806/0x814/0x2000../0x1001, 0x4000 shadow) | same |
| 0x04 | u8[4] | env colour, same bytes | same |
| 0x08 | u32 | 0 | same |
| 0x0C | ptr | UVScroll*, NULL in all cars (the draw code loads it as a GL texture matrix if set) | same |
| 0x10 | u32 | **triangle count** (`glDrawElements(GL_TRIANGLES, 3*n, ...)`) | DL byte size |
| 0x14 | u16 | blend mode | same |
| 0x16 | u16 | sort key | same |
| 0x18 | u32 | texture slot. Textured: texture_relocs entry (same index | pack<<16 as GC). Pre-reloc content on PC is `0x00120000 \| idx` (5093/5093). Junk, so write 0 or that. The draw code binds **u16 at +0x18** (LE low half) after reloc. Untextured: **`0xFFFF0000`** (u16 +0x18 = 0, u16 +0x1A = 0xFFFF; the same u16 pair as GC's 0x0000FFFF). Shadow groups: 0, **no reloc** (GC has a reloc there, see §1.6) | |
| 0x1C | ptr | **u16 index list** (`3*n` u16, triangle list, CCW like GC) | display list |

Draw path (exe 0x469440 and 0x469560 = lit variant with `glEnable(GL_LIGHTING)`; 0x4696a0 = env/alpha variants):
`bind(u16 grp+0x18)`; if `grp+0xC` set, load a texture matrix; `SetVertexBuffer(lod+0xC)` (0x469b80);
then `if (idx[0] & 0x8000)` **strip mode**: records `{u16 count (bit 15 set on the first record only), count indices}`
repeated until a 0xFFFF terminator, each drawn with `glDrawElements(GL_TRIANGLE_STRIP, ...)`. Otherwise
`glDrawElements(GL_TRIANGLES, 3*grp+0x10, GL_UNSIGNED_SHORT, idx)`. **No car pack uses strips.**
A converter must keep the first index of each list below 0x8000 (in practice: < 32768 vertices per LOD).

### 1.4 Unified vertex buffer (16-aligned)
```
+0x00 ptr pos   f32 x,y,z + 4 pad bytes (0xCDCDCDCD in every retail vertex: uninitialised MSVC heap). stride 16
+0x04 ptr col   u8 R,G,B,A, stride 4. 0 (not in pointers.bin) when the LOD has no colours
+0x08 ptr uv    f32 s,t, stride 8
+0x0C ptr nrm   f32 x,y,z + 4 pad bytes (0xCDCDCDCD), stride 16. 0 when the LOD has no normals
+0x10 16 zero bytes
+0x20 arrays: pos[N] | uv[N] | nrm[N] | col[N], each 16-aligned (zero padding), in exactly this order
```
SetVertexBuffer (0x469b80): `glVertexPointer(3, GL_FLOAT, 16, pos)`, `glTexCoordPointer(2, GL_FLOAT, 8, uv)`,
and colour/normal client arrays are enabled only if the pointer is non-zero (`glColorPointer(4, GL_UNSIGNED_BYTE, 4)`,
`glNormalPointer(GL_FLOAT, 16)`). There is no count field: N = max index + 1 (every vertex is referenced in 1802/1802 LODs).
Which arrays exist follows the LOD class bits of the group flags, exactly like GC:
0x800 -> pos, uv, nrm. 0x1000 -> pos, uv, col. 0x2000 -> pos, uv, nrm, col.

### 1.5 Vertex order and de-duplication (derivation rules)
* A PC vertex is the full tuple (pos, uv, nrm if the LOD has normals, col if it has colours), shared by **all groups of the LOD**.
* **Order = first appearance** while walking groups in order and each group's index list in order (1800/1802 LODs; the 2 exceptions are the skinned LODs, §1.7).
* De-duplication is on the full tuple, except that the PC exporter left **exact duplicate vertices in 53/1802 LODs** (43 geometries). That cannot be derived; it only costs a few bytes.
* PC vertices carry attributes that GC dropped (`attrcheck.py`, over all paired groups):
  * lit/env vertices in 0x2000 LODs have a colour, which is `FFFFFFFF` in 255,945/255,945 lit and 193,517/193,647 env cases;
  * alpha (flags & 0x81) and unlit vertices in 0x2000 LODs have a **normal**. It is a real normal and is present in the GC normal array
    (67,356/67,488), but GC never indexes it;
  * env groups (flags & 2): GC DL index C is the UV index (430k/430k), so PC env vertices get `uv[idxC]`.
* Inverse direction: **GC arrays = unique-in-order (bitwise) of the PC arrays** (`arrorder.py`: pos 1744/1762, uv 1759/1762,
  nrm 1652/1670, col 670/670; the misses are content edits).

### 1.6 Shadow-volume geometry (CarAInfo+0x54; 33/33 cars)
* Header and LOD as GC: header 0x0002, 1 LOD, `LOD+0xC` = **packed f32[3] positions (stride 12)**, the same array as GC (32/33 identical, 1 edit),
  and `LOD+0x14` = the LOD itself.
* Group: `u16 0, u16 0x4000, u8 col[4] = 00 00 00 00` (GC: FF FF FF 00), `u32 0, u32 0, u32 numEdges, u32 0, u32 0` (no texture reloc;
  **GC carries a texture reloc there**, shared 2:20 in 602 groups, 2:2/2:3 in 20), `ptr edges`.
* Edge (0x20) is the same as GC, LE: `f32 n0[3], u16 v0, u16 v1, f32 n1[3], s32 type = 0`.
* **PC = GC edges plus boundary edges** (`shadowcmp.py`): 618/622 groups have GC ⊆ PC, the common edges are in the same relative order,
  n0/n1 are equal. The extra PC edges have **n1 = (0,0,0)** (1821, an edge with only one adjacent triangle), plus 42 others.
  GC->PC: copy the GC edges (the boundary edges can't be rebuilt without the source triangles). PC->GC: drop edges with n1 == 0.

### 1.7 Skinned geometry (car_29 @0x20 only)
`LOD+0x10` points to a 0x20-byte table `u32 count[4]` (+16 zero bytes), identical to GC (`{30,15,0,15}`, `{10,5,0,5}`). It is the
number of vertices per bone. **PC vertices are sorted by bone**: GC pnmtx 0 -> vertices 0..29, pnmtx 3 -> 30..44, pnmtx 9 -> 45..59,
i.e. bone = pnmtx/3. There is no per-vertex matrix index on PC (pos.w = 0xCDCDCDCD). The order inside a bone is not first-appearance
(unresolved). Layout: header, LODs, table, groups, ...

### 1.8 Layout and alignment (reproduced byte-exactly by `gc2pc_geo.py`)
Everything is **16-aligned** with zero padding (GC uses 32 for geometry, DLs and shadow data).
Per geometry: `header | LOD array | per LOD { groups | index list of each group (each 16-aligned) | VB header | pos | uv | nrm | col | names } | lodDistances`,
and the next geometry starts at the next 16-byte boundary. Name list: the strings with their NULs, but **no space is reserved for the final
list terminator**. The next object starts at `align16(start + sum(strlen+1))`, so the terminator is the next object's zero byte or padding
(11 times the lodDistances follow directly, so the list is "terminated" by `20 BC BE 4C` = 1e8; the list is never read at runtime).
Shadow: `header | LOD | groups | positions | edges per group | lodDistances`.
Geometries appear in the same order as on GC (index-paired, bboxes equal 585/585).

### 1.9 GC -> PC geometry recipe
1. Header: copy, u16 0 = 2, swap all fields (u16 at 0..6, u32/f32 after that).
2. LOD: same, +0x10 = 0 (or the bone table). Group: copy flags, colour bytes, +0x14/+0x16. +0x10 = triangle count. +0x18 as in §1.3.
3. For each LOD, walk the GC DL triples `(A, B, C)` of each group in order:
   lit/env (flags & 0x16 and not & 0x81): `pos[A], nrm[B], uv[C]`, colour FFFFFFFF if the LOD has colours.
   alpha/unlit: `pos[A], col[B], uv[C]`, normal = any value if the LOD has normals (lighting is off for those groups; the retail value is not recoverable).
   De-duplicate on the full tuple, number vertices in first-appearance order, emit u16 indices.
4. Write arrays with stride 16 for pos/nrm (pad = 0xCDCDCDCD to match retail, or 0), 8 for uv, 4 for col, in the §1.4 order.
5. Shadow: §1.6. Skinned: sort by bone (§1.7).

**Validation** (`gc2pc_geo.py`, 584 paired non-skinned geometries):
* defaults as above: **316 byte-identical** (all pointer slots and texture relocs included; texture-slot pre-reloc bytes ignored);
* with the hidden attributes supplied from the real PC data (`--oracle`, i.e. the normal of alpha vertices): **481 identical**, 3.5% of bytes differ overall.
  `gc2pc_classify.py` explains every one of the other 103: 33 shadows (boundary edges), 43 PC duplicate vertices, 12 GC junk texture names
  (`"B "` entries), 7 +0.0/-0.0 float signs merged by the GC exporter, 7 art/float edits, 1 texture index change.

**PC -> GC** (`pc2gc_check.py`): rebuilding GC arrays as unique-in-order (bitwise) and DLs as `0x90, u16 n, (posIdx, nrm-or-colIdx, uvIdx)*`,
zero-padded to 32, gives **6869/6936 DLs and 1726/1766 VBs byte-identical**. The misses are content edits.

---------------------------------------------------------------------------------------------
## 2. PC textures

### 2.1 Table and header
`texture_data_offset` -> `num_textures` **packed 0x10-byte headers** (GC: 0x20 slots), all LE:

| off | type | meaning |
|---|---|---|
| 0x0 | u16 | format: **4 = DXT1**, **5 = DXT5**, 0 = RGB888, 1 = RGBA8888 (raw, row-major, GL_UNSIGNED_BYTE; not used in car packs) |
| 0x2 | u16 | width |
| 0x4 | u16 | height |
| 0x6 | u16 | **number of stored levels** if mipmapped, **full chain to 1x1** (`log2(max(w,h))+1`: 64 -> 7, 128 -> 8, 256 -> 9; 109/109); 0 = one level |
| 0x8 | ptr | pixel data (in pointers.bin), **32-aligned** (171/171) |
| 0xC | u32 | 0 |

Then a zero gap (0x330-0x380 bytes; GC has about twice that), then the pixel blocks in texture order, each at `align32(previous end)` (137/137).

### 2.2 Upload (exe 0x4678b0; struct at 0x6d3d44 + id*0x40: +0x24 data, +0x30 w, +0x32 h, +0x34 levels, +0x38 format)
* fmt 0/1: `glTexImage2D(GL_RGB/GL_RGBA, GL_UNSIGNED_BYTE)`, or `gluBuild2DMipmaps` when levels > 1.
* fmt > 1 with S3TC available: internal format = `fmt == 4 ? 0x83F0 (COMPRESSED_RGB_S3TC_DXT1) : 0x83F3 (COMPRESSED_RGBA_S3TC_DXT5)`.
  Then for each level: `size = ((w+3)&~3)*((h+3)&~3)` (halved for DXT1), `glCompressedTexImage2DARB(level, ifmt, w, h, 0, size, p)`,
  `p += size`, w and h halve (minimum 1). Levels are **contiguous with no padding**.
  Without S3TC the game decompresses on the CPU (0x46a590 for fmt 5, 0x46a4b0 otherwise) and uses gluBuild2DMipmaps.
* There is an option (flag `0x6d4d10 & 1`) that skips levels larger than 64.
* Because fmt 4 is uploaded as RGB DXT1, **DXT1 alpha is ignored**. Every texture with any transparency is DXT5.

### 2.3 Pixel layout
Standard DXT: 4x4 blocks in row-major order, no GX tiling. DXT1 block = `u16 c0, u16 c1` (LE RGB565), `u32` indices with pixel 0 in bits 1..0.
DXT5 = 8-byte alpha block (a0, a1, 48-bit 3-bit indices) + DXT1-style colour block (always 4-colour mode). The image orientation is the same as the GC decode (row 0 = top), so no flip is needed.

### 2.4 GC <-> PC mapping (164 paired textures, `texcmp.txt`)
| GC | PC | count |
|---|---|---|
| CMPR, opaque | fmt 4 DXT1 | 112 (+1 to DXT5) |
| CMPR with transparent texels (cut-out) | fmt 5 DXT5 | 41 |
| RGBA8 | fmt 5 DXT5 | 10 |
| mips 0 | mips 0 | always |
| mips n > 1 (stops at 16x16) | full chain, mips = log2+1 | always |

* **Colour:** GC texels are the PC texels **squared**: `gc = pc² / 255` per R, G, B. This wins on 129/129 non-flat textures
  (mean error 1-5 against 20-50 for identity, e.g. bins 144 -> 81, 176 -> 121; `gamma.txt`). The 35 "identity" cases are flat white/black 16x16 textures.
  Alpha is not squared. Vertex and group colours are **not** affected (they are identical).
* PC alpha (DXT5) is graded and anti-aliased. GC cut-outs are 1-bit and more opaque (GC's threshold looks lower than 128).
* DXT1 blocks are **not** a re-tiling of GC CMPR (2/117 equal): PC was encoded separately from the source art.
  A lossless re-tile function (`pctex.gc_cmpr_to_dxt1`) exists, but it would keep GC's squared colours, so use it only if you accept the darker look.
* PC mips are box-filtered from level 0 (mean error 0-8 against a 2x2 box of level 0).

### 2.5 GC -> PC texture recipe (`gc2pc_tex.py`, validated on 01_car_0, 02_car_1 and 15_car_14: format, mips field and size 14/14, mean RGB error 0-15 against the real PC texels)
1. Decode the GC texture (CMPR/RGBA8) to RGBA. `rgb = round(sqrt(255*rgb))`.
2. fmt = 5 if GC was RGBA8 or any alpha < 255, else 4.
3. If GC mips > 1: build the full box-filtered chain to 1x1 and set mips = number of levels. Else mips = 0 and one level.
4. Encode DXT1 (opaque, c0 > c1) or DXT5 per level and concatenate the levels unpadded. Place the data 32-aligned.
   Header `<4H fmt,w,h,mips> <I ptr> <I 0>`.
PC -> GC: decode DXT, `rgb = rgb²/255`, then the GC encoder (`research/gcenc.py`) with GC's mip rule.

---------------------------------------------------------------------------------------------
## 3. PC sounds (`sndcmp.py`, 145 sounds in 34 packs)
* The descriptor (32 bytes, `sfx_offset` 0) has the **same fields as GC**, LE: `ptr, length(bytes), loop(bytes or ~0), rate, u8 0, u8 bits=16, u16 vol 0x7FFF, 12 zero bytes`.
  ptr/len/loop/rate/bits/vol are equal to GC in 145/145.
* Samples: **signed PCM16 little-endian mono**, sample-for-sample identical to GC's BE data (145/145).
* `sound_pointers.bin` is identical (`0x20*i`). The first sample offset is identical to GC (0x740-0x7A0; zero gap after the descriptors).
* Alignment: each next sample is at **align16(previous end)** (111/111; GC uses align32). The PC inter-sample padding is junk (non-zero in 23 packs), so write zeros.
  The region ends 2-4 bytes after the last sample.
* `sound_relocs` are identical (indices, pack ids, order).
* Conversion: swap every s16, re-pack at 16-byte boundaries, fix the descriptor ptrs, swap the descriptor fields (u32 x4, u16 vol).

---------------------------------------------------------------------------------------------
## 4. Other PC vs GC car-pack differences
* **Pack header** (`pack.json`): all fields equal except `texture_data_offset` (and `num_textures` in 04_car_3 and 14_car_13, art).
  `refs`, `num_sounds`, `num_shaders`, palettes, song and trigger fields are equal.
* **Blob order** is the same: `root {u32 1, ptr}` at 0 (pointing to a slot at 0x10 on PC, 0x20 on GC, which holds the CarAInfo ptr) | geometries (same order) |
  small anim/skeleton chunk | texture table + pixels | object graph (CarAInfo, parts, anims, particles, sounds, AI, HUD).
* **Object graph** (`objswapcmp.py`, which swaps GC with `research/swapmap.py`, ignores zero padding and aligns word sequences): 375,552 / 401k non-zero words
  (**93.7%**) are identical after swapping, over 33 cars. **No struct layout differs.** The remainder:
  * pre-reloc junk in texture/sound reloc slots (PC writes `0x0012xxxx`, GC writes other junk): ObjSub, Particle.anim, ...;
  * non-pointer geometry offsets (MotionPart[]/CarPart[]/ThingRecord/EventSeq, CarAInfo+0x54 shadow geometry, anim geometry list).
    They hold blob offsets that are not in pointers.bin and must be **remapped** to the new geometry offsets (the "relocated elsewhere" fields of the GC doc §1.8);
  * fields swapmap mis-types (u8 arrays in Sound/SoundSeq/keys.color, chanflags) that turn out equal raw;
  * **HUD content**: IMObject/ObjAnim/StateAnim/TextInfo have a few different values and extra entries on GC (about 84 words per car). Same structs;
  * 30_car_29 has extra GC-only geometry (art).
* **Alignment:** every object (and pointer target) is 16-aligned on PC (32 on GC); padding is zero. Texture pixels are 32-aligned on both.
* **Relocs:** `ref_relocs` are identical except in 1 pack. `texture_relocs`: GC has extra entries on **shadow groups** (16-29 per car). PC has none,
  so drop them in GC->PC and don't add them in PC->GC (or add 2:20 to match GC). `pointers.bin`: PC has about 50-260 more slots (the VB array pointers).
* data.bin is 8-37% larger on PC (16-byte pos/nrm, unified vertices).

---------------------------------------------------------------------------------------------
## 5. Uncertainties
1. **Hidden vertex attributes** (GC->PC): retail normals of alpha/unlit vertices and the rare non-white colours of env vertices can't be recovered,
   and neither can the exporter's leftover duplicate vertices. I believe they don't render (lighting is enabled only in the lit draw path),
   but I haven't checked every PC draw path (env and alpha at 0x4696a0) in detail.
2. **Shadow boundary edges** (n1 = 0) can't be rebuilt from GC data. I haven't checked how the PC silhouette code treats them. GC works without them.
3. **Skinned vertex order inside a bone** (car_29 only) is unresolved. The per-bone grouping and the count table are confirmed.
4. **Gamma:** `gc = pc²/255` is an empirical fit (very tight, but rounding is unknown). GC->PC loses precision in dark texels.
   The PC DXT encoder and alpha cut-off used by the original tools are unknown.
5. Texture format 0/1 (raw) and index-list strip mode exist in the exe but are not used by any car pack, so they are untested.
6. PC pre-reloc slot value `0x00120000|idx`: the meaning of 0x12 is unknown. It is overwritten at load.
7. The PC texture-table loader itself (the PC equivalent of LoadBTF_RAM) was not located in the exe. Header semantics come from the upload function and the data.
