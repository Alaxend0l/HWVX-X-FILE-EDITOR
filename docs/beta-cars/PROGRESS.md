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

## 6. Research results (details in `research/`)

- **Geometry** (`research/geometry.md`): validated on 437 models that exist
  in both builds (72 % of triangles identical, 21 % differ only by whole-unit
  UV shifts, the rest are GameCube art edits). Strips decode with GS rules,
  including strips that continue across the 79-vertex batch limit.
- **Object structures** (`research/objects.md`): every object type in a car
  pack has the *same layout* in both builds (the prototype aligns objects to
  16 bytes, GameCube to 32 — harmless). So the object graph is byte-swapped
  in place; a Rust port of the swap walker (`src/convert/carwalk.rs`) matches
  the Python reference byte-for-byte on all three cars.
- **Shared-pack references**: indices into `HW_share` / the `HW_Cars` shared
  pack differ between builds → `data/beta-imports.json` (built from 16 cars
  present in both builds plus image/sound matching).
- **Roster** (`research/roster.md`): the GameCube car-select interfaces
  already hold 36 name / thumbnail slots; only the carousel range (max car id
  32) and the names had to change.

## 7. First in-game test (HWVX-SDL)

`port-car` + `roster` + `pack`: all three cars load, show on the car-select
turntable with their textures, and race (HUD, speed, points, checkpoints).
Two problems were reported and fixed:

| Problem | Cause | Fix |
|---------|-------|-----|
| Race camera stuck above the car | camera parameter blocks are 0x18 bytes; the transplant copied 0x10 and packed them 0x10 apart, so the chase camera's last two values were overwritten | copy each block up to the next object |
| Cars too low on the car-select turntable | `CarAInfo.viewOffset` (0x00) lifts the car on interface screens; 0 in every prototype car, 0.5–3.5 (median 2.0) on GameCube, tuned per car | `port-car --view-offset` (default 2.0) |

Also changed: damage tuning (`damageLevel` 1 → 20, `damagePercent` 99 → 75,
the values every GameCube car uses; the prototype lost parts on every hit).

## 8. Thumbnails and names

The thumbnails that were already in the GameCube data for slots 33–35 show
three *other* unreleased cars (their placeholder names were "HW Prototype
12", "Power Rocket", "Jet Threat 3"), not these. `hwvx-x render-car` (a
small software renderer: model parts posed on the car's skeleton, plus its
breakable body panels, textured) renders each converted car; `roster --cars`
writes the renders into the thumbnail textures (128×128 RGBA8, stored upside
down like the game's own).

Names come from each prototype pack's own art: `car_1` has the "Fandango"
graffiti livery, `car_12` the `sweet16.tga` skin, `car_17` `jester256.tga`.

## 9. Unlocking (code)

HWVX-SDL `GameSettings.c` / `common.h`: unlock flags for cars 33–35 live in
3 bytes of unused save padding (`carsExt`), and the cars always report as
unlocked (nothing in the game would unlock them). Save format unchanged.

## 10. Results screen showed Jester as "Overbored 454"

Reported from play: after a drag race the results named Jester "Overbored
454". The 20 drag/battle worlds carry their own results screens with their
own 36-entry car name lists, indexed by per-player interface variables whose
range was 0–32 — car 35 wrapped to 35 − 33 = 2 (Overbored 454), and entries
33–35 still held the placeholder names. `roster` now patches every car name
list in interface *and* world packs (names, plus the range of the variable
that indexes the list), and accepts a whole tree of extracted archives.
The 20 worlds are installed (originals in `HWVX-SDL/orig/backup/worlds_/`).

## Status (2026-10-31)

Installed in `HWVX-SDL/orig/files/` (`HW_Cars.xgc`, `HW_IPack.xgc`;
originals in `HWVX-SDL/orig/backup/` and `orig/gc/files/`), HWVX-SDL
rebuilt. Waiting for in-game feedback on the fixes above.

Open items:
- AI opponents never pick the new cars: 14 race/battle worlds limit random
  AI cars to ids 0–32 (`maxVehicle` at world car slot +0x37).
- A real GameCube needs the unlock change patched into the DOL (not done).
- Turntable height is one default for all three; tune per car with
  `--view-offset` if any sits too high/low.
- Converted car sounds are PCM16 (≈3.6× the size of VAG); fine in the port,
  untested on hardware.

---

# Part 2: the PC version (`.xpc`)

Goal: the same three cars as new cars 33–35 in `orig/pc/HW_Cars.xpc`, tested
with `orig/pc/Hot Wheels.exe` (runs under Wine; see "Running the PC game"
below).

## P1. First look (2026-10-31)

- PC is the same release as GameCube (v183), little-endian like the
  prototype. Every car exists in both PC and GameCube with the same models,
  so PC structures can be checked against their GameCube twins exactly.
- Geometry and LOD headers have the GameCube layout; the magic is `0x0002`
  (GameCube `0x2003`) and the per-group draw data differs (OpenGL renderer).
  Texture headers are 0x10 bytes (GameCube: 0x20 slots).
- The interface packs hold the same car lists as GameCube (little-endian):
  `hwvx-x roster` now works on PC archives too (one car-select screen per
  pack on PC).
- No source code: the 33-car unlock limit is in `Hot Wheels.exe` and needs a
  binary patch.

## Running the PC game under Wine (Fedora)

Two packaging gaps had to be fixed before the retail PC game would start:
- **"requires DirectX 8.0"**: the game probes `D3D8.DLL` (it renders with
  OpenGL). Fedora's Wine ships the 32-bit DLL as `wine-d3d8.dll` but does
  not register the `wine-d3d8(x86-32)` alternative. Fix: copy
  `/usr/lib64/wine-wow64/wine/i386-windows/wine-d3d8.dll` to
  `~/.wine/drive_c/windows/syswow64/d3d8.dll` (or register the alternative).
- **"File: Voice_.cpp Line: 253"**: no Wine audio driver → install
  `wine-pulseaudio`.
- The PC copy has no `FMV_` folder; the intro movies are skipped with an
  "could not be opened" message each.

## P2. The 33-car limit in `Hot Wheels.exe` (`research/pc_exe.md`)

Only two places limit the car count: the bounds checks (`cmp eax,0x21`) in
the code-variable 0x64 (car unlocked) get (VA 0x42A680) and set
(VA 0x429FA0). Car select, names and AI car choice take their ranges from
data; the save-data struct has the GameCube layout with the same unused
padding.

`hwvx-x patch-exe "orig/pc/Hot Wheels.exe"` (35 bytes, in place, backup
written as `Hot Wheels.exe.orig`, `--revert` undoes it):
- get: ids 33–35 → 1 (unlocked); ids ≥ 36 still 0;
- set: ids 33–35 stored in the unused save padding (same as HWVX-SDL).
The new code sits in two 15-byte NOP alignment gaps after a `ret`; checked
that nothing in the exe jumps there (function map from HWVX-PC-Decomp +
disassembly), and it uses relative jumps only.

PC interface data: same structures as GameCube (little-endian). Thumbnails
for 33–35 are textures 0x3A–0x3C (interface 3) and 0x61–0x63 (interfaces
6/7); PC textures are DXT1 (format 4) / DXT5 (format 5) with mips.

## P3. PC formats (`research/pc_formats.md`) and the PC converter

PC vs GameCube car packs (same release, every car exists in both, so the
mapping was checked byte-for-byte on real data):
- **Models:** GameCube header/LOD/group layout, magic `0x0002`. Each group
  has a triangle count (+0x10) and a u16 triangle list (+0x1C,
  `glDrawElements`); each LOD has one shared vertex array (position stride
  16, UV 8, normal 16, colour 4). Everything 16-byte aligned. Shadow volumes
  as GameCube plus open edges (second normal 0), colour 0, no texture.
- **Textures:** packed 0x10-byte headers; DXT1 (format 4, opaque) or DXT5
  (format 5, any transparency), full mip chain to 1x1.
- **Sounds:** GameCube layout, little-endian, samples 16-byte aligned.
- **Everything else:** same structures, little-endian — the prototype's
  object graph is used as is (no byte swap).

`port-car` writes PC packs when `--into` is a PC archive. Checked on a car
present in all three builds (prototype `car_0` → PC vs retail PC `car_0`):
same groups, triangle counts and flags for every model; vertex counts differ
slightly where the retail exporter shifted UVs, as on GameCube.
`render-car` and `roster --cars` read PC packs too (DXT decode); PC
thumbnails are DXT5 with 8 levels, stored upside down like GameCube's.

### Colour correction found along the way

Comparing the same textures in all three builds: prototype ≈ PC, while
GameCube stores every channel squared (`gc ≈ c² / 255`, darker). The first
GameCube conversion used the prototype colours unchanged, so the new cars
were brighter than the retail GameCube cars. Fixed (`texture::gc_tone`), and
the GameCube files in `HWVX-SDL/orig/files/` rebuilt.

## PC status (2026-10-31)

Installed in `orig/pc/`: `HW_Cars.xpc` (37 packs), `HW_IPack.xpc` (names,
range, thumbnails), 14 worlds (results-screen names), patched
`Hot Wheels.exe`. Originals in `orig/pc-backup/` (17 files). Waiting for
in-game feedback.

## P4. Crash after landing a stunt (PC, Sweet 16 II in Joyride)

Reported from play. Wine: page fault at 0x45D7B9 in the PC sound-voice code
(it indexes a voice table with a Sound record's first byte and calls
DirectSound `SetCurrentPosition`/`SetVolume` on a stale buffer).
Cause: every prototype car has one Sound record of **type 2** (its
explosion/crash sound, HW_Cars sound 12 → release 3). The release only uses
type 0 (positional) and 1 (plain sample) — the same sound is type 1 in all
33 retail cars — and the GameCube code happened to tolerate 2.
Fix (`port-car`, both platforms): Sound type 2 → 1, and the category byte
(+1) remapped from prototype to release numbering (1/5/2/10 → 10/6/9/1).
One positional sound per car keeps category 3 (no release equivalent known).
PC and GameCube `HW_Cars` rebuilt and reinstalled.

## Status (2026-10-31, after play-testing)

Confirmed in play by the user:
- **PC** (`orig/pc`, Wine): the three cars select, race and do stunts with no
  crash after the Sound-type fix.
- **GameCube** (HWVX-SDL): car select, drag race results, Joyride and
  adventure missions (world 2) with the new cars — "all good".

Still open:
- AI opponents never pick the new cars (world car slots cap random AI
  vehicles at id 32, `maxVehicle` at +0x37).
- A real GameCube would need the unlock change patched into the DOL.
- One positional Sound per car keeps prototype category 3 (no release
  equivalent known); harmless in testing.
