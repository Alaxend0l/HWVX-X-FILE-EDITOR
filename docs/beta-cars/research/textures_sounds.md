# HWVX textures & sounds: PS2 prototype (v179, LE) -> GameCube (v183, BE)

Every claim below was checked against the code (HWVX-SDL/src/game) or against all 36 car packs
(+ `00_share_2`, HW_share) in both builds. Probe scripts are in this folder:

| script | what it does |
|---|---|
| `texlib.py` | GC (CMPR/RGBA8) and beta (8-bit CLUT / RGBA32 / RGB24) decoders, header readers |
| `texstats.py` | survey: formats, sizes, mip semantics (pointer gaps), palette alpha histogram |
| `gcenc.py` | **reference encoder** beta -> GC record (CMPR / RGBA8 + mips) with a round-trip decode check |
| `vag.py`, `sndstats.py` | PS2 SPU-ADPCM decoder, GC PCM16 reader, WAV writer; SFX descriptor survey |
| `sndmatch.py`, `sndpair.py`, `sndmatch_lib.py` | cross-correlation pairing of beta and GC sounds |
| `sharemap.py` -> `share_index_map.json` | candidate beta->GC index maps for the shared packs (pack ids 1, 2) |

Evidence images: `sheet_car_AT.png` (row 1 GC 02_car_1, row 2 beta 08_car_7 with CSM1 unswizzle,
row 3 beta *without* the unswizzle, which is visibly wrong), `sheet_roundtrip_08_car_7.png` (top: beta decode,
bottom: after GC encode -> decode), `tex/*.png` (each texture separately, alpha planes, round trips),
`snd_pair_overlay.png` (waveform overlay of a paired sound), `snd_*.wav`.

Extra extractions made for this work: `SCRATCH/beta_share/00_share_1` and `SCRATCH/gc_share/00_share_1`
(HW_share, pack id 1), made with `target/release/hwvx-x extract`.

---------------------------------------------------------------------------------------------------

## 1. GameCube texture records (v183)

### 1.1 Loader path
`DataPack_RefreshTextures` (DataGamePackLoad.c:478):
```
tex = base + texture_data_offset
for i in 0..num_textures:
    textureIDs[i] = Gfx_LoadTexture_RAM(&tex)   // LoadBTF_RAM: reads a 0x10 header, tex += 0x10
    if (tex & 0x1F) tex += 0x20 - (tex & 0x1F)  // so the stride between headers is 0x20
```
The headers form a contiguous table at `texture_data_offset`, one per 0x20 bytes (0x10 header + 0x10 zeros).
The pixels are **not** inline. Each header points to them.

### 1.2 Header (0x10 bytes, padded to 0x20)
| off | type | meaning |
|---|---|---|
| 0x0 | u16 **LE** | format: `1` = GX_TF_RGBA8, `4` = GX_TF_CMPR (`Texture.unk38`) |
| 0x2 | u16 **LE** | width (`Texture.unk30`, passed as GXInitTexObj width) |
| 0x4 | u16 **LE** | height (`Texture.width`, passed as GXInitTexObj height) |
| 0x6 | u16 **LE** | mip **level count** (`Texture.height`). 0 or 1 = no mips; n>1 = n levels stored |
| 0x8 | u32 BE | pointer to pixel data (blob offset, listed in `pointers.bin`) |
| 0xC | u32 BE | `Texture.unk28`, always 0 in all 173 GC car/share-car textures |
| 0x10..0x1F | | zero padding |

Even on GC the four u16s are little-endian. LoadBTF_RAM byte-reverses them with `__sthbrx`, and Port_Swap.c treats them that way too.

### 1.3 Supported formats
`Render_CreateTextureSlot` (Render_Texture_.c) handles only:
* `1` -> GX_TF_RGBA8 (6). The game forces `levels = 0`, so **only level 0 is used** even if the header says 3 or 4.
  In the GC files these textures store only level 0 (pointer gaps = w*h*4). Flush size is w*h*4.
* `4` -> GX_TF_CMPR (0xE), with mips when levels > 1: `GXInitTexObjLOD(LIN_MIP_LIN, LINEAR, 0, (levels-1)/1.15, ...)`.
* `0`/`2` reach `GXInitTexObj` with an **uninitialised format**. They are not valid in packs. There is no TLUT/CI path at all.
* Width or height > 0x200 is rejected (the slot is left empty).

`LoadBTF` (disk `.btf`: bpp 8/9/12/24/32 with an inline palette) and `LoadTGA` are file loaders and are not used for packs.
**`Gfx_LoadPalettes_RAM` is an empty function on GC.** Palettes in packs are dead data, and every GC pack has `num_palettes = 0`.

### 1.4 Pixel layout (GX tiling, verified by decode, `sheet_car_AT.png` row 1)
* **CMPR**: 8x8 macro-tiles in row-major order. Each tile is 4 DXT1 sub-blocks of 8 bytes in the order TL, TR, BL, BR.
  A sub-block is `u16 BE c0 (RGB565), u16 BE c1, 4 bytes of 2-bit indices`, one byte per row,
  **pixel x=0 in bits 7..6** (MSB-first, the reverse of PC DXT1). `c0 > c1` gives 4 colours
  (c0, c1, (2c0+c1)/3, (c0+2c1)/3). `c0 <= c1` gives 3 colours + transparent (c0, c1, (c0+c1)/2, alpha 0).
  Level size = `ceil(w/8)*8 * ceil(h/8)*8 / 2`.
* **RGBA8**: 4x4 tiles of 64 bytes, row-major. Within a tile: 16 (A,R) byte pairs, then 16 (G,B) byte pairs,
  pixels row-major. Level size = `ceil(w/4)*4 * ceil(h/4)*4 * 4`.
* Mip levels follow each other with no padding (CMPR levels are multiples of 32 B anyway). Size = sum over levels of the level size,
  halving w and h each level (see `TexSize` in Port_Swap.c).

### 1.5 What GC car packs actually use (173 textures surveyed)
| fmt | w x h | header mips | count |
|---|---|---|---|
| CMPR | 16x16 | 0 | 37 |
| CMPR | 32x32 | 2 | 3 |
| CMPR | 64x64 | 3 | 50 |
| CMPR | 128x128 | 4 / 0 | 25 / 1 |
| CMPR | 256x256 | 5 / 0 | 28 / 19 |
| RGBA8 | 64x64 | 3 (1 level stored) | 7 |
| RGBA8 | 128x128 | 4 / 0 (1 level stored) | 2 / 1 |

* All textures are square powers of two, so the width/height order at 0x2/0x4 can't be confirmed from data (see §5).
* **Mip rule:** chains stop at 16x16, so `levels = log2(size) - 3` (64 -> 3: 64, 32, 16), or 0 for no mips.
* **Format rule:** CMPR for opaque textures (118) and cut-out alpha (45, 1-bit). RGBA8 for real translucency (all 10 have graded alpha:
  windows, glass, shadows).
* Pixel data offsets are 32-aligned (the blob base is 32-aligned at load: `data` is 64-aligned and the reloc tables are padded to 32).
  The pixel block starts after the header table plus a zero gap (~0x680-0x6E0 bytes, not required), in texture order.

### 1.6 Texture index -> relocations
`texture_relocs.bin` holds u32 pairs `(blob offset, idx | packId<<16)`. After loading,
`*(u32*)(base+off) = packId ? refs[packId].pack->textureIDs[idx & 0xFFFF] : textureIDs[idx]`. `textureIDs[i]` is the runtime
`m_texture` slot that `Gfx_LoadTexture_RAM` returns for the i-th header. So a **local index is a position in the header table**.
If the converter keeps beta texture order 1:1 (one GC header per beta header), local texture reloc indices carry over unchanged.
Quad packs use the same scheme (`DataPack_RefreshQuadPackPtrs`). Pack id 1 = HW_share, 2 = HW_Cars `00_share_2` (pack.json `refs`).
**Shared indices differ between builds** (beta HW_share has 82 textures, GC 81; beta 00_share_2 has 20, GC 27), so they must be remapped
(see §3.4).

---------------------------------------------------------------------------------------------------

## 2. Beta (PS2 prototype v179) textures and palettes

### 2.1 Header table (0x10 bytes, **packed, no 0x20 padding**), at `texture_data_offset`, all LE
| off | type | meaning |
|---|---|---|
| 0x0 | u16 | format: 0 = RGB24, 1 = RGBA32, 2 = 8-bit indexed (car packs: 123x fmt 2, 2x fmt 1 in 00_share_2) |
| 0x2 | u16 | width |
| 0x4 | u16 | height |
| 0x6 | u16 | mip value `m`: **stored levels = m - 1** (m = 0 means 1 level) |
| 0x8 | u32 | pixel pointer (in `pointers.bin`; 16-aligned, *not always 32-aligned*) |
| 0xC | u32 | CLUT index: palette = `palette_offset + idx*0x400`. In car packs idx == texture index; in share packs CLUTs are shared |

`m - 1` was proven from the pointer gaps: 64x64 m=4 -> 4096+1024+256 = 5376 B (3 levels); 32x32 m=3 -> 1280 (2 levels);
256 m=9 -> 8 levels (down to 2x2) + 16 B pad. Beta values seen: 64 -> 4 or 7, 128 -> 5 or 8, 256 -> 6 or 9. So "m-1 = log2-3"
(down to 16x16, like GC) or "full chain to 2x2".

### 2.2 Pixels
* **Linear, row-major, not GS-swizzled.** Proof: `sheet_car_AT.png` row 2 decodes correctly with a plain `y*w+x` read.
* fmt 2: 1 byte per pixel, an index into a 256-entry CLUT. fmt 1: bytes R,G,B,A. fmt 0: R,G,B.
* Mip levels follow level 0 contiguously, each `w*h*bpp/8`, halving each level.
  Many beta mips are point-sampled and noisy (`tex/mipcheck_14_t3.png`). Regenerating mips from level 0 is recommended.

### 2.3 CLUTs
* `num_palettes` entries of 0x400 bytes (256 x 4: bytes **R,G,B,A**) at `palette_offset`. No header, and nothing points at them
  (only the CLUT index in the texture header refers to them).
* **CSM1-swizzled:** logical index `i` is stored at `i ^ ((i>>3 & 1) != (i>>4 & 1) ? 0x18 : 0)`. That is the same as swapping bits 3 and 4:
  `stored = (i & ~0x18) | ((i & 8) << 1) | ((i & 0x10) >> 1)`. Proof: grey ramps in raw CLUTs jump at entries 8..15 / 16..23,
  and decoding without the unswizzle gives the broken row 3 of `sheet_car_AT.png`.
* **Alpha is already 0..255, not PS2 0..0x80.** Histogram over every beta car CLUT: 28,574 entries at 255, 535 at 254,
  and a full spread 0..253 (only 40 at 128). RGBA32 textures also reach 255. Use the alpha as-is (no x2).

### 2.4 Paired proof (same car: beta 08_car_7 <-> GC 02_car_1)
| beta | GC | notes |
|---|---|---|
| t0 256 fmt2 m0 (skin) | t0/t4 256 CMPR mips0 | same art; GC skin recoloured red ("at_skinV2") |
| t1 32 m3 (paint) | t1 32 CMPR mips2 | colour differs (V2 paint) |
| t2 64 m4 (windows, alpha 92..255) | t2 64 **RGBA8** (alpha 85..255) | translucent -> RGBA8 |
| t3 32 m3 (grey) | (t5 16x16 white) | no exact match |
| t4 64 m4 (wheel, mostly 0/255 + AA edge) | t3 64 CMPR mips3, binary alpha | cut-out -> CMPR |

Running `gcenc.py ../beta_cars/08_car_7` produces exactly GC's format, mip count and size for each counterpart:
CMPR 256 mips0 32768 B, CMPR 32 mips2 640 B, RGBA8 64 16384 B, CMPR 64 mips3 2688 B.

---------------------------------------------------------------------------------------------------

## 3. Recommended texture conversion

### 3.1 Per texture (keep order and count; one GC header per beta header)
1. Decode level 0 to RGBA8888. fmt 2: `rgba = clut[unswizzle(idx)]` with CLUT `clutIndex` from header 0xC. fmt 1/0: direct.
   Keep alpha unchanged.
2. Choose the GC format:
   * `partial = fraction of pixels with 8 < A < 247`.
   * `partial > 0.25` -> **RGBA8** (fmt 1). Otherwise **CMPR** (fmt 4), with alpha thresholded at 128.
     This reproduces GC's choices on the paired car. The threshold is a heuristic (§5). Names containing `window`, `glass` or `bkglass`
     are good candidates to force to RGBA8.
3. Mips:
   * CMPR: `levels = (m == 0) ? 1 : min(m - 1, log2(min(w,h)) - 3)`, with a minimum of 1. Header mips = `levels == 1 ? 0 : levels`.
     Build level k by 2x2 box-filtering level k-1 (RGBA). Optionally reuse beta's stored levels (they exist for every k < m-1).
   * RGBA8: store level 0 only. Header mips = 0. (GC sometimes writes 3/4 here, but the game ignores it.)
4. Encode:
   * **CMPR** (`gcenc.enc_cmpr`): pad to a multiple of 8. For each 8x8 tile (row-major), for each sub-block in TL, TR, BL, BR order:
     * Opaque block: pick 2 endpoints (principal axis extremes, or any DXT1 encoder), quantise to RGB565
       (`r5 = (r*31+127)/255`, etc.). Ensure **c0 > c1** (swap if needed). If c0 == c1, write index 0 everywhere.
     * Block with any A < 128: ensure **c0 <= c1** and use palette {c0, c1, mid}. Index 3 = transparent.
       All-transparent: `c0 = c1 = 0`, indices 0xFFFFFFFF.
     * Write `c0` and `c1` as BE u16, then 4 index bytes, one per row: `byte = i0<<6 | i1<<4 | i2<<2 | i3`.
     * A libsquish / DXT1 encoder can be used if you convert its output: byte-swap the colours to BE and, within each
       row byte, reverse the 2-bit order. (PC DXT1 stores the indices as an LE u32 with pixel 0 in the low bits.)
   * **RGBA8** (`gcenc.enc_rgba8`): for each 4x4 tile: `A0 R0 A1 R1 ... A15 R15` then `G0 B0 ... G15 B15`.
5. Write the header: `<HHHH fmt,w,h,mips` (**LE**), `>I ptr`, `>I 0`, 16 zero bytes. Put pixel data at a **32-aligned**
   blob offset and add header+8 to `pointers.bin`.

### 3.2 Pack-level
* GC texture table: `texture_data_offset` -> `num_textures * 0x20` bytes of headers. Then the pixel blocks (32-aligned, in order).
* `num_palettes = 0`, `palette_offset = 0`. Drop the beta CLUT block and the beta 16-byte header table.
* Beta pixel pointers are only 16-aligned, so data **must** be re-laid out, not just converted in place.
* Size changes: 8-bit 256x256 (64 KiB) -> CMPR 32 KiB (+ mips 43,648 B). 64x64 translucent -> RGBA8 16 KiB.

### 3.3 Round-trip validation
`gcenc.py <beta pack>` encodes every texture, checks the byte size against the GC size formula, decodes with the
GC decoder and writes `tex/<pack>_t<i>_roundtrip_gc.png` + `sheet_roundtrip_<pack>.png`. Mean abs RGB error is 0-5 for opaque textures,
and ~10 for cut-outs (RGB under transparent pixels is not kept). Tested on 08_car_7, 04_car_3, 31_car_30.

### 3.4 Shared-pack indices (`texture_relocs` / `sound_relocs` with packId != 0)
Beta cars reference HW_share (1) tex 0..81 and 00_share_2 (2) tex 0..18, and 00_share_2 sounds 0..49. These need
beta->GC index maps. `share_index_map.json` (from `sharemap.py`) gives the candidates:
* **Sounds, pack 2:** reliable (NCC ~0.98-0.997): beta 8,9,10 -> GC 0,1,2; beta 12..43 -> GC 3..38 (beta 18 -> 9 at 0.84 needs a check);
  beta 48,49 -> GC 46,47. Beta 0..7, 11 and 44..47 have **no GC counterpart** (GC 10..13 and 39..45 are new).
* **Textures:** image-feature matching. Many are clear (score > 0.95 with a distinct runner-up), but flat or blank textures and
  colour variants are ambiguous. This needs a manual pass using the PNGs (decode both share packs with texlib).

---------------------------------------------------------------------------------------------------

## 4. Sounds

### 4.1 Region layout (both builds)
`sound_pointers.bin` = u32 offsets (relative to soundBase) of the pointer slots in the sound region. In car packs these are
exactly the descriptor `data` fields: 0, 0x20, 0x40, ... `soundBase` = start of `sound_data.bin` (32-aligned at runtime:
`(sfx + numSoundPointers*4 + 0x1F) & ~0x1F`). `UpdatePointers` adds soundBase. Descriptors are at `soundBase + sfx_offset`
(0 in all cars), 32 bytes each, `num_sounds` of them. Samples follow. GC puts the first sample at 0x740-0x7A0 (0x600 for the
48-sound share pack), with zeros between the descriptors and the first sample. Beta does the same at 0x640-0x6C0.
`song_offset`/`song_bank_offset` = 0 in every car pack (song = IT module + bank, only in world/music packs).
Loader caveat: if `soundDataSize != 0 && numSoundRelocs == 0`, `pack->song` is set to soundBase. That is harmless while songBank = 0.

### 4.2 GC SFX descriptor (32 bytes, BE). `SFX` in Voice.h, swapped by `PortSwap_SFXDescriptors`
| off | type | meaning | GC car values |
|---|---|---|---|
| 0x00 | u32 ptr | sample data (soundBase-relative, slot listed in sound_pointers) | 32-aligned |
| 0x04 | u32 | length in **bytes** (CreateSFX_ halves it for 16-bit) | even |
| 0x08 | u32 | loop start in **bytes** from data, `0xFFFFFFFF` = one-shot | 0 (97x) or ~0 (48x) |
| 0x0C | u32 | sample rate Hz | 11025 / 22050 |
| 0x10 | u8 | unk10 | 0 |
| 0x11 | u8 | bits: 8 = PCM8 (signed), 16 = PCM16, else DSP-ADPCM | **16 (all 145)** |
| 0x12 | u16 | volume 0..0x7FFF | 0x7FFF |
| 0x14 | 12 B | pad | 0 |

Samples: **signed PCM16 big-endian, mono** (port `FetchSample` reads `Port_ReadBE16`, and the port swap leaves sample bytes alone).
`CreateSFX_` DMA-copies `(length+0x1F)&~0x1F` bytes to ARAM. Sample starts must be 32-aligned, with readable padding after each
sample (GC: zero pad, next sample at the next 32 boundary). AX: `end = addr + len/2 - 1`, `loopAddr = addr + loop/2`, `ratio = rate*pitch/48000`.
DSP-ADPCM is **not usable**: the descriptor has no coefficient/predictor fields, and the port mixer has `PORT_TODO("ADPCM voices")`.

`sound_relocs.bin` (`off, idx | packId<<16`): writes `soundIDs[idx]` (the CreateSFX handle) into the blob. Same rule as for textures.

### 4.3 Beta SFX descriptor (32 bytes, LE)
| off | type | meaning | values |
|---|---|---|---|
| 0x00 | u32 ptr | VAG data (soundBase-relative) | 16-aligned |
| 0x04 | u32 | length in bytes (multiple of 16) | |
| 0x08 | u32 | loop start, **byte offset of a VAG block**, `0xFFFFFFFF` = one-shot | 59 loop (16 or mid) / 53 one-shot |
| 0x0C | u32 | rate Hz | 11025-11041, 22050-22103, 44145 (pitch-tuned values) |
| 0x10 | u16 | 0x1000 (SPU pitch 1.0). The bytes are `00 10`, which happen to equal GC's `unk10=0, bits=16` | always |
| 0x12 | u16 | volume 0x7FFF | always |
| 0x14 | 12 B | 0 | |

Samples: **headerless PS2 SPU ADPCM (VAG body)**, 16-byte blocks = `[shift | filter<<4][flags][14 bytes = 28 nibbles, low nibble first]`.
Survey of all 112 car sounds:
* Block 0 is always all-zero (28 silent samples).
* Looping sounds: the block at `loop/16` has flags 6 (loop start), the others 2, and the last has flags 3 (loop end + repeat).
* One-shots: all flags 0, last block 1.

Decoder: `s = (nib<<12 >> shift) + (s1*f0 + s2*f1 + 32 >> 6)`, clamp to s16,
with filters `(0,0),(60,0),(115,-52),(98,-55),(122,-60)` (`vag.py`).

### 4.4 Validation (paired sound)
Beta 08_car_7 sfx1 (VAG 4928 B, 8624 samples @22062, loop 16) vs GC 02_car_1 sfx1 (PCM16 17150 B, 8575 samples @22050, loop 0).
After shifting 40 samples (block 0 + 12): NCC 0.927 (the fractional-sample offset limits it), gain 0.93, RMS 11754 vs 11817.
`snd_pair_overlay.png` shows the waveforms on top of each other. The same pair appears in 16 beta cars and in GC 32_car_31.
In `00_share_2`, 38 sounds pair at NCC 0.98-0.997, which confirms the VAG decoder and the PCM16-BE reading.

### 4.5 Recommended sound conversion
For each beta descriptor, in the same order:
1. Decode all VAG blocks to s16 (`vag.py`). **Drop block 0** (always 28 zero samples) so the result matches the GC
   sounds, which start directly and loop from 0.
2. `pcm` = the remaining samples. Length = `2 * len(pcm)` bytes.
3. Loop: `loop == 0xFFFFFFFF` -> keep `0xFFFFFFFF`. Otherwise `loop_gc = 2 * ((loop/16 - 1) * 28)`
   (beta loop 16 -> 0, the common case). If you keep block 0, use `2 * (loop/16) * 28` instead.
4. Rate: copy the beta value (it is within 0.5% of 11025/22050). Snapping to 11025/22050 is also fine.
5. Descriptor (BE): `data` = 32-aligned offset in the new sound region (add to sound_pointers), `length`, `loop`, `rate`,
   byte 0x10 = 0, byte 0x11 = 16, u16 0x12 = 0x7FFF (copy beta vol), 0x14..0x1F = 0.
6. Samples: s16 **big-endian**, zero-padded to a 32-byte boundary.
7. The region is the descriptors at 0 (`sfx_offset = 0`) and then the samples. The beta/GC gap before the first sample can be kept
   (e.g. first sample at 0x760) but is not required. `sound_pointers` = `[0x20*i]`. Song offsets 0.
8. Remap shared sound indices (pack 2) using §3.4.

Size: PCM16 is about 3.6x the VAG size. Beta 31_car_30 (75 KB) -> about 265 KB, against the GC max of about 113 KB for a car.
ARAM is about 16 MB with a first-fit allocator (Audio_.c), so this should fit. PCM8 (`bits = 8`, signed) halves the size
if needed (the port supports it), at a loss of quality.

---------------------------------------------------------------------------------------------------

## 5. Uncertainties
1. **Width/height order** at header 0x2/0x4: from code, 0x2 = GX width and 0x4 = GX height (Port_Swap.c agrees, and so does the beta doc's
   x/y). There are no non-square textures in either car set to confirm it.
2. **RGBA8 vs CMPR threshold** (`partial > 0.25`) is a heuristic fitted to one paired car. GC's tool probably decided from
   source TGA metadata. Textures with partial alpha around 0.05-0.25 (e.g. 31_car_30 t3, 0.208) could go either way.
   Forcing windows/glass to RGBA8 is the safer choice.
3. **GC mip depth**: GC stops at 16x16 (`log2-3`). Deeper CMPR chains should work in hardware (GXInitTexObjLOD max LOD
   follows the count) but are untested in the port. Mip filter quality (box filter vs beta's levels) is a matter of taste.
4. **Beta `m-1` level rule** is inferred from pointer gaps. The extra count (`m` vs levels) has no known runtime meaning.
5. **Zero gaps** (after the GC texture header table, before the first sample in the sound region) are not explained. They are not read by the loader.
6. **Rate values** like 22062: kept as-is. The GC versions of the same sounds are 22050. Not audible either way.
7. **Shared texture index map** (pack 1 and pack 2) is a set of automatic candidates. It needs manual verification. Several beta shared
   sounds and textures have no GC equivalent (substitute or drop the reloc).
8. CMPR encoder quality: `gcenc.py` is a correctness reference (axis-extreme endpoints), not an optimal encoder.
9. Beta RGBA32 (fmt 1) byte order is assumed to be R,G,B,A. The two such textures (00_share_2 t4/t10) decode plausibly, but no car pack uses fmt 1/0.
