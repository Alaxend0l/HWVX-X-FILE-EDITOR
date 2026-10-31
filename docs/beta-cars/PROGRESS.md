# Porting PS2-prototype cars to the GameCube build — progress log

Goal: add three cars that exist only in the leaked PS2 prototype —
**Fandango**, **Sweet 16 II**, **Jester** — to the GameCube release
(`HW_Cars.xgc`) as *new* cars (pack ids 33, 34, 35), leaving the 33 existing
cars untouched. Test bed: `HWVX-SDL` (build, then run with
`--data HWVX-SDL/orig/files`).

## 1. What the cars are (2026-10-31)

`hwvx-x info orig/ps2-beta/XPS/ENGLISH/HW_CARS.XPS` lists 36 car packs
(GC has 33). Identified by texture names / strings in each pack's data:

| Car | Beta pack | Evidence |
|-----|-----------|----------|
| Fandango | `car_1` (entry 02) | string `fandango` ×46 |
| Sweet 16 II | `car_12` (entry 13) | texture `sweet16.tga` |
| Jester | `car_17` (entry 18) | texture `jester256.tga` |

Cars present in **both** builds (useful as a "Rosetta stone" when comparing
formats), matched by texture names, e.g. beta `car_7` (AT_*) = GC `car_1`.

## 2. Format differences found so far

| Layer | PS2 prototype (v179) | GameCube (v183) |
|-------|----------------------|-----------------|
| Archive / pack container | handled by `hwvx-x` (see README) | handled by `hwvx-x` |
| Byte order | little-endian | big-endian |
| `CarAInfo` (root object) | same field offsets as GC (first 0x100 bytes checked) | — |
| u16 texture ids in records | low half valid, high half `0xCCCC` garbage | clean |
| Geometry | PS2 model: header `u16 0x1005`, VIF UNPACK packets | GX display lists + vertex buffers |
| Textures | palettized + CLUTs (`num_palettes` > 0) | GC BTF textures, no palettes in car packs |
| Sounds | PS2 (VAG) | GC |

### Textures (confirmed by decoding the paired AT car in both builds)

- GC: one 0x20-byte header per texture at `texture_data_offset`
  (u16 format, width, height, mip count — little-endian even on GC — then a
  big-endian pixel pointer). Only format 1 (RGBA8, level 0 only) and 4
  (CMPR, mips down to 16×16) are used; GC has no palette support.
  Pixel data is 32-byte aligned.
- Beta: 16-byte LE headers, field 0xC = palette index; pixels are linear
  8-bit indices (not GS-swizzled); palettes are 256 × RGBA in the PS2 CSM1
  order (index bits 3 and 4 swapped); alpha already 0..255.
- Conversion: RGBA8 when >25 % of pixels are semi-transparent, otherwise
  CMPR (alpha cut at 128). On the paired car this reproduces GC's own
  format, mip count and size choice for every texture.

### Sounds (confirmed by waveform comparison, NCC 0.93–0.997)

- GC: 32-byte BE descriptor (data pointer, length in bytes, loop start in
  bytes or `~0`, sample rate, byte 0x11 = 16), samples PCM16 big-endian.
- Beta: LE descriptor, headerless PS2 VAG ADPCM; block 0 is silent.
- Conversion: decode VAG, drop block 0, loop start
  `2*28*(loop/16 - 1)`, write PCM16 BE padded to 32 bytes.

### Shared-pack references

Cars import textures/sounds/objects from `HW_share` (pack 1) and the
`HW_Cars` shared pack (pack 2) by index, and the indices differ between
builds, so imports must be remapped (matched by name/content).

## 3. How cars are chosen in the GC game

- Car select (`GameSpecificInterfaces.c: GSI_VehicleItemSelection`) takes
  the pack id from a list supplied by the interface pack
  (`msg->list[selection]`) and loads `HW_Cars` type 0, that id.
- Unlock state: code variable `0x64`, `HotWheelsGameData.cars[0x21]`
  (33 entries, part of the save data) — code change needed for more cars.

## 4. Test harness (HWVX-SDL)

- `HWVX_CHEATS=1` – everything unlocked.
- `HWVX_AUTOINPUT="frame:button*hold,..."` – scripted pad input.
- `HWVX_DUMP=dir` – writes a BMP every 60 frames (view after
  `magick frameNNNNN.bmp out.png`).
- `HWVX_NO_DIALOG=1` – fatal errors only logged.

## 5. Tools built so far

- `src/blob.rs` – editable pack object data (append / re-point / drop
  objects, keep pointer & relocation tables consistent) and a per-byte
  `SwapMap` for byte-order conversion.
- `src/convert/texture.rs` – prototype texture decoder (CLUT + CSM1),
  GC CMPR / RGBA8 encoders and decoders, mip generation, TGA output.
- `src/convert/sound.rs` – VAG ADPCM decoder, GC PCM16 sound-region
  builder, WAV output.
- `hwvx-x textures <pack folder> <out>` – dump a pack's textures as TGA
  (works for GC and prototype packs). All three target cars' textures
  decode correctly (body skins with the Fandango / Sweet 16 / Jester art,
  wheels).

## Next

Research in progress: geometry format, textures & sounds, the remaining
object structures and cross-pack imports, and the car roster / interface.
