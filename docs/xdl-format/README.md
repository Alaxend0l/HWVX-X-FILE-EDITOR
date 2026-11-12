# `.xdl`: a pack format for HWVX-SDL (design draft)

**Status: proposal.** Nothing reads or writes `.xdl` yet. This collects what a pack format made
*for* HWVX-SDL would look like, so the `.x*` compiler can be designed against it. It builds on
[`../port-format/README.md`](../port-format/README.md), which specifies the in-memory layout the
port already converts every release into and the rules that produce it. Open questions are
marked **Open**.

## 1. Why a format of its own

Every release's packs work today, but each load converts them: the GameCube's byte-swapper
(heuristic, the main source of wrong values), display lists to indexed geometry, texture
headers, sample byte order, music-bank headers, and per-texture CPU decoding to RGBA8 at first
draw. A pack written by the compiler in the port's own layout removes all of that:

| today, at load or first draw | `.xdl` |
|---|---|
| GameCube byte-swap, guessed per field | written in host order once; checked by the compiler |
| display lists → indexed triangles | indexed triangles stored |
| textures decoded to RGBA8 on the CPU, uploaded per use | GPU block formats, uploaded as they are |
| vertices rebuilt each draw (`gx_mesh.c`) | static vertex and index buffers, uploaded once per pack |
| menus patched at startup (`Port_Menu.c`) | patched screens stored |
| PC/GC differences branched on at every step | one form; the source release recorded |

The gains are load time, first-draw hitches, GPU memory (block-compressed textures), frame time
(no per-draw vertex building), robustness (nothing guessed at runtime), and one target for tools
and mods.

## 2. Ground rules

* **Generated on the player's machine, from their copy.** An `.xdl` derives from game data. It is
  a cache in the port's preference folder, rebuilt when the source or the compiler changes, and
  never distributed. Mods ship as changes or source assets (§8).
* **The game code still reads pack structs in place.** Game structs keep their layouts and their
  32-bit pointer slots, which hold handles (HWVX-SDL README, "pointers in 4-byte slots"). So an
  `.xdl` pack still contains a BrainCore object graph with a pointer list, relocated by adding the
  blob's handle. That keeps the loader trivial. What changes is what the graph points *to*, and
  that the graph is final.
* **Little-endian, host order** throughout, every field of every struct, the leftovers listed
  in port-format §2.1 included.
* **One file per source pack set**, mirroring the source names (`HW_Cars.xdl`,
  `worlds_/geo_/w1_geo.xdl`), so the port's per-file lookup (`dvd.c` `s_formats`) just gains an
  entry that is tried first, in the cache folder.

## 3. Container

```
XdlHeader                       0x40
  char  magic[4]   "XDL1"
  u32   version                 format version (bumped on any layout change)
  u32   compiler                converter version that wrote it
  u32   source                  PORT_DATA_GC / PC / PS2: whose data it came from
  u8    sourceHash[16]          of the source archive (rebuild when it changes)
  u32   numSections, sectionTableOffset
  u32   flags                   bit 0: sections may be compressed
  u32   reserved[...]
XdlSection[numSections]         0x20 each
  u32   kind                    TOC, PACK, GPU_VERTEX, GPU_INDEX, GPU_TEXTURE, AUDIO, STRINGS, ...
  u32   id                      the BrainCore pack type/id it belongs to (TOC order)
  u64   offset, size            in the file
  u32   rawSize                 when compressed
  u32   compression             0 none, 1 zstd   (Open: worth it? see §14)
  u32   align
```

* The **TOC** section keeps BrainCore's entries (type, id, version) so `DataPack_LoadFile` finds
  packs by type and id as it does now, plus each pack's section indices.
* A **PACK** section is one pack as the game sees it: header, relocation tables, blob, pointer
  list, already in final form (§4). The loader reads it into the game heap and relocates.
* **GPU_*** and **AUDIO** sections are bulk data that never needs to be in the game heap: uploaded
  or handed to the mixer straight from the file (or a mapping of it).
* Sections are aligned (64 bytes, or 4 KiB for memory mapping). **Open:** memory-map the file
  and point into it instead of reading, once the handle registry can hold mapped ranges.

**As built in step 1** (§5.1, HWVX-SDL `src/port/xdl.c`): the fields above do not fit 0x20, so a
section is `u32 kind, pack (TOC index); u64 offset; u32 size, rawSize; u16 compression, align;
u32 segment`. The header's `sourceHash` holds the source file's size and modification time (u64
each), not a content hash, and a `settings` word records conversion switches that change the
output (`HWVX_PS2CULL`). The TOC is the last section and the section table follows it, so a pack
written later appends its sections where the TOC was and rewrites both, the header last. The
TOC's entries are `{type, id, source offset, PACK section}`, keyed by the pack's offset in its
source file (a quadrant has no type/id of its own the loader knows).

## 4. Pack contents

### 4.1 Object graph

Final, host-order structs. Things the port does at load today become stored facts:

* texture ids already at a group's `+0x1A` (no `PORT_TEXRELOC_U16`);
* song banks in one form;
* interface screens as patched (§6), with the source release recorded for code that still needs
  to know.

The pointer list stays; texture, shader and sound relocations stay (they resolve against
*other* packs' ids at load).

### 4.2 Geometry

The indexed layout (port-format §2.2): level `+0x0` marked indexed, `+0xC` the vertex buffer,
groups' `+0x10`/`+0x1C` triangle count and index list, skinned levels' bone counts at `+0x10`.
Two changes for the GPU:

* **Vertex data lives in a GPU_VERTEX section**, one per pack. The level's vertex buffer holds
  `{base vertex, vertex count, attribute mask}` into it instead of pointers to arrays.
  **Index lists live in GPU_INDEX** (u16; offsets relative to the level's base vertex), and a
  group's `+0x1C` holds `{first index, count}`. The renderer uploads both once per pack and draws
  with `baseVertex`/`firstIndex`, no per-draw vertex building (today's `gx_mesh.c`).
* **One interleaved vertex format**, close to what the shaders read (`GXVertex`, `gx_internal.h`),
  but compact:

  | field | type | bytes |
  |---|---|---|
  | position | f32 ×3 | 12 |
  | normal | s8 ×4 (×1/64, the hardware's scale) | 4 |
  | colour | u8 RGBA | 4 |
  | uv | f32 ×2 | 8 |
  | bone | u8, + 3 pad | 4 |
  | **total** | | **32** |

  Attributes a group doesn't read stay in the record (zero/white) so every group of a level shares
  one stream. **Open:** half-float uvs (28 → 24 bytes) if no world needs the range.
* Which attributes a draw *uses* still follows the group's kind (port-format §2.2); that stays a
  property of the group, not the data.
* Shadow volumes stay as they are (positions and edges, read on the CPU).

**As built at runtime (step 2a, HWVX-SDL `src/port/gx_mesh.c`):** every source, not only `.xdl`,
now draws from static buffers built the first time a level is drawn: its groups become one
triangle list (the PC's strips too, so a group is one draw), and each vertex setup (the use
flags) gets a buffer of the 32-byte record above (bone as f32). The matrix slot of an unskinned
level is passed per draw, so one buffer serves every object; a skinned level's is in the record.
Buffers are dropped when the memory they were built from is freed or rewritten (the display-list
cache's `Port_InvalidateDisplayLists`). What it taught:

* **Shared vertices change pixels.** With each vertex stored once and indexed, the title demo
  differs from the per-draw build in a few thousand edge pixels per frame (max channel difference
  ~70) on Apple GPUs, both renderers, wherever passes meet at equal depth (a car's env-map pass
  over its body). The vertex data was checked identical (`HWVX_MESHVERIFY`); drawing the same
  buffers with the triangles' vertices in order (three per triangle, no index reuse) gives
  byte-identical frames. So the buffers hold vertices in draw order by default and are drawn
  without indices; `HWVX_MESHSHARED=1` keeps the indexed form. The cost is memory: at most 6.3 MB
  in the title demo's races (300-400 vertex buffers), against 2.7 MB shared (2.5 MB vertices,
  0.2 MB indices) for the same stretch of the GameCube demo.
* **What it saves.** Uploads drop from ~4.5 MB to ~0.15 MB of vertices per frame in the demo
  races; the CPU time drawing pack geometry from 1.5 to 1.0 ms/frame (OpenGL; what is left is
  per-draw state) and from 0.57 to 0.27 ms/frame (SDL_GPU on Metal).
* **Attributes per setup, not per level.** A vertex's values depend on the setup (white without
  colours, alpha 255 for the RGB setup, zero normals or uvs), so one record per level would need
  the shaders to select. Up to 8 buffers per level (4 setups, skinned or not); in practice 1-2.
* **So for `.xdl`:** GPU_VERTEX would store the expanded per-setup streams (or the shared
  stream plus the index list, if pixel identity across the paths stops mattering), and GPU_INDEX
  only for the indexed form.

### 4.3 Textures

* Stored as **BC1** (opaque, DXT1) or **BC3** (alpha, DXT5) with the full mip chain, the format
  the PC release already uses. GameCube CMPR re-tiles to BC1 without re-encoding, but BC1 draws
  most of it differently (below); RGBA8/RGB565 encode to BC3/BC1 (or stay raw RGBA8 where quality
  matters: fonts, UI, **Open**).
* Colour look decided at compile time: GameCube colours or the PC's (`pc = sqrt(255 · gc)`).
  Switching rebuilds the cache, or both are stored (**Open**: size vs. instant switching).
* The texture header in the graph points into GPU_TEXTURE: `{format, width, height, levels,
  offset}`. **Open:** devices without BC support (some mobile GPUs) would need ASTC/ETC2 or a
  decode fallback; the compiler can choose per target.

**As built at runtime (step 2b, HWVX-SDL `src/port/gx_tex.c`):** the renderers take BC1/BC3
(OpenGL through `EXT_texture_compression_s3tc`, with the PC's opaque DXT1; SDL_GPU where the
device samples `BC1_RGBA`/`BC3_RGBA`), and a texture goes up as stored where the GPU draws it as
the release's hardware did; everything else is decoded to RGBA8 at first draw, as before. What it
taught:

* **The GameCube doesn't draw CMPR as BC1 does.** Its middle colours lie 5/8 and 3/8 of the way
  between the endpoints (Dolphin's decoder, which follows the hardware), BC1's 2/3 and 1/3 (on
  Apple GPUs exactly `(43a + 21b + 32) / 64`, measured over every endpoint pair with a Metal
  probe). In a 3-colour block the GameCube's transparent texel keeps the average colour (rounded
  towards the first endpoint, as is its opaque middle colour); BC1's is black. The game draws with that colour: the green glow box behind the GO sign at a race start
  is those texels drawn additively, and the PC and PS2 copies have the box in their own pixels
  (the same texels leave faint speckles around the HUD's speedometer dial).
  The port's old decoder had neither (it also expanded 5-bit channels as `v * 255 / 31` instead
  of repeating the high bits), so the GameCube release lacked the box. The CPU decoder now
  decodes as the hardware does. A CMPR texture goes up as BC1 only if every colour its blocks use
  comes out the same both ways, which holds for 5 of the title demo's ~200 textures.
  `HWVX_TEX_CMPR=close` also takes the textures whose transparent texels are safe: 7.7 MB instead
  of 19.9 MB, with middle colours up to 12 levels off (in the demo's frames ±1 almost everywhere,
  at most 31 in 152 pixels of one frame).
* **Frames against the build before** (title demo, a minute, 60 frames, OpenGL and SDL_GPU alike):
  the PS2 release byte-identical; the GameCube's 49 of 59 frames differ, mostly by 1 (the
  5-bit expansion and 5/8 weights; 0.2% of pixels by 16 or more, the GO box and the dial up to
  255); the PC's 57 of 60 by at most 8 with its default GameCube colours, at most 6 with its own
  (the decoder's rounding, now the GPU's). The GameCube's default and its forced CPU decode
  (`HWVX_TEX_DECODE=1`) are byte-identical.
* **The PC's DXT is BC as it is,** when drawn with the PC's own colours (`pctextures=pc`). Frames
  are byte-identical whether the textures go up as BC1/BC3 or the CPU decodes them, on both
  renderers, once the decoder rounds as the GPU does (Apple's weights: colours as above, DXT5
  alpha in 64ths). Its default GameCube colours square each texel, which a block can't hold, so
  those are decoded, as are the PS2's raw RGBA8 textures. SDL_GPU has no opaque BC1, so a DXT1
  texture that uses the transparent index (the PC drew it opaque black) is decoded there.
* **Textures are shared by content.** A cache miss hashes every byte of the texture (64-bit) and
  reuses any GPU texture with the same content. Over a minute of the title demo (no frame dumps,
  so it gets into the races) that saves 21 textures (2.6 MB) on the GameCube, 16 (3.1 MB) on the
  PC and 23 (3.7 MB) on the PS2.
* **Memory and time,** resident texture memory at the end of that minute against RGBA8 with one
  texture per texture object (as before), and the time first draws spent making textures (hash,
  decode, upload; OpenGL / SDL_GPU on Metal):

  | release, setting | before | after | first draws, before → after |
  |---|---|---|---|
  | GameCube | 22.5 MB | 19.9 MB | 15.7 → 14.3 ms / 10.2 → 9.1 ms |
  | GameCube, `HWVX_TEX_CMPR=close` | 22.5 MB | 7.7 MB | → 10.4 ms / 7.4 ms |
  | PC, GameCube colours (default) | 23.5 MB | 20.4 MB | 27.3 → 23.7 ms / 20.7 → 18.1 ms |
  | PC, its own colours | 23.5 MB | 3.3 MB / 4.2 MB | → 5.8 ms / 4.6 ms |
  | PS2 | 25.1 MB | 21.4 MB | 29.8 → 26.6 ms / 23.1 → 20.6 ms |

  The time is ~0.1 ms a texture either way: decoding was never expensive. Pack loads don't
  touch textures and take as long as before.
* **So for `.xdl`, GPU_TEXTURE is not built yet.** What the GPU takes as stored is the source's
  bytes (DXT) or a byte shuffle of them (CMPR to BC1); everything else goes up as RGBA8, 4-8
  times the source's size, to save the ~0.1 ms a texture above. GPU_TEXTURE pays off once the
  compiler picks each texture's encoding and colour look ([`ultimate.md`](ultimate.md)), and can
  carry the content hash so loading needn't compute it.

### 4.4 Audio

* SFX samples: PCM16 host order in an AUDIO section; descriptors point into it.
* Music: songs (standard MIDI inside the RIFX wrapper today, **Open:** store plain little-endian
  headers) and instrument banks in one host-order form, samples in AUDIO.
* **Open:** compressed audio (ADPCM or Opus) for size; today's mixer reads PCM only.

### 4.5 Worlds and streaming

* Quadrant packs stay separate PACK/GPU sections per quadrant, so streaming keeps loading one
  quadrant at a time; the section table lets a quadrant's GPU data upload on its own.
* Collision: host-order structs as they are. **Open:** precomputed acceleration data.

### 4.6 Shared data across packs

Car packs repeat data: measured on the PC release's 36 car packs, **sound samples** total 2.5 MB of
which 1.7 MB (67%) also occur in another car, **texture pixels** 2.5 MB with 0.4 MB (17%, 16
textures) repeated, and the **object graph** (particles, stunt and AI setups, HUD; ~32 MB with
geometry) about 6% of its data, pointer values aside. (Stunt message texts are not in car packs.)
BrainCore already shares some of it on purpose, through the `share` pack cars reference.

Per-car copies have real benefits: a car pack loads, unloads and streams on its own; adding or
modding a car is one pack; any car can change its own particles or sounds; packs are not
read-only (the loader writes texture ids, some structs hold playback state), so data shared by
cars on screen together would need every written field audited; and a BrainCore pack references
at most two others.

So `.xdl` splits by kind:

* **Read-only bulk data is shared by content.** Sound samples and texture pixels go in a shared
  store (`shared.xdl`, sections keyed by a hash of their content); a pack's descriptors and
  texture headers reference them by hash. The compiler merges identical blocks across every
  pack, cars and worlds alike. The renderer and mixer also cache by content, so a texture used by
  several cars is uploaded once (today's texture cache is keyed by address and uploads it per
  car).
* **Object graphs stay per pack**: about 6% to gain, mutable at runtime, and what a modder edits
  per car.
* Sources (mods) stay one car per pack; sharing happens only in compiled output.

Measured since ([`components.md`](components.md)): the 6% above is over whole blobs, geometry
included, and car geometry never repeats. Without it, the 33 cars' object graphs hold 3.9 MB, of
which 1.7 MB are copies within the same pack, 2.0 MB also occur in another car and only 0.23 MB
are distinct; across worlds, interface scripts (pause menus, HUDs: 20 MB → 0.27 MB), effects
(4.5 MB → 0.22 MB), sound sequences and world props repeat most. That is what §13's components
are built from.

## 5. Loading in the port

* `dvd.c`: `s_formats` gains `PORT_DATA_XDL` (`.xdl`, the cache folder), tried first.
* `Port_Data.c`: the XDL steps do nothing; the graph is final.
* `Render_Geometry_.c`: an XDL level draws from the pack's uploaded buffers (base vertex, first
  index); the CPU path in `gx_mesh.c` remains for any data not in buffers. (Since step 2a every
  level does, built at first draw: §4.2.)
* `gx_tex.c`: XDL texture formats upload as block data; no decode. (Since step 2b every source's
  BC data uploads as stored where the GPU draws it as the release's hardware did, and textures
  are shared by content: §4.3.)
* Fallback: if a cache file is missing or stale, the port loads the source as today (and can
  compile it in the background).

### 5.1 Step 1, as built: the converted packs, cached

HWVX-SDL's first `.xdl` stores no new layout: it is each pack as the port's loader leaves it in
memory, read back with every conversion skipped (`HWVX_XDL_WRITE=1` fills it, `HWVX_XDL=1`
reads it; `src/port/xdl.c`). The title demo renders byte-identical frames from the cache and
from the source for the GameCube, PC and PS2 releases. What it taught:

* **The source stays open.** `dvd.c` doesn't open the cache *instead of* the source: the game
  still reads the source's TOC (`DataPack_LoadFile`) and its quadrant offsets (`FileStreaming`)
  through its own file layer. `DVDOpen` attaches the cache file, if current, to the open source
  (`DVDFileInfo.xdl`); `DataPack_LoadDataPack` and `DataPack_LoadQuadPack` look a pack up by its
  offset in the source and skip the reads. One cache file per source file, under
  `<pref>/xdl/<gc|pc|ps2>/`, so one release's cache is never read for another's data.
* **A pack is segments, not one blob.** Per pack: MAIN (tables, blob and relocations as read and
  converted), SOUND (the sound data as `CreateSFX`/`CreateMIDI` read it, PS2 samples decoded
  and pointers fixed), BANK and SFX (the PS2's rebuilt song bank and decoded samples: blocks
  freed again during the load, but the mixer reads them, so they are cached), up to 8 BLOCKs (the
  converters' geometry and PS2 texture blocks; each records whether `DataPack.soundData` holds it
  or `PortGeometry_Keep` does), and RAW_* (below). A quadrant is a pack of its own with MAIN and
  BLOCKs only.
* **Pointers: fixups, recorded.** A fixup is `{slot segment, slot offset, target segment}`; the
  stored slot holds the offset into the target, and the loader adds the target's handle. The
  slots are the pointer list's, the sound pointer table's, the local pointer relocations' (cross-
  pack ones stay relocations, resolved at load as before, the stored slot zeroed), and every slot
  a converter stores to or clears, which it reports (`Xdl_NoteSlot` in `PlaceLevel`, the PS2
  shadow levels, PS2 texture pixels and SFX descriptors). The pointer list alone is not enough:
  converters point list slots into new blocks, write pointers where the list has none (a skinned
  level's bone counts at 0x10) and clear list slots (PS2 shadow levels). A slot's target is the
  segment its value falls in; nothing is guessed from what memory holds.
* **Offsets are from the aligned read, not the allocation.** The loader aligns the blob to 64
  bytes inside its allocation, so where it lands depends on where the allocation does, which
  differs between loads of the same pack.
* **The heap is part of what the game can see.** The loader takes the same blocks, the same
  sizes, in the same order as the source load (including a PS2 bank or sample block freed again
  within the load), so later allocations land where they would have. `File_Read` borrows a heap
  buffer for small reads and leaves the file's bytes in it, and large reads write up to 31 bytes
  past the end: both are reproduced (RAW_* segments; segments rounded up to 32 bytes).
* **Loader-written slots inside the data.** A quadrant's header holds its pack refs, which the
  loader fills in only if they are empty: they are cached as read (`Xdl_KeepValue`), or a cached
  quadrant would point at the refs of the run that wrote the cache.
* Kept from the source path: sound ids, song loading, the order cross-pack refs load in, the
  display-list invalidation the byte-swapper does. Skipped: `PortData_*` conversions,
  `UpdatePointers`, `PortGeometry_Convert*`, `PortPS2_*`.

### 5.2 File safety (after the code review)

Appending a pack: its sections, the new TOC and the new section table are
written after the end of the file, synced (fsync), then the header pointing
at the new table, synced. The old TOC and table stay behind unused (a few KB
per file: 0.1% of a full GameCube conversion). Until the header is written
the file reads as before, so a write cut short (killed, full disk) loses
only that pack.

A cache file is not read (the source is loaded, as for an out-of-date file)
when its section table or any section runs past the end of the file. A pack
is damaged (loaded from its source, the file's other packs still read) when
a fixup's slot holds an offset past the end of its target segment, or its
song bank reference names no segment or lies past the end of it; a pack's
segments are read whole when it is found, before anything is placed.

A pack written this session is added to the open file's index: later loads
read it from the cache (reading) or skip recording it. Writing only, the
file's index is read once when it is opened.

## 6. Menus

Interface screens are data, and the port's additions (Graphics, Quit, Multi-Player, card-screen
changes) are runtime patches today (`Port_Menu.c`). In `.xdl`:

* a **menu dump** turns a release's interface packs into a release-neutral description (screens,
  objects, records, events, conditions, variables, text, animations, references by id) and back,
  byte-exact on a round trip;
* the port's changes become a **declarative patch** applied to that description, matching rows
  by text or role, so one patch works on every release;
* the compiler stores the patched screens; an option keeps a release's original main menu.

The base screens still come from the player's copy: the port cannot ship one release's menus to
the owner of another. A player who owns both can already pick one release's `HW_IPack`, since
the port mixes releases file by file.

## 7. The compiler

**Where it runs: in the port.** HWVX-SDL already turns every release into the port's layout as
packs load (byte order, `Port_Geometry.c`, `Port_PS2.c`, `Port_Data.c`, the menu patches), and
those conversions are the ones its frame tests check. A compiler that reimplements them in the
editor would be a second copy to keep equal. So the compiler is the port's loader run headless:
load each pack exactly as the game would, then write what is in memory (the relocated graph back
to offsets through the pointer list, the blocks the converters allocated as sections). The
launcher's "Convert to .xdl" runs it on the player's imports; `hwvx-x compile` in the editor can
call the same code as a library. The steps below are what that load already does:

1. Read each archive (any release) with the editor's existing readers.
2. Byte order: GameCube data through a checked swapper. The PC's little-endian copy of the same
   structs is a test oracle for field types.
3. Geometry: port-format §2.3 (`geom_check.py` is the reference), then pack vertex records and
   indices into GPU sections.
4. Textures: decode, colour look, encode BC1/BC3 with mips (the editor's `textures` code and
   `gc2pc_tex.py` are references).
5. Audio, music: host order, one bank form.
6. Menus: dump, patch, store.
7. Write the header (source, hashes, versions), sections and TOC.

Checks: every pack loads in HWVX-SDL with `HWVX_DETERMINISTIC=1` and the title demo renders
byte-identical frames to loading the source (this works today for source data, see
port-format §5).

## 8. Mods without game data

A mod must never contain the game's data: redistributing it, even inside a mod, raises legal
issues. The format and tools make that the default and check it, instead of leaving it to
modders. (A technical design, not legal advice.)

* **Reference, don't copy.** A mod names the base data it builds on by a stable asset id
  (archive, pack type and id, object kind, index: `HW_Cars/car_5/texture/3`) plus a hash of the
  original bytes. The compiler takes the data from the player's own copy when it builds the cache;
  the hash confirms the copy is the one the mod expects and reports a different release (GC vs.
  PC content) clearly instead of building something wrong.
* **Changes as operations.**
  * Tweaks to existing objects (handling values, particle colours, timings, menu rows) are
    field-level patches on the dumped object graph, carrying only the new values.
  * Edits to game art are recipes the compiler applies ("hue-shift texture T", "overlay
    `decal.png` on texture T"), not finished pixels: an edited game texture is still derived from
    it.
  * New content (a modder's own models, textures, sounds) ships as ordinary sources: glTF, PNG,
    WAV, JSON.
* **A packaging check.** `hwvx-x mod pack` indexes every block of base data (texture pixels,
  samples, geometry arrays, strings, object records, by content hash) from the modder's own
  install and refuses to package a mod containing any of it, naming the asset to reference
  instead. That catches the common accident: a folder from `hwvx-x extract` copied into a mod.
* **Stable ids across releases.** Ids follow the BrainCore structure (pack type/id, object
  index), which the releases share, so one mod works on whichever release a player owns, and
  release-only assets are reported as such.
* **Across releases** (details in §8.1).
* **Built on the player's machine.** The compiler merges base data, patches, recipes and new
  sources into the `.xdl` cache, like the base packs (§2). The cache is never shared.
* A mods folder is checked before the cache and the source. Room past retail limits: more cars
  than 33 (`pc_exe.md`), bigger TOC caches, widescreen UI.

### 8.1 References across releases

The releases come from one BrainCore project, so their *structure* matches where their bytes
don't (PC vs. GameCube, from [`../beta-cars/research/pc_formats.md`](../beta-cars/research/pc_formats.md)
and [`../pc-version/README.md`](../pc-version/README.md)):

| | GameCube vs. PC |
|---|---|
| archives, pack types and ids, TOC order | the same |
| object order in a pack | the same: geometries index-paired (car bounding boxes equal 585/585), texture and sound relocations the same indices, object graph 93.7% equal after byte order |
| sound samples | equal sample for sample (byte order aside) |
| geometry | the same triangles in the same order (6883/6936 groups; the rest art edits) |
| textures | encoded separately (PC DXT from the source art, GameCube CMPR, its colours squared): never byte-equal |
| release-only content | interface packs (wording, keyboard screen, one car-select header vs. two), some geometry (car_29), ~84 HUD words per car |

So a reference is:

1. **A structural id**, the same on every release: archive, pack type and id, object kind,
   ordinal (`HW_Cars/car_5/geometry/12/group/3`, `.../texture/3`, `.../sound/7`).
2. **A normalised fingerprint** where content is equal across releases, computed after
   conversion to the port's layout: samples in host order; geometry as its triangle list
   (positions, uvs) in the indexed layout; object records as host-order fields. These match
   between releases, so a mod made on one verifies on the other.
3. **Per-release fingerprints** where encodings differ (textures): the mod lists one per release
   it knows. The table of `id → {release: hash}` is generated by pairing the releases' packs and
   holds only ids and hashes, so it can be published: [`pairing.md`](pairing.md) (id grammar,
   fingerprints, coverage) and [`pairing/`](pairing/) (69,734 assets; `scripts/pairing.py`
   regenerates it from a player's own copies).
4. **Encoding-independent operations**: recipes act on decoded images and sample streams, field
   patches name struct fields (the pack structs are identical across releases), not offsets.
5. **Release-only assets are explicit**: an id missing from the player's release is reported;
   a mod can give a fallback or declare the releases it needs.
6. **Mixed installs** resolve per file, as the loader does: each file's release picks the
   fingerprint to check.

PS2 (retail) likely shares the structure; it needs the same pairing once its formats are known.
The PS2 prototype differs more (`docs/beta-*`).

### 8.2 As built in HWVX-SDL (step 4e)

**Packing** (`tools/xdl_mod.py`, the manifest is `schemas/Mod.schema.json`):

| command | does |
|---|---|
| `index DATA_DIR... -o INDEX.npz` | hashes the modder's own install; made locally, never shipped |
| `check MOD_DIR --index INDEX.npz` | what `pack` would refuse |
| `pack MOD_DIR -o OUT.hwmod --index INDEX.npz` | refuses anything that matches the base data; writes a zip of stored entries with `check.json` |
| `install OUT.hwmod MODS_DIR` | unpacks a package (the same as `hwvx --install-mod OUT.hwmod`) |

`pack` refuses when a mod holds any of these:

* Whole files of the install.
* Raw 32-byte runs, including hex, base64 or data-URI strings inside JSON.
* Decoded sound PCM.
* A texture equal to a base one, or one that looks like a base one: an edited texture is still the
  game's, so it is made with a recipe instead.
* Mesh positions equal to a base mesh's.
* A patch that restates the base value. `--strip-unchanged` drops those instead of refusing.

**Loading** (`src/game/Port_Mods.c`, its API in `Port_Mods.h`):

* **Where:** mods are folders in `mods/` in the preferences folder, or in `HWVX_MODS_DIR`.
* **Order:** `mods.jsonc` (`order`, `disabled`), then the remaining mods by id, then each mod's
  `after`.
* **Entries:** an enabled mod's `cars` and `items` go into `ids.lock` with `alias`, `name`, `mod`
  and `unlocked`. A mod that is off or gone leaves its entries `"active": false`, so their numbers
  and saves wait for it.
* **Patches:** applied as a pack's pointers are fixed up (`FileStreaming_InitializeDataPack`). The
  fields come from the schemas (`Port_ModFields.inc`, from `tools/gen_mod_fields.py`). A mod
  entry's own patches apply to its own number's pack only.
* **Recipes:** colour matrices on the decoded texture (`gx_texmod.c`), composed the same way as
  `recipe_matrix`.
* **Report:** two mods setting the same field, recolouring the same texture or bringing the same
  entry are reported, and the later one applies. A mod put in by hand, with no `check.json`, is
  reported as unchecked.
* **Switches:** `HWVX_MODS=0` turns every mod off. `HWVX_MODS_LOG=1` logs each value a patch sets
  together with its old value.

**Tested:** the sample `tools/mod-samples/night-shift` on GameCube data.

* It packs with no game data found.
* It installs; a damaged package and one without `check.json` are refused.
* Its car becomes number 33, and in a race five fields are set: `maxRpm` 7400 → 8200,
  `turboTorqueScale` 1.15 → 1.3, `steerRate` 6 → 7.5, `damageScale` 0.05 → 0.03, `brakeDecel` 1 → 1.25.
* Car 7's patch and both recipes apply.
* Without mods, the attract demo is frame-identical.

**Not done yet:**

* The launcher's Mods page is still a placeholder; it has a hook, `Port_LauncherMods`.
* Online play doesn't check `Port_ModsActive` yet.
* Items' patches assume the item's pack number (`Port_ContentItemPack`); only cars were tested.
* Tracks (`"tracks"`: `extends` base://mcp/track/N, `name`, `thumbnail`, `unlocked`, `world`) go into ids.lock like cars; the sample's shows in the Challenge select, unlocked.

#### 8.2.1 After the code review: hardening

For docs/xdl-format/README.md §8 (mods) and §8.2 (Tested).

**Patches.** Patches apply only to car packs (type 0, root struct `CarAInfo`) and item packs
(type 2, root `ItemRecord`), the same `ROOT_KIND` as tools/xdl_mod.py. Every object a patch reads
or writes must lie inside the pack's data blob, along with the root table and each link target.
An index in an object path is allowed only into an array whose length the schema gives (a `num*`
or `count*` field, or a `[numX]` in the link's description), and it is checked against that
count. Anywhere else the only index allowed is 0. Fields that count an array's entries can't be
patched (the port treats them as `MF_OTHER`, and the packer refuses them with "counts an array's
entries"). A value outside its field type's range is clamped and reported. Report lines from
loading packs appear only once.

**Load order.** `after` is applied as a stable topological sort, so mods.jsonc's order is kept
wherever no `after` says otherwise. Mods in a cycle keep their list order and are reported.
When two mods bring the same entry, the later mod's entry is written to ids.lock once, so the
file is no longer rewritten on every start. `HWVX_MODS_LOG=1` also logs the load order.

**Packages (check.json, tool_version 2).** `files` is `{name: {"size": n, "sha256": hex}}`.
`Port_ModsInstall` and `xdl_mod.py install` both refuse a package when:
- a file has a kind the check doesn't read, or a name with a hidden part;
- an entry is compressed, or a name appears twice;
- check.json isn't from tools/xdl_mod.py with format_version 1 and tool_version 2 or later;
- check.json names a different mod, or its result isn't "no game data found";
- the files it lists aren't exactly the package's files, with matching sizes and sha256.

Older packages have to be re-packed. A mod folder counts as "checked" only when its check.json
passes the same test against the files on disk.

**Install.** The package is unpacked to `<id>.part`. If an old copy exists, it is renamed to
`<id>.part-old`, the new copy is renamed into place, and then the old copy is deleted; if
something fails, the old copy is put back. A folder named `<id>` that holds some other mod (or
no mod) blocks the install. Once the new copy is in place, a copy of the same mod in any other
folder is removed. Removing a folder never follows links: a working folder symlinked into mods/
loses only the link.

**Packer.** Hex is now decoded from .txt and .md files as well as JSON strings, including hex
with `0x` prefixes or separators (whitespace, commas, semicolons, colons) and 2- or 4-byte words
(also read little-endian). Base64 runs are decoded the same way, across line breaks. Arrays of
numbers in JSON, nested arrays flattened, are checked as u8 (and s8), f32 LE and f32 BE. When a
mod's `releases` names a release the index has no data for, it is refused unless
`--allow-partial-index` is passed; that case is recorded in check.json as `unindexed_releases`.

**Tested:** tools/test_xdl_mod.py. With HWVX_BIN (default build/hwvx), it also runs the port's
`--install-mod` against crafted packages and symlinked folders. tools/mod_race_test.sh installs
a package and runs a race with HWVX_MODS_LOG=1, which shows the sample's 5 car values.

## 9. Mixed builds: one game from several releases

A player who owns more than one release can build a cache that takes each asset from the release
that has the best version of it (the "ultimate" build). Which release wins for what is measured,
not assumed: [`ultimate.md`](ultimate.md) holds the rule table and the numbers behind it.

* **A base release per pack.** Each pack's object graph (structs, pointer list, ids) comes from
  one release, its *base*; the graph is what the game code reads, and releases differ there in
  small ways (GameCube vs. PC 93.7% equal after byte order; PS2 adds textures to some tables).
  The base is chosen per pack by the rule table; by default the release whose interface pack is
  in use, so menus and the code variables that depend on the release (0x258) agree.
* **Assets substituted by pairing.** Leaf data (texture pixels, sound samples, movie files) may
  come from another release when the pairing tables (§8.1: structural id plus content pairing,
  as `ps2_tex.py --check` / `ps2_audio.py --check` do) match it to the base's asset:
  * a texture keeps the base's header slot and texture id; its pixels, size, levels and format
    come from the chosen release (the uvs are normalised, so a larger texture drops in);
  * a sound keeps the base's descriptor; samples, rate and loop come from the chosen release (a
    PS2 base takes the PC's PCM samples with the PC's rate and loop, not its own resampled ones);
  * unpaired assets stay the base's: nothing is guessed.
* **Content is the union.** A pack or file only one release has (the PC's two extra movies, the
  multi-player drag worlds the PC lacks, PS2-only sounds) is taken whole from that release, with
  its own base.
* **Provenance is recorded.** The header's `source` becomes `MIXED`; a MANIFEST section lists, per
  asset, the release and fingerprint it came from. That keeps mod references (§8.1) checkable and
  makes a wrong pick traceable.
* **Every source is the player's.** A mixed build reads only releases the player imported; one
  they don't own is simply not a candidate. Nothing changes about distribution (§2).
* **Checks.** A build whose rules all pick the base must equal the single-release build byte for
  byte (and so render the same frames); a substituted asset must decode and match its pair within
  the pairing threshold.

### 9.1 The PS2 prototype's content

The prototype (`orig/ps2-beta`, research in `../beta-cars`, `../beta-worlds`, `../beta-audio`) is
a release too, with three import modes the player picks (the launcher's Games page, on the
prototype's row), each a rule set over the same pairing table ([`pairing.md`](pairing.md)):

| mode | effect | ids, saves |
|---|---|---|
| **beta cars only** | the prototype-only cars are added as extra cars | new ids for them (`ids.lock`); retail untouched |
| **beta content added** | all prototype content (cars, worlds, audio) is added beside the retail game; where both have a version, the prototype's appears as its own entry ("… (prototype)") | new ids for everything added; saves unaffected |
| **beta replaces retail** | where the pairing table matches a prototype asset to a retail one, the prototype's is used; prototype-only content is added as above | retail ids keep their meaning, so saves keep working |

* Only what pairs can be replaced: the prototype pairs with retail for 33–43% of its textures,
  sounds and geometry, the rest being prototype-only or reworked. The preview says what is
  replaced, added and left as retail.
* Extra cars past the retail 33 need the port's fixed tables raised (as `../beta-cars/research/pc_exe.md`
  did for the PC exe); the id-keyed saves (§11) already hold them.

#### 9.1.1 As built in HWVX-SDL: prototype import

Branch `protoimport` of HWVX-SDL. Goes under §9.1 as "#### 9.1.1 As built in HWVX-SDL"; the
ids.lock part also belongs in §10.5.1 (the `data` member) and §8.2 (entries with `"source":
"prototype"` sit beside the mods' entries).

**Modes.** The launcher's Games page has the modes on the prototype's row, once it is imported:
Off, Beta cars, Beta added, Beta replaces. The choice is saved in sources.cfg as
`proto=off|cars|added|replace`, and a line under the buttons says what the mode replaces, adds and
leaves as retail. `HWVX_PROTO` overrides the choice, and `HWVX_PROTO_DIR` overrides the
prototype's folder (by default it comes from `ps2beta=`).

| mode | cars | numbers |
|---|---|---|
| **cars** | Fandango, Sweet 16 II and Jester are added | 33, 34, 35 in a fresh ids.lock |
| **added** | all 36 are added: those 3, and the 33 others as "<name> (prototype)" | 33–68 |
| **replace** | the release's 33 cars load the prototype's versions under their own numbers, so saves keep their meaning; the 3 are added | 33, 34, 35 |

**Loading the prototype's packs** (`src/port/proto.c`, `dvd.c`, `DataGamePackLoad.c`, `Port_PS2.c`):

* A pack file whose name starts with `~` belongs to the prototype. `dvd.c` opens it from the
  prototype's folder under the PS2's file names, as `PORT_DATA_PS2`, with `DVDFileInfo.proto` set.
  Its packs are not cached in `.xdl`.
* The prototype's archives are version 177/179, and each TOC entry's offset counts from the end of
  that entry. Its packs have no sector padding and no 32-byte padding. Their `ref_relocs` and
  `pointers` come before the data, so they are read in file order into the places the loader keeps
  them.
* Texture headers: a level value m means m − 1 stored levels, and there is a format 0 (RGB24).
  Everything else is the PS2 release's (`PortPS2_Textures(..., proto)`).
* A prototype pack's references (`HW_share`, `HW_Cars`#2) get the prefix too. Its imports then
  resolve against the prototype's own shared packs, not the release's, so no index remapping is
  needed. When the mode is not off, the share pool grows from 3 to 5 slots.
* Fixed for every release: a pack that loads the packs it refers to while it is loading overwrote
  the static pack header. That cut the loop over the pack's relocations short and left the rest
  unrelocated. The release never got there, because its shared packs are loaded ahead.

**The cars** (`src/game/Port_Proto.c`):

* Of the prototype's 36 car packs, 33 are the release's cars under other numbers. They are paired
  by their skins' texture names, and every release car has exactly one: prototype car 7 is car 1
  (At-A-Tude), 28 is 25 (the bus), and so on (`s_cars`). Cars 1, 12 and 17 exist only in the
  prototype.
* Entries go into ids.lock as `uid://ps2proto-car-N`, with these members:
  * `"source": "prototype"`;
  * `"data": "proto://HW_Cars/car_N"`, the pack the car is loaded from;
  * `"alias"`: its twin, or car 0 for the three. The alias stands in online and in a game without
    the prototype;
  * `name` / `suffix`;
  * `thumbnail`: `proto://HW_IPack/interface_3/texture/<19+N>`, the prototype's own car-select
    pictures;
  * `"unlocked": true`.
* A mode that leaves an entry out sets it `"active": false`, so its number stays reserved for its
  saves, as with mods. New entries are numbered in a fixed order: the three first.
* `Port_ContentPackSource` (registry) gives the archive name and TOC id for a car: the
  `proto://` data link, or the twin in mode replace. `Port_ContentFixScripts` renumbers the
  scripts as it does for aliases.
* Once loaded, a prototype car gets the same changes `hwvx-x port-car` makes:
  * camera mode 1, which the release doesn't have, becomes the default chase camera (0) with a
    release car's values;
  * damage is set to 20 hits / 75%;
  * the turntable height (viewOffset y) is set to 2;
  * sound records of type 2 become type 1.
* The car keeps its own HUD template, which includes the prototype's x/y/z debug readout. It keeps
  its own stat bars and AI scripts.
* The pictures come from the prototype's `HW_IPack` interface_3, without its sounds. That pack is
  loaded outside the streaming pools the first time a select asks for one, and stays loaded for the
  run.

**Tested** on GameCube data with the prototype from sources.cfg:

* Car select in each mode: names, pictures and the turntable model. In mode added, "At-A-Tude
  (prototype)™" shows next to the release's.
* Joyride and the ch1_l2 challenge raced with Fandango, Sweet 16 II and Jester, and with car 1 in
  mode replace (`tools/proto_race.sh`).
* ids.lock through cars → added → off → cars: every car kept its number.
* The attract demo is frame-identical to pc-data with no mode set.
* ctest passes. `test_content` now expects a suffix before a trailing `*`.

**Not done (large):**

* **Worlds and tracks.** The registry side is ready: a registered track with its own `world`
  (§10.5.1). The prototype's level archives can't be loaded as they are:
  * its world packs have another layout (quadrant records are 0x40 bytes against the release's
    0x50, `docs/beta-worlds`; the rest of the world graph has not been compared);
  * its quadrants are packs inside each level's archive, where the release keeps them in
    `w*_geo`, `w*_tex` and `w*_col`; the prototype has no such archives.
  * Two ways forward:
    * (a) a v179→v183 world-pack converter, plus quadrant lookup in the level's own archive in
      `FileStreaming_LoadQuadPacks`;
    * (b) the `hwvx-x port-world` approach: the release's world pack as the host, with the
      prototype's quadrants, world textures and collision. This needs a release level with the
      same quadrant slots (only ch1_l2 has been done).
  * Either way it then becomes a registered track whose `world` is `~worlds_\sp_\...`.
* **Audio.** The briefing dialogue (`AUDIO_/HWB_M01–14`: 174 sounds against the release's 233) can
  only replace sounds one by one, through the pairing table. The music (`HW_MUSAK`, `HW_IT_PC`)
  is IMPM (Impulse Tracker) modules, and the port has no player for them.
* **Mode replace beyond cars** (textures and sounds of worlds and menus through the pairing table,
  as a mixed build does). Port_Mix's per-release arrays are `PORT_DATA_COUNT` (3) long, so the
  prototype has to become a fourth source there:
  * `Port_MixRoot`;
  * `RelPack` reading the prototype's layout (TOC offsets, section order, its texture headers);
  * a `proto` column from `tools/gen_mix_pairs.py`;
  * a recipe rule.
  * Cars are better done whole, as here. The pairing table pairs packs by id, and in `HW_Cars` the
    same id is a different car in the prototype.

**Doubtful:**

* Prototype cars look dark in races (they are lit correctly on the turntable). Not investigated:
  it may be its geometry's vertex colours or env map.
* Mode replace keeps the release's car-select pictures for the replaced cars.

#### 9.1.2 Prototype worlds, music and replace mode

Branch `protoworlds` of HWVX-SDL. Belongs in docs/xdl-format/README.md §9.1.1 ("As built in
HWVX-SDL: prototype import"), replacing its "Not done" and "Doubtful" lists; the tools paragraph
also fits §10.7.

## Reading the prototype's code

The prototype's executable (`orig/ps2-beta/SLUS_001.00`) is not stripped: its `.symtab` names
2,081 functions. `tools/mipsdis.py ELF FUNC...` disassembles functions by name (calls, globals
and strings named from the symbol table, `lui`/`addiu` pairs folded; `--find`, `--addr`), with no
toolchain: macOS's objdump has no MIPS target. Everything below was read from it, not inferred
from the data alone. No prototype function was decompiled into C.

## Cars in races

* **Dark cars** had two causes:
  * **Texture slots.** A race with a prototype car loads the prototype's own `HW_share` (82
    textures) and `HW_Cars` shared pack (20) on top of the release's. That ran past the game's
    0x1FF texture slots: `Gfx_LoadTexture_RAM` gives every texture after that the last slot, so
    the car's body got some other texture. With the prototype's content on, the table has
    `PORT_MAX_TEXTURES` (0x7FF) slots, and the tables indexed by a texture id are sized for it
    (`Gfx_DisplayList.c`, `Render_Geometry_.c`, `Render_QuadTri_.c`, `Render_Texture_.c`). The
    release keeps 0x1FF, so the attract demo is unchanged.
  * **The shadow hull.** `CarAInfo` 0x54 is the release's shadow volume (groups flagged 0x4000,
    which `Render_DrawShadows` draws as stencil volumes and `RenderGroup` skips). The prototype's
    is a closed hull stored as a plain lit geometry (`docs/beta-cars/research/geometry.md` §1.7).
    `CarA_Draw` draws it in a race, so it showed as a black shell over the car. It is left out:
    prototype cars have no shadow volume.
* **The x/y/z readout** is the prototype's own: a role-19 text object in each car's HUD reading
  `X: 00.0 |Y: 00.0 |Z: 00.0`, which the prototype fills with the car's position (it shows in the
  prototype running on PCSX2). The release never sets role 19's text. It is blank by default;
  `HWVX_PROTO_DEBUGHUD=1` (or `proto_debughud=1` in sources.cfg) keeps it and fills it in, as the
  prototype does (`Port_ProtoHudInfo`).

## Worlds as tracks (mode "added")

All 48 single-player and battle levels of the prototype become extra tracks: ids.lock
`uid://ps2proto-track-<level>`, `"source": "prototype"`, `"alias"` the release track of the same
level, `"world"` the prototype's archive (`~worlds_\sp_\joyride_\world1`), `"suffix": " (prototype)"`.
Each is a level of its twin's progression (missions, challenges, battles, drag races,
Joyride), so the world selects list "Monument City (prototype)" and so on next to the release's.
The level keeps the twin's progression entry (briefing, win and lose sequences, timer); the
world itself, with its scripts and objectives, is the prototype's. The prototype's 5
multi-player drag worlds (`race*l1`) have no track number in the release and are left out. In
modes other than "added" the entries stay in ids.lock as `"active": false`.

A prototype world pack has the release's `World` (the same offsets up to `gameData`, 0x9C), but
three of its record types differ (`Port_ProtoWorldLoaded` copies them into the release's layout
as the world pack loads, before anything reads it):

| record | prototype | release | how found |
|---|---|---|---|
| quadrant (`WorldQuad`, world 0x24) | 0x40 bytes | 0x50 | `DataPack_LoadQuadPack`, `LoadQuadrants` |
| nav point (`NavData.nodes`, world 0x78) | 0x70 bytes | 0x60 | `NavHolder_GetNavPoint` (7 × 16), `NavHolder_IsNavLinkNearer`, `_CreatePath`, `_ClearTempValues` |
| moving path (`TilePath`, world 0x14) | 0x14 bytes | 0x20 | `World_GetTilePath`, `StateGamePlay_Init` |

The other world arrays have the release's strides (`World_GetVolume` 0x30, `_GetTileLight` 0x20,
`_GetTileSpeaker` 0x40, `_GetTileCar` 0x60).

* **Quadrants.** The first 0x38 bytes are the release's (id, geometry count, geometries, region,
  neighbours, pack header and data, contents, loaded). The release adds `packId`, `fileOffset`
  and `size`, filled from the TOC. The prototype finds a quadrant's pack each time it loads it,
  by the TOC entry whose id is the record's id, with no geometry count in its high half, in the
  level's own archive (`LoadQuadrants`: `"XPS\<language>\" + world name + ".xps"`). So `packId` =
  the record's id, and `LoadQuadrants` takes a prototype archive: version 177/179, offsets from
  the end of the entry, ids without the count. The quadrant packs themselves are the release's
  layout and load as PS2 packs.
* **Pack names.** The world names its quadrant, texture and collision packs (0x7C, 0x84, 0x8C)
  with the release's folders; they all live in the level's own archive. They get the `~` prefix,
  so they're opened from the prototype.
* **Nav points.** Position, link costs (0x10) and links (0x20), four each and inline; the links
  again with the way forward first (0x30); radius (0x40); index (0x44); two floats (0x48, 0x4C,
  not identified); the chance byte (0x51); A* state (0x54 parent index, 0x58 cost, 0x5C parent);
  0x60 a blocked flag; 0x62 type; 0x64 next way point. The links run both ways. The release keeps
  the links A* follows (`prev`, with costs) apart from the ways on (`next`). Converted: `prev` and
  costs from 0x10/0x20; `next` from 0x30, without its last link when it has more than one (the way
  back); speed 0 (no limit); enabled.
* **Moving paths.** Anim, speed, record and thing, as the release's, without `enabled` (the
  prototype spawns every path) and draw distance.

**Tested:** `tools/proto_worlds_check.sh` starts each of the 48 as a race under lldb: all load and
run with no crash. Races were looked at on Joyride `world1` (it matches the prototype on PCSX2:
the same billboards, chevron sign, palm trees and night sky), `ch1_l2` (checkpoints and the
objective work) and drag `race1l2` (the opponents drive).

**Doubtful:** the item packs a prototype world spawns (pickups, by pack id) come from the
release's `HW_Items`, whose ids may mean other items. The prototype's pause screen (its mission
info toggle) is not used: the release's interface runs. The nav direction rule is a guess: AI
cars may turn back at junctions.

## Music

The prototype's songs are Impulse Tracker modules (`IMPM`). Its IOP module (`AUDIO.IRX`) plays
them; `CreateMusic` hands the song and bank over by RPC. The release's game code has an IT player
too (`IT.c`, matched from the GameCube decomp, ARAM samples), and `CreateMusic` picks it for
`IMPM`. The modules are standard IT headers (`HW E3 1`: 96 orders, 17 instruments and samples, 79
patterns). Only the samples differ: each sample header says 16-bit and compressed (flags 0x0B),
its data is SPU-ADPCM in the song bank at the header's offset, and loops are in the ADPCM blocks'
flags (bit 1 on every block of a looping sample, bit 2 at the loop start), not in the header.
`PortPS2_ProtoIT` decodes them into a bank of its own and rewrites the headers (PCM byte offset,
16-bit, uncompressed, loop). The prototype's worlds and `HW_MUSAK` then play their music with
the release's player.

## Mode "replace" beyond cars

The prototype is a fourth source in `Port_Mix.c` (`PORT_MIX_PROTO`), used by mode "replace" when no
mixed-build recipe is loaded (`Port_MixProtoReplace`). Each pack loads as its release has it.
Every texture and sound the pairing table pairs with the prototype comes from the prototype's
copy, read from its file in its own layout:
* TOC offsets from the end of each entry;
* no sector padding;
* sound pointers and data, the relocation tables and the pointer tables, then the blob;
* texture level count one too many, RGB24 textures.

The copy is checked as any other: thumbnail distance under 24, sound duration within 15%.
Prototype sounds are SPU-ADPCM and are decoded to PCM16 (`PortPS2_DecodeSound`). This is also the
dialogue swap: `HWB_M01`–`14` sounds that pair are replaced (58 of 233; the rest are different
recordings and stay the release's). Not heard yet: the release loads those packs only from its
briefing screens.
`tools/gen_mix_pairs.py` writes the prototype's column (`g_portMixPairs[][PORT_MIX_RELS]`).
Prototype packs (`~` names) are never mixed. The cars are still replaced whole (their pack ids
mean other cars in the prototype).

## The .xdl cache

Prototype packs are cached like the release's (`HWVX_XDL`, `HWVX_XDL_WRITE`, the launcher's
converted cache), in the cache folder `ps2proto` (`Xdl_OpenFileIn`). The release's packs mixed
by mode "replace" are cached in `gc+proto`, `pc+proto` and `ps2+proto`, so they never mix with
the plain release's. The fix-ups made after a load (cars, world records, quadrants) run on cached
loads too. They work from the pack as loaded and are idempotent.

## The Ripper Wheels

In the prototype a new game unlocks items 0–5 only (`GameSetting_Init`, 0x260AD4: Jet Boosters,
Energy Shield, Armor Repair, Magnet Mine, Super Zapper, Doom Disks). Ripper Wheels is item 6, so
it starts locked, as do the five after it, until a level unlocks it. Its pack (`HW_ITEMS` item 7,
whose ids start at 1) and its name and picture in `HW_IPACK` are there. This is the prototype's
data, not a menu bug, so the port leaves it as is. The port's item select is the release's in
every mode.

## Tools

* `tools/mipsdis.py`: disassembles the prototype's executable.
* `tools/proto_pack.py`: one of its archives, TOC and pack header; `--song` writes a pack's song
  and bank into a build folder.
* `tools/crash_bt.sh`: one run under lldb, the backtrace. It kills only the processes it started
  (lldb, debugserver, the game) by PID.
* `tools/proto_world.sh`: a race on a prototype world.
* `tools/proto_worlds_check.sh`: every prototype world, crash check.

No PCSX2 captures were made: getting the prototype into a given race takes menu input that
PCSX2 can't script. The comparisons above are against the user's PCSX2 screenshots: Joyride
`world1` with a green car and with Twin Mill II, and the pause menu.

## 10. Source projects: what people edit

The compiled `.xdl` is for the port to load fast; nobody edits it. What people work with (new
cars, music, events, missions, worlds, mods, and ports to other engines) is a **source project**:
a folder of readable text that the compiler turns into `.xdl`. Godot's split is the model: text
scenes and resources under version control, an import cache beside them.

### 10.1 Layout and file formats

* **JSON for structure, standard formats for bulk data.** Objects, tuning, links and scripts are
  JSON; the data no one reads as numbers keeps its usual format: models **glTF 2.0** (skinning
  included; Blender reads and writes it), textures **PNG** (or **KTX2** when already compressed),
  sound **WAV/FLAC/OGG**, music **`.mid`** with a JSON instrument bank naming its samples.
* **One file per object.** A car is a folder (`car.json`, its model, textures, sounds); a world
  a folder of its pieces, triggers and spawn points. Small files diff and merge cleanly.
* **JSONC** (comments, trailing commas; decided) so tuning values can be annotated. Every file
  starts with `"$schema"` and `"format_version"`.
* **Text tables per language** (`strings/en.json`, `strings/fr.json`), referenced by key; no text
  inline in objects.
* **Engine-neutral.** A project describes the game (a car, its handling, its parts), not the
  port's memory; the port's layout exists only in compiled `.xdl`, so another engine could read
  a project.

### 10.2 Links

* **`uid://`** for the project's own assets, as Godot does: a random 64-bit id stored in each
  asset (or a sidecar for binary files), a project table from id to path, paths only a hint. A
  file can move or be renamed without breaking a link.
* **`base://`** for the player's game data: the structural ids of §8.1
  (`base://HW_Cars/car_5/texture/3`) with per-release fingerprints. A project never contains base
  data, only these references (§8).
* The two namespaces never mix: a `uid://` is always something the project ships, a `base://`
  never is. The packaging check (§8) relies on it.

### 10.3 Inheritance, overrides, mods

* `"extends": "base://HW_Cars/car_5"` (or a `uid://`): an object takes everything from its parent
  and overrides only the fields it names, as Godot's inherited scenes do. A new car is a few
  changed fields and its own art.
* A **mod** is a set of overlays on objects it doesn't own: JSON Merge Patch (RFC 7396) per
  object, plus its own new objects. Mods have an explicit load order, and two mods changing the
  same field are reported as a conflict, not resolved silently.

### 10.4 Schemas from the decomp

* Field names come from the BrainCore structs (`maxHealth`, not `+0x5C`). The **JSON Schema for
  each object type is generated from the C struct definitions**, so editor, compiler and port
  share one source of truth.
* **Undecoded bytes** (about 18% of the struct fields in the port's headers are still `pad`/`unk`:
  463 of 2631; `CarA` 15, `CarAInfo` 12, `Creature` 11, `WorldGameData` 9, `ProjectileInfo` 8,
  `IfaceObjectDef` 5) are sorted by what the three binaries do with them, not shown as hex:
  * **read by the game, meaning unknown**: a typed field named by offset (`"unk_0x38": 1.5`), the
    type taken from how the code loads it (`lfs` → f32, `lbz` → u8). Editable, and a name can
    replace the offset later without changing the data;
  * **never read by any release**: left out of the JSON and taken from the base object at build
    time (an `extends` default), so modders don't see bytes that do nothing;
  * **whole blocks no tool parses yet**: a binary sidecar beside the JSON, named by structural id;
    rare, and each one is a to-do for the decoders.

  A round trip stays byte-exact either way; a scan of the binaries' loads per struct offset (the
  tooling the decomp checks use) does the sorting.
* Schemas give editors autocomplete and inline errors; a `format_version` bump comes with a
  migration step in the tools.

### 10.5 Ids the game needs

The game addresses cars, worlds, packs and items by number, and **save files store those
numbers**. The compiler assigns them from uids and records the assignment in **`ids.lock`**
(committed with the project), so a car keeps its number across builds and installing a mod does
not shuffle anyone's save. Base content keeps its retail numbers.

#### 10.5.1 As built in HWVX-SDL: the content registry

Branches `registry` and `registry2` of HWVX-SDL (`src/game/Port_Content.c`, `Port_ContentData.c`).
The game's cars, tracks and items (code variables 0x64, 0x65, 0x66) are counted from the data and
their unlocks kept in a registry sized to the count, instead of the save struct's `cars[0x21]`,
`tracks[0x39]`, `items[0xC]`:

* **Numbers.** 0..retail-1 are the struct's arrays; then base content past them that the data
  has (a 34th pack in `HW_Cars` is `base://HW_Cars/car_33`); then entries registered by id
  (`Port_ContentRegisterEntry`: a project's `ids.lock`, the test switches). Cars are counted from
  `HW_Cars`' TOC (33 on every release); the game itself never learns a count: tracks are numbers
  the interface scripts set and test, in families, and items the `HW_Items` packs 0-11, so both
  stay the array's count plus registered entries.
* **The struct stays.** `HotWheelsGameData` keeps its layout (memory card files, imports) as the
  view of the retail entries: every write goes to both, and where the game replaces the struct
  (a new game, a load, the end of an online session) the registry takes it back. Entries past
  the arrays live only in the registry and the JSON save (§11.4), by id, per save slot: the slot
  the game saves the live game to gets the registry's values, the others keep theirs as the game
  moves its slots. Without the entry's id (a mod removed) they stay, marked inactive.
* **Choke points.** `GameSettings_Get/SetCodeVariable` 0x64-0x66 and the autosave's unlock check;
  the car and item selects' ranges (the carousel variables that number entries, min 0 max
  retail-1, are widened to the count as the screen is created; with the retail counts nothing
  changes); the results screens' car names (variables reading code variable 0xCB); online's
  car clamp. An extra car whose number `HW_Cars` has no pack for loads the pack it aliases
  (`DataPack_LoadFile`), its scripts renumbered to its own once loaded.
* **Names and pictures.** The selects show an entry through lists indexed by its number, in the
  interface pack: a picture (an image whose texture key is the carousel variable: 36 keys for
  the cars on every release, 14 for the items), a name (a text list, 36 / 12) and a paragraph
  (how to unlock it). Past a list's end the last entry shows, and keys between the base count
  and the list's end are the pack's own (a "?"). As a screen is created, each list it numbers by
  a kind's variable is replaced by a copy as long as the count (shared by content, kept for the
  run): a registered entry's picture is the texture its `thumbnail` link names
  (`base://<archive>/<type>_<id>/texture/<n>` of a loaded pack, §8.1), else its alias's; its name
  its `name`, else the alias's followed by its `suffix` (the test switches: `" (test)"`); its
  paragraph blank. The carousel's window variables start at neighbouring entries modulo the
  retail count (the item select's left windows at 10 and 11 for item 0); widened, they're
  rebased around the middle window so they stay neighbours modulo the new count.
* **HUD icons.** The race HUD's item icon is not a list: it is the item's own `WeaponInfo.hudIcon`
  (a texture of its `HW_Items` pack, `InterfaceHudInfo.weaponIcon`), so an extra item shows its
  pack's icon: an aliasing one its alias's (item 13 aliasing Armor Repair shows its wrench), a
  mod's the one its pack brings.
* **Item numbers are not pack ids.** `HW_Items` packs 0-11 are the 12 items, but 16-79 are the
  AI's and the worlds' own, loaded by pack id by world scripts and spawn points. So registered
  items are numbered 12, 13, ... with no gap, and the places that turn the player's item (code
  variable 0xCC, the item select's list) into a pack go through the registry
  (`Port_ContentItemPack`): an aliasing item loads its alias's pack and script, one with data of
  its own a pack numbered past the archive's last (where a mod's build puts it). The release
  does the same kind of mapping itself (item 3 loads pack 0x4D with a cheat). Cars need none:
  they are numbered past `HW_Cars`' highest pack.
* **`ids.lock`.** JSONC, the project's (`HWVX_IDS_LOCK`, else `ids.lock` in the preferences
  folder), read once as the game settings start (`Port_ContentLoadLock`):

  ```jsonc
  {
    "format_version": 1,
    "next": {"cars": 36, "tracks": 57, "items": 13},   // never goes down
    "cars": {
      "uid://8f2c...": {"number": 33, "alias": "base://HW_Cars/car_3", "name": "Alpha",
                        "thumbnail": "base://HW_IPack/interface_6/texture/70", "unlocked": false},
      "uid://41a0...": {"number": 34, "alias": "base://HW_Cars/car_4", "active": false},
      "uid://77d1...": {"alias": "base://HW_Cars/car_5"}   // no number yet: the game assigns 35
    },
    "tracks": {},
    "items": {"uid://c3d9...": {"number": 12, "alias": "base://mcp/item/2"}}
  }
  ```

  Entries are registered at their numbers, in number order. One without a number, or with a
  number base content or another entry has, gets the next past every number the file ever gave
  (`next`), and the file is written back (its other fields kept). So an entry keeps its number
  for good: removing an entry leaves its number unused, adding it back with its number restores
  it, and saves (keyed by id) are right throughout. `"active": false` keeps a number reserved
  for content that's away; so does an entry without an `alias` (data of its own: no mod loading
  yet). A reserved number, and any between registered ones, is a placeholder: never unlocked, no
  id to save by, base entry 0's data, shown as the lists' own last entry with no name. An
  unreadable file is left alone.
* **Archives of any size.** The file layer kept 50 TOC entries per archive type and read an
  archive's first 0x800 bytes (84 entries); `HW_Items` already has 49, and an archive with more
  packs (a mod's) overran the cache. TOCs are now read whole (`DataPack_LoadFile`, the worlds'
  quadrant TOC, the registry's scan).
* **Tracks: what a new one needs.** Nothing loads by track number: the number (0x65) is what the
  interface scripts and worlds test for unlocks and show names by. A mode's world select is a
  carousel over two parallel variables: the progression level index (0..levels-1, its pictures
  and the 0xF event's argument, `StateProgressionManager_SetProgressionID(mode, index)`) and the
  track number in the mode's family (its names and unlock paragraphs, one 57-entry list shared
  by all the selects: Joyride 0-4, missions 5-18, battles 19-26, drag races 29-34, challenges
  39-56 on the GameCube). The level index picks the MCP progression script's level: world name,
  pack, steps, win and lose sequences. So a new track is: a level in the mode's progression
  script, a world archive (world, collision, texture, quadrant geometry packs, its own scripts
  setting 0x65 for what it unlocks), a track number (past the families: 57, 58, ..., which the
  registry already counts and saves), and its entries in the select: a picture, a name, a lock
  condition on its number, the carousel's ranges. The registry side is cheap and could extend
  the select the way it does the car select, but without a level in the progression script
  there is nothing to start, so a new track is mod and data work (an MCP overlay and a world
  archive, §10.3) before the port needs anything more.
* **Testing.** `HWVX_TEST_EXTRA_CARS=2` / `HWVX_TEST_EXTRA_ITEMS=2` (aliasing cars 3, 4 and
  items 0, 1, named after them with " (test)"), `HWVX_TEST_UNLOCK_*`, `HWVX_IDS_LOCK`;
  `HWVX_START` opens a mode's menu, world select, car or item select, or starts a race, skipping
  the menus before it; `HWVX_MENUDUMP=1` logs every list a screen shows (names, pictures,
  paragraphs, ranges) and the progression scripts' levels, so checks are greps. `test_content`
  (ctest, headless, about a second) boots the game per scenario with a stub renderer and checks
  that log against the GameCube data: the retail car select, extra cars and items, an
  `ids.lock`, item 16 loading pack 4, an `HW_Items` of 129 packs, and each mode's world select
  against its progression script; `savetest` covers the registry, an extra car's save and
  `ids.lock` (add, remove, re-add, conflicts). The title demo is frame-identical on GameCube
  data.

**Online.** Registered cars and items are numbered by each machine's own `ids.lock`, so a number
alone can name different content on two machines. `Port_ContentWriteChoice` therefore sends a
choice past the base content as its id, together with the base entry it is made from. On the other
machine, `Port_ContentReadChoice` finds it by id; when that machine lacks it, the base entry stands
in. Base content keeps its number, which is the same everywhere. The protocol is version 2
(`NET_PROTOCOL`). `savetest` covers both machines' views.

**Tracks.** A registered track becomes a level of its mode's progression and an entry of the
mode's world select, with its own name, picture and padlock. It loads its alias's world, or its own
`world`, and is saved by id. For testing: `HWVX_TEST_EXTRA_TRACKS=N`, `HWVX_TEST_UNLOCK_TRACKS`,
and `HWVX_START=worldselect:mode=M,world=W`.

### 10.6 Events and missions

The game's scripting is already data (trigger types, action functions, event sequences:
`Trigger.c`, `Action.c`, `EventSequence.c`). In a project they are named trigger and action types
with their parameters and uid references, validated by schema. A visual graph editor can be built
on that later without changing the format.

### 10.7 Building and checking

* **Deterministic**: the same project gives byte-identical `.xdl`; the cache is keyed by hashes
  of the inputs.
* **Errors at `file:line`**, naming the field; the engine's limits (33 cars, 0x200 textures, 64
  voices, the 8-car arrays) are checked at compile time instead of found as crashes.
* **Round trip**: extract a release to a project (locally, from the player's own copy) and
  compile it back; it must equal the direct build. That proves the project format loses nothing.
* **Hot reload** in development: the port watches a project folder, recompiles what changed and
  reloads that pack, so tuning is edit, save, drive.

### 10.8 As built: packs to projects and back (step 3c)

HWVX-SDL `tools/xdl_graph.py` (whole packs) and `tools/xdl_gltf.py` (geometry), on top of
`xdl_struct.py` and the generated schemas ([`schemas.md`](schemas.md)). They work on
`hwvx-x extract` trees. The port is not involved: it still compiles packs to its `.xdl` cache (§5.1).

```
xdl_graph.py extract-archive <extracted archive> <project archive>   archive.jsonc + one folder per pack
xdl_graph.py build-archive <project archive> <out>                    packs + manifest.json (hwvx-x pack's input)
xdl_graph.py extract <pack> <project pack> / build <project pack> <out> [--check <pack>]
xdl_graph.py roundtrip <survey> [--rel gc,pc,ps2] [--archive ..] [--orig <orig>]
xdl_gltf.py export <pack> <out>                                       the geometry alone
```

**Nodes.** A pack's blob is cut at every pointer and local-ref target, at the header's tables
(each texture header, shader and trigger record, the palettes) and at every geometry. Two
things claim their whole extent, so a pointer into them is an interior link and not a cut:

* a geometry, from its header to the last byte its levels use;
* an object of a schematised struct (the 14 schemas). The roots, the Port_Swap walkers'
  labels and links from typed fields find these. An object is typed only if no scalar field
  sits on a relocation slot and its links match their slots' kinds; otherwise it stays raw.

A node runs to the next cut. A GameCube car has about 2,800 nodes.

**Ids: the canonical path.** Every node gets one `base://<archive>/<pack>/...` id:

1. **Fixed ids.** `geometry/N` and `texture/N` (pairing.md: the GameCube's index, via the pairing
   table on the other releases), `shader/N`, `trigger/N`, `palette/0`, the root table's entries
   (`CarAInfo/0`, `ItemRecord/i`, `IMHeader/i`, `SoundSeq/i`), and `World/0` or `header/0` for
   offset 0.
2. **Paths.** Every other node is named by the **first path that reaches it**: breadth-first from
   the root entries, then from the other fixed ids and from offset 0, with each node's slots in
   offset order. The shortest path wins, and ties go to the earlier root and lower offset.
   * A step is the field path where a schema covers the slot (`CarAInfo/0/engine`,
     `.../aiScripts/2`, the same as xdl_struct's links), or `p<hex offset>` elsewhere
     (`CarAInfo/0/rollSound/p18`).
   * Repeats of one step are counted: `p0_r12` is twelve `p0` steps (linked lists).
   * Any other path to the same node is a link to that id. Car 5's three `aiScripts` slots that
     hold one script all read `.../aiScripts/0`.
3. **Orphans.** Nodes no root reaches are `orphan/K`, in blob order.

A link into a node is `<id>/o<hex offset>`. Inside a geometry it is a structural name
(`geometry/3/level/0/group/2`, `.../uvscroll`, `.../vertices`), so the link still holds after the
geometry is re-encoded.

**Files.** One file per object. A car folder holds `CarAInfo/0.jsonc`,
`CarAInfo/0/engine.jsonc`, `geometry/14.gltf`, `texture/3.jsonc` and so on, about 620 files and
5 MB on GameCube.

* **Typed objects:** the xdl_struct JSONC (`$schema`, `extends` its own id). The bytes the JSON
  doesn't carry (hidden fields, padding, the node's tail, and the links in them) go in
  `<name>.base.jsonc`. This is the scrubbed base of schemas.md §6, kept in the project, so a
  build doesn't need the original pack.
* **Geometry:** `geometry/N.gltf` and `.bin`, plus `N.xgeo.jsonc` and `N.orig.bin`, which hold the
  original encoding, its links, the interior anchors and the glTF's content hash.
* **Everything else:** a raw record with:
  * `bytes` (hex lines; a `.bin` above 256 bytes), with pointer and local-ref slots zeroed;
  * `links`, `textures`, `sounds`, `refs` and `shaders` by slot offset;
  * `type`, the walker's label, where there is one.

  A raw node gets its own file if it is a root, a texture, shader or trigger, an orphan, 4 KB or
  larger, or labelled with a component kind (comp_survey.KINDS). The rest (key tracks, strings,
  small arrays) sit in their canonical parent's file under `children`. So a subgraph is a
  folder: an EventSeq with its emitters is `.../explosionThing.jsonc` and `.../explosionThing/`.
  A component (§13, task 4a) is that folder moved out and given a uid.

**pack.jsonc and pack.order.json.**

* `pack.jsonc` holds:
  * pack.json, with the header's data offsets as links;
  * the manifest entry;
  * the layout: node order, alignment and file;
  * the sound sections, copied as files for the media step.
* `pack.order.json` holds the relocation tables' original order as (layout index, offset)
  pairs. The pointer lists are in the exporter's traversal order, not sorted, in 505 of 511
  GameCube packs. A build keeps that order for slots that still exist and appends new ones
  sorted.

**Build.**

1. Every node is encoded:
   * a raw record from its bytes;
   * a typed object by xdl_struct's encoder over its base, with links kept symbolic;
   * a geometry from its original bytes while the glTF hashes as extracted, otherwise encoded
     afresh.
2. The nodes are laid out in order, each at its alignment. Unchanged sizes give the original
   offsets.
3. Links resolve to offsets, and the tables and pack.json are written:
   * a texture or sound link goes back to its index through the pairing table;
   * a local ref's value is the target's index in the geometry scan.

A node that grows or shrinks moves everything after it, and every link follows. The tests grow a
record by 16 bytes and extract the built pack to the same objects.

**glTF.**

* **Nodes and meshes.** One file per geometry. Its root node `geometry_N` carries the header,
  the LOD distances and the level records in `extras`. Each level is a child node (`lod0`, ...)
  with its own mesh, and its distance in `extras`.
* **Primitives.** A group is a primitive. Its `extras.group` holds flags, colour, blend, sort key,
  the raw texture slot and the uvScroll record.
* **Materials.** A material carries the texture as a `base://` id in `extras.texture`, with no
  image. The PNGs of the media step are found by that id, not by path.
* **Attributes:**
  * POSITION and TEXCOORD_0;
  * NORMAL for lit and env-mapped groups, COLOR_0 (u8, GameCube scale) for the others, as the
    draw code reads them;
  * NORMAL is normalised for the validator; when the source's normals aren't unit length, the
    exact ones are kept in `_NORMAL_RAW`.
* **Skinning.** A skinned geometry gets a skin over 8 bone nodes and one JOINTS_0 bone per
  vertex with weight 1.0. Positions stay bone-local, as stored, and the per-bone counts are in
  the level's extras.
* **Shadow volumes** are LINES primitives (their edges) with the edge normals in extras.
* **Validator.** The Khronos glTF Validator reports 0 errors on every car geometry on all
  three releases (1,847 files) and on 1,980 world, quad, item and interface geometries; the
  remaining notices are info-level.
* **Encoding edited geometry.** GameCube display lists and PC index lists are written afresh,
  with per-attribute arrays deduplicated and skinned vertices sorted by bone. The bounding box,
  radius and centre are recomputed (geometry.md §1.1).
* **Not decoded or encoded on PS2.** Edited PS2 geometry (VIF packets) isn't encoded yet; the
  build refuses it. PS2 shadow volumes (kind 3 levels, which point to a packet of their own)
  aren't decoded and stay raw bytes beside the glTF.

**Round trip** (`roundtrip --orig`: every pack extracted, built and compared file by file, then
each rebuilt archive put through `hwvx-x pack` and compared with the original file):

| release | packs | byte-identical | archive files identical |
|---|---|---|---|
| GameCube | 512 | 512 | 105 / 105 |
| PC | 506 | 506 | 99 / 99 (against the pristine `orig/pc-backup`, then `orig/pc`) |
| PS2 | 512 | 512 | 105 / 105 |

Every pack kind is covered: cars, worlds, world textures, quad geometry, collision, interface,
items, share, audio, music and link. On GameCube the 512 packs make 1.17 million nodes, with
4,313 typed objects and 21,652 geometries. A whole release extracts and builds in about 6
minutes and takes 2.2 GB.

**Typed so far**:

* CarAInfo and its 5 parts in every car;
* DestructibleInfo, WorldGameData and CreatureInfo in worlds;
* ItemRecord, WeaponInfo and ProjectileInfo in items.

The rest of the graph is raw records. By walker label, the most common are:

| category | labels |
|---|---|
| interface | StateAnim, ObjAnim, ObjState[], CondList, ObjSub, TextInfo, IMHeader, IMObject[] |
| key tracks | keys.vec4, keys.active, keys.color, keys.uv |
| effects | ParticleEmitter, Particle, EventSeq |
| sound | SoundSeq, Sound |
| world | ThingRecord, Trigger, TileCar |

Nodes no walker labels (about 260,000 on GameCube) are the largest group. Each label is a
schema to write.

Marking the headers' link fields for this step (`@xdl:` on ItemRecord.hudName/pickupDebris,
WeaponInfo.hudIcon, ProjectileInfo.geometry/holeTexture/trailFx/impactFx/expireFx,
WorldGameData.mapTexture/radarBlip/lightTexture/envmapA/envmapB) made those objects typable.
Three things are still to fix:

* WeaponInfo `unk_0x1C` (u8[4]) holds a pointer;
* ProjectileInfo `slaves` (u8[8]) holds a pointer at +4;
* a geometry slot that holds a share pack's export has no link form yet, so those
  ProjectileInfo objects stay raw.

#### 10.8.1 Checks when building (after the code review)

After the paragraph "A link into a node is `<id>/o<hex offset>`. Inside a geometry it is a
structural name ...", add:

Links that reach into a geometry by offset (`geometry/1/o3b04`: world quads' orphans point into
vertex and display-list data; many thousands of such links in the GameCube world packs)
can't follow a re-encoding. Building a project whose edited glTF re-encodes such a geometry is
refused, with the links listed. Every `o<hex>` offset must lie inside its object.

Under **Files**, typed objects, add:

A typed file lists every field. Its base (`.base.jsonc`) has the JSON's fields zeroed, so a
field left out is an error naming it, not a 0 (top-level texture links excepted: the base keeps
their bytes).

In the command list: `extract` / `extract-archive` replace the output folder only when it is
empty or a previous extract (has `pack.jsonc`); `--force` replaces any folder.

Building writes a relocation table the source pack lacked (car_5 has no `shader_relocs.bin`)
once an edit gives it entries, and adds its `pack.json` section.

## 11. Save data

Today's saves are emulated GameCube memory-card files (`<pref>/memcardA/*.sav`, `card.c`) holding
the game's own save structs: unlocks as **fixed arrays indexed by number** (`HotWheelsGameData`:
`cars[0x21]`, `tracks[0x39]`, `items[0xC]`, cheats, extras, script values; DD Rescue's world state),
written **in place** (`r+b`) and guarded by one checksum. That can't hold a 34th car, gives an
unlock to the wrong car when a mod renumbers them, and a crash mid-write ruins the file. When the
game runs on `.xdl` data it uses its own save format instead.

### 11.1 Format

* **JSONC, keyed by id, not index**: unlocks and progress against `base://` and `uid://` ids
  (`"base://HW_Cars/car_5": {"unlocked": true}`), DD Rescue's world state as named records. A
  removed mod's entries stay, marked inactive, until it comes back; nothing shifts.
* **Versioned** (`"format_version"`), upgraded by migration, never discarded.
* The game's code keeps its arrays: on load the save fills them through `ids.lock` (§10.5), on
  save it reads them back. Content past the retail arrays (a 34th car) is kept by id in the port's
  own table, which the port's unlock checks consult.

### 11.2 Protection

* **Atomic writes**: write `save.json.tmp`, flush, rename over `save.json`. A save is wholly old
  or wholly new.
* **Rotating backups**: the last few saves kept, each with a hash; a damaged current save
  offers the newest good backup instead of a reset.
* **Profiles per mod set**: vanilla and modded progress are separate unless the player merges
  them; a broken mod can't reach the main save.
* Integrity, not anti-cheat: the format is open and players may edit their own saves.

### 11.3 Importing existing saves

Converting to `.xdl` imports progress from every save the player has, whichever releases:

| source | how | state |
|---|---|---|
| the port's own `.sav` files | the game's struct, already in host order | straightforward |
| GameCube memory card (Dolphin `.gci` exports, raw card images) | `GHWE78` file `hwvx`: one 0x2000 block after the 0x40-byte `.gci` header, or found through a card image's directory and block map; name and comment, banner and icons (0x800), then at +0x840 the save, **big-endian**, checksum verified | **imported** (§11.5) |
| PC saves (`My Documents\VelocityX Saved Games\velocityX`) | one file, 8 slots of the GameCube's `HotWheelsGameData` in little-endian after a 0x194-byte header and settings block, checksum verified ([`../pc-version/README.md`](../pc-version/README.md) "Save files") | **imported** |
| PS2 memory card (`.ps2` card images, `.psu` exports) | the card's file system (`ps2_mcd.py`), then `BASLUS-20412/BASLUS-20412`: the GameCube's slots in little-endian after **packed** settings, checksum verified ([`../ps2-version/README.md`](../ps2-version/README.md) §5.5) | **imported**; `.max` not (its LZARI compression isn't implemented) |

All three releases store the same file: `SAVE_FILE_MAGIC` 4, size, the game's checksum (sum of
byte + 1), `GeneralGameSettings` (0x150; the PC's 0x38 bytes longer, the PS2's packed), then 8 ×
0xF4 `HotWheelsGameData` slots. GameCube big-endian, PC and PS2 little-endian. Saves made after the same
progress (Challenge 1) on all three hold the same tracks, items, extras and script values; the
cars differ only by the releases' defaults (GameCube 3–5, its 0–2 coming from the GBA link; PC
and PS2 0–5). An import keeps the union; the PC's and PS2's cars 0–2 are their defaults, so a
GameCube player importing such a save gets them only by choosing to (§11.5).

Retail ids are the same on every release (§8.1), so each save maps through its release's numbers
to the same `base://` ids. Several saves **merge as a union** (an unlock in any counts; for counters
and best results the highest) with a preview and the choice to import one save only. The
original files are never changed. An export back to a release's own format remains possible
where the content is retail-only.

### 11.4 As built in HWVX-SDL (step 4c)

Branch `saves` of HWVX-SDL. **`HWVX_SAVES=json`** switches it on (off by default, so retail
behaviour is unchanged; not yet tied to running on `.xdl`). `HWVX_PREF_DIR` points the whole
preferences folder elsewhere for tests.

* **Where it hooks:** the memory card calls in `FileManager_NGC.c` (`MemCard_Open/Read/Write`,
  `File_Delete`), which the game always makes with a whole `SaveFile`. Everything above them is
  the game's own flow: card status, the slot list, overwrite, naming, "Saving...", the autoload
  and the port's autosaves. A save with no good file reads as zeroes, so the game's checksum check
  shows its damaged-file screen, as for a bad card.
* **Files**, `<pref>/saves/<profile>/` (profile `default` only, `Port_SavesProfile` is where a
  profile per mod set will be chosen): `hwvx.json`; backups `hwvx.1.json` (newest) to
  `hwvx.5.json`; `hwvx.json.tmp` during a write; `hwvx.damaged.json`, a damaged save moved aside
  by the next write (not rotated into the backups); `hwvx.imported`, the card file it was
  imported from; `deleted/`, where the game's Delete moves the files.
* **Write:** the `.tmp` is written and synced (`fsync`, `FlushFileBuffers`), the backups move up
  one and the current save is *copied* to `.1`, then the `.tmp` is renamed over the save
  (`rename`, `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`): the save itself is never missing.
* **Hash:** each file ends in `// sha256: <hex>` over everything before that line, so one file is
  self-checking and no index has to be kept in step. A file without the line counts as edited by
  hand and is read as it is (logged); one whose hash is wrong, that won't parse or isn't a save
  falls back to the newest good backup, logged. A `format_version` newer than the build is never
  read.
* **Ids** (`Port_SaveIds.c`, one table): cars `base://HW_Cars/car_<n>` (the car packs);
  tracks and items are numbers the MCP script gives events and items, not packs, so
  `base://mcp/track/<n>` and `base://mcp/item/<n>` (confirmed 2026-11-11). The chosen vehicle
  and item are ids too.
  `ids.lock` entries will be consulted first. An id with no index (mod content not installed, a
  car past the array) is kept per save slot and written back with `"inactive": true`; a chosen
  vehicle that isn't installed reads as none (-1) and keeps its id. Which slot keeps them across
  the game's shifting of slots: the one with the same name and date, else the same name. The
  port's unlock checks don't consult kept entries yet (nothing past the arrays exists yet).
* **Lossless:** the round trip is byte for byte, padding included (written only when not zero;
  DD Rescue's struct has an unnamed 2-byte hole after `carMode`). DD Rescue's world state is
  named records (timers, variables, world cars...), its byte arrays as hex. Its `SaveFile`
  (0xE83C bytes) doesn't fit the card file's 0x17C0-byte data area, which the card path
  overruns; the JSON store holds it whole.
* **Migration:** with no JSON save, `memcardA/<name>.sav` is imported once (checksum checked; the
  `.sav` stays). The `.imported` marker keeps a deleted save from coming back.
* **Tests:** `tools/savetest.c` (`ctest -R savetest`): the GameCube example `.gci` through JSON
  and back field by field, random saves of both game types byte for byte, kept ids, a write
  stopped after its `.tmp`, damaged saves, rotation, import. In game: a save made from the Save
  Game screen loads after a restart; a damaged save boots from its backup. With the switch off
  the title demo is frame-identical.

Importing other releases' saves into an existing save: §11.5.

### 11.5 As built in HWVX-SDL (step 4d): importing every release's saves

Branch `saveimport` of HWVX-SDL. `src/game/Port_SaveImport.c` reads the containers and merges;
the launcher's **Import saves...** is the player's way in. It writes through the store of §11.4
(so it works whether or not `HWVX_SAVES=json` is set; the game reads the result with the
switch on).

**Containers** (`Port_SaveImportRead` tells them apart by content, not by name):

| container | release | how it is read |
|---|---|---|
| the port's `memcardA/hwvx.sav` | GameCube | the card file (name, comment, banner, icons), the `SaveFile` at +0x840 in the order of the machine that wrote it |
| Dolphin `.gci` | GameCube | the 0x40-byte directory entry (`GHW?` by maker `78`, file `hwvx`; any region), its block count matching the size; the card file follows |
| raw card image (Dolphin `MemoryCardA.*.raw`) | GameCube | block 0 header; blocks 1-2 the directory and its backup (127 × 0x40 entries, update counter at 0x1FFA, checksums at 0x1FFC over 0..0x1FFC); blocks 3-4 the block map and its backup (checksums at 0 over 4..0x2000, counter at 4, u16 next block for blocks 5.., 0xFFFF the last). Checksums are the SDK's: the sum of big-endian u16s and the sum of their complements, 0xFFFF read as 0. The good copy with the newer counter is used; the file's blocks are followed through the map, bounds and length checked |
| `velocityX` | PC | magic 4, size 0x928, checksum over its own 0x928 bytes; the settings moved into the GameCube's layout (below) |
| `BASLUS-20412` (the file itself) | PS2 | magic 4, size 0x8F0, checksum; packed settings moved into the GameCube's layout (below) |
| PCSX2 `.ps2` card image | PS2 | `ps2_mcd.py` in C: superblock, two-level FAT, 512-byte directory entries, raw pages with 16 ECC bytes skipped when the size says so. `BASLUS-20412/BASLUS-20412` first, else any folder holding a file of its own name that reads as a save (other regions). A subdirectory's entry count is the one in its parent's entry (its own `.` entry doesn't hold it); the root's is in its `.` |
| `.psu` (uLaunchELF export) | PS2 | the folder's 512-byte entry (the card's directory entry format), `.` and `..`, then each file's entry and its bytes padded to 1 KB; the file named as its folder is the save |
| `.max` (Action Replay MAX) | PS2 | recognised (`Ps2PowerSave`) and refused: its LZARI compression isn't implemented, so it would be guesswork; export as `.psu` or import the card image |

Doomsday Rescue saves are recognised by their size and refused (Hot Wheels progress only).

**Settings layouts**, moved into the GameCube's before `Port_SaveFromBinary` reads the result
(each file's own checksum is checked first, over its own bytes):

* **PC**: settings 0x188 = s32 difficulty, audio (4), **4 × 0x20 controller settings**, 8 × 0x20
  slot entries at +0x88. Each player's 0x20 bytes are the GameCube's 0x12 (`data[0]` vibration,
  code variable 0xD1; `data[1]` control scheme, 0xCE; 16 bytes of the MCP binding, the same bytes
  on all three releases) followed by a 13-byte gamepad button map (`10 11 9 7 8 6 4 5 3 2 12 13 1`
  by default, the PC decomp's `func_00429F10`) and one byte of padding: 4 × 14 = the 0x38 extra
  bytes. The button map is read (`PortPCControls`) but not imported.
* **PS2**: settings 0x150 but packed: **u8** at +0 (the GameCube's s32 difficulty; the PS2's
  `GameSetting_Init` sets it to 1, the GameCube's to 0, so it isn't carried over), audio at +1,
  controllers at +5 (`GameSettings_GetControllerSettings`: base + 5 + 0x12 × (player − 1)), 3
  bytes of padding, slot entries at +0x50 as the GameCube's. The slots after the settings are the
  GameCube's struct.

The example saves through every container agree: the three Challenge 1 saves hold the same
tracks (0, 5, 19-22, 29-32, 39, 40), items (0-3), extras, cheats and script values; the cars
differ only by the defaults (GameCube 3-5, PC and PS2 0-5).

**The merge** (`Port_SaveImportMerge`, a pure function on two `SaveFile`s; Hot Wheels saves):

* Slot by slot: a save goes into the store's save **of the same name**, else into the first
  free slot as a new save (its name, date, vehicle and item); with eight saves and none of that
  name it is skipped (the preview says "no room").
* **Unlocks** (cars, tracks, items, cheats, extras): the higher value of the two, so an unlock
  in either counts and a value other than 1 isn't lowered. Nothing is ever cleared.
* **Script values** (the 30 `miscValues`, the MCP script's own numbers): taken **as a set**
  from whichever save is further on (more unlocked cars, tracks, items and extras); a tie keeps
  the store's. A per-value maximum could combine values no game ever wrote; a whole set from
  one save can't.
* The store's name, chosen vehicle and item and settings stay; the date becomes the later one.
  With no save yet, the settings come from the first file (in the GameCube's layout).
* **Default cars**: cars 0-2 of a PC or PS2 save are that release's defaults (the GameCube's come
  from the GBA link), so they are not taken unless the player asks (`defaultCars`); the preview
  lists them as "3 default cars (0 1 2) left out" and the launcher shows the choice. A
  GameCube save's cars 0-2 are real unlocks and always count.
* `onlySource`/`onlySlot`: one save of one file only.

**Preview and import:** `Port_SaveImportPreview` merges into a copy of the store's save read by
`Port_SavesPeek`, which writes nothing: with no JSON save yet it reads `memcardA/hwvx.sav` in its
place (what the store would import first), so the preview shows the merge the import will do.
`Port_SaveImportApply` reads the save through the store (importing `memcardA` first, marker and
all, as §11.4), merges, and writes it with `Port_SavesWriteImported` (atomic, the save before kept
as `hwvx.1.json`; `imported_from` set if the save had none). An import that changes nothing
writes nothing. A damaged save with no good backup is refused, not replaced. Source files are
only ever read.

**Launcher:** a Saves row, **Import saves...**: the system's file dialog (several files; "All
files" for the PC's extensionless `velocityX`). Each file is read; one that fails says why in
the status line. The preview lists the files (release, container), where they go ("Into:"),
and per save what it adds (car, track, item numbers; extras, cheats, values raised, script
values, the default cars left out). Choices: **Saves:** cycles every save / one save; **[ ] Add
cars 0-2 too** (shown when a PC or PS2 file is in); **Import**, **Add file...**, **Cancel**
(Escape, B). Mouse, keyboard and gamepad as elsewhere. `HWVX_LAUNCHER_IMPORT="FILE;FILE"` opens
the preview on start (for tests and screenshots with `HWVX_LAUNCHER_SHOT`).

**Tests:** `tools/saveimporttest.c` (`ctest -R saveimporttest`), with the example saves
(`HWVX-SDL/build-saves`, `orig/`, found by CMake): every save through every container (the
`.gci`; a `memcardA` `.sav` and a raw card image made around the `.gci`'s own card file, its
blocks out of order and its backup directory stale; `velocityX`; the PS2 file, the real `.ps2`
card and a `.psu` made from its file); the three releases' Challenge 1 saves agree, also as JSON
ids; the merge rules; the default-car choice; one save; the preview writing nothing; a second
import changing nothing; `memcardA` imported first; damaged cards (directory, block map, chain,
superblock, FAT), cut and damaged files, `.max`, DD Rescue and junk refused with a reason; the
sample files unchanged at the end. 143 checks, also under ASan/UBSan.

Not done: DD Rescue's saves (its `playerStats` in the other byte order are still bytes);
`.max`, CodeBreaker and X-Port exports; exporting back to a release's format.

### 11.6 After the code review (fix-saves)

Corrections to §11.3–§11.5 above:

**§11.3 Files**, add after `hwvx.damaged.json ...`:
`hwvx.before-import.json`, the save as it was before the last import (replaced only by the next import; not one of the rotated backups).

**§11.3 Write**, replace the sync and rotation sentences:
* the `.tmp` is written and synced (`fsync`; on macOS `fcntl(F_FULLFSYNC)`, falling back to `fsync`; `FlushFileBuffers`), and so is the folder after the rename.
* A write whose text is the same as the save's changes nothing: no rotation, so repeated autosaves don't push the backups out.
* Only a save the reader calls damaged (bad hash, won't parse, not a save, unreadable) is moved to `.damaged.json`. If it can't be checked (out of memory) it goes into the backups as usual; if its text can't be loaded just then, it is renamed to `.1` rather than copied.
* The read cache (keyed on the files' sizes and times) is dropped after every write and delete.

**Preview and import**, replace "(atomic, the save before kept as `hwvx.1.json`; ...)" with:
(atomic; the save before it kept as `hwvx.before-import.json` and in the backups; `imported_from` set from the file actually imported if the save had none). With one file picked (`onlySource`), `imported_from`, the log and a blank save's settings come from that file. A new save from a PC/PS2 file whose chosen car is a default car left out (cars 0-2) gets its first unlocked car instead (-1 if none).

**Loading a slot** (§11.3 Ids, after "a chosen car the data doesn't have reads as none"): a car or item outside [-1, count) reads as none (-1); the JSON reader itself stays lossless.

**JSON**: `JSON_MAX_DEPTH` (64) is the reader's nesting limit; the writer goes 16 deeper, so anything read can be written back.

## 12. Extracting existing packs

`.xgc`, `.xpc` and `.xps` packs extract to source projects (§10), so existing content becomes
something to edit, learn from or build on:

* `hwvx-x extract <pack or folder> <project>`: each pack's objects become JSONC files, its models
  glTF, textures PNG, sounds WAV, songs `.mid` with their bank. Undecoded bytes as §10.4.
* **Extraction is local.** An extracted project holds the player's game data; it is for their own
  use. Anything published (a mod) goes through the packaging check (§8), which turns copied base
  data back into `base://` references or refuses.
* The round trip (§10.7) is the test: extract a release, compile it, compare with the direct
  build.
* **Two directions, two homes.** Source project ⇄ pack bytes belongs to **Python tools** in
  HWVX-SDL `tools/`: they read a release's own packs (`.xgc` big-endian, `.xpc` and `.xps`
  little-endian) and write them back byte for byte. Pack → `.xdl` cache is the port's job (§5),
  and is done. So extraction does **not** go through the port's readers: the tools have their own
  per-release readers, and the byte-exact round trip is what keeps them honest. `xdl_struct.py`
  converts pack structs ⇄ JSONC from the generated schemas ([`schemas.md`](schemas.md)),
  `xdl_media.py` the bulk data (§12.1), `xdl_gltf.py` the models and `xdl_graph.py` whole packs: object graph, models and every other byte, so a pack builds back byte for byte (§10.8, step 3c).
* Every release extracts to the same file kinds under the same `base://` ids
  ([`pairing.md`](pairing.md)), each copy in its own release's encoding; `--release` picks one when
  the player has several, or a mixed extraction applies the rule table of §9.

### 12.1 Media: `tools/xdl_media.py`

Textures to PNG, sound effects to WAV, songs to `.mid` with their instrument bank as JSONC naming
WAV samples (step 3a). Built on HWVX-SDL branch `media`.

```
xdl_media.py extract ARCHIVE[:PACK] OUT     one pack (car_5, geometry_0_q12, a TOC index), or all
xdl_media.py build OUT PACK_FILE [--base ARCHIVE]   the pack's bytes (its TOC entry's payload)
xdl_media.py roundtrip ARCHIVE|FOLDER... [--fresh] [--keep]   extract + build every pack, compare
xdl_media.py schemas [--check]              schemas/Media{Pack,Texture,Sound,Bank}.schema.json
```

A pack's folder (`<i>` is the id's index, `ps2:5` written `ps2_5`):

```
media.jsonc                 base://HW_Cars/car_5, release, the source archive and its BLAKE2b
texture/<i>/texture.jsonc   format, size, the header's level count, per level: pixels + encoded hash
texture/<i>/level<k>.png    one PNG per stored level (level0 = full size)
sound/<i>.wav, <i>.jsonc    mono PCM16; rate, loop (in samples), volume, the descriptor's other bytes
song/song.mid               the RIFX 'MIDI' chunk's body: already a standard MIDI file
song/bank.jsonc             instruments: program (128 = drum), key range, envelope, rate, loop, sample
song/bank/<k>.wav           the bank's samples, one per distinct sample (instruments share them)
```

**Build starts from the player's copy of the pack**, found through `media.jsonc` (or `--base`) and
checked against its recorded hash, as `xdl_struct.py` starts from the `extends` object. It zeroes
every byte the files describe (texture headers, levels, CLUTs, descriptors, samples, song, bank)
and writes them from the files; everything else (objects, relocations, geometry) comes from the
base until the other tools write it.

* **Lossless** kinds are encoded from the files every time: GameCube RGBA8, PS2 PSMCT32, PS2 PSMT8
  (written as a **palette PNG**: indices and CLUT exactly; textures sharing a CLUT must keep equal
  palettes), PCM16 samples, songs, bank tables.
* **Lossy** encodings (CMPR, DXT1/DXT5, SPU-ADPCM) can't be recovered from what they decode to.
  Their sidecars record a BLAKE2b-128 of the stored bytes and of the decoded pixels or samples;
  while a file still decodes to the recorded hash, build copies the base's encoding, else it
  encodes afresh (principal-axis endpoint fit for the block formats, a 5-filter search for ADPCM,
  Pillow's octree for a PSMT8 texture edited as RGBA). A missing mip PNG is made from the level
  above.
* **Sizes**: textures keep their slot's format, size and level count (the data blob can't grow
  without the compiler). Sounds, songs and bank samples may change length: `sound_data` holds
  nothing but them (checked on every pack of the three releases), so build lays it out again,
  keeping offsets until something grows, and moves the sections after it.
* No wrap or filter in `texture.jsonc`: a pack's texture header has none (format, size, levels,
  pointer, one word); the game sets wrap per draw (`Gfx_SetTextureWrapMode`).
* Stored levels: the header's count (0 = one), except **GameCube RGBA8** (one level whatever the
  count says, as the game's `TexSize`) and the **PS2's PSMCT32 chains**, whose 1×1 level is the
  next texture's first texel (71 textures). PS2 sounds drop their silent first ADPCM block in the
  WAV; a fresh encode writes it back with the release's block flags.
* Hashes agree with the pairing table: a texture's `fingerprint` is its §3.1 `x`, a GameCube or PC
  sound's `samples` its `n`.

**Round trip**, every pack of the three retail releases (`roundtrip`, ~40 s on 10 cores):

| release | packs byte-identical | built from the files | base's encoding reused (files unchanged) |
|---|---|---|---|
| GameCube | 512 / 512 | 1,301 RGBA8 levels, 4,723 sounds, 60 songs, 60 banks with 1,259 samples | 30,207 CMPR levels |
| PC | 506 / 506 | 4,347 sounds, 54 songs, 54 banks with 1,146 samples | 48,248 DXT1, 16,333 DXT5 levels |
| PS2 | 512 / 512 | 571 PSMCT32 and 68,675 PSMT8 levels, 8,435 CLUTs, 60 songs, 60 bank tables | 4,723 sounds, 1,259 bank samples (ADPCM) |

What can't round-trip from the files alone is exactly the lossy part: CMPR/DXT levels and PS2
samples come back from the base. With `--fresh` (every lossy asset encoded from its PNG or WAV) on
HW_Cars and HW_IPack, against the original's decode: CMPR RGB RMSE median 1.0 (p90 2.1), DXT1 0.9
(2.5), DXT5 0.4 (2.1) with alpha 1.7 (5.4), ADPCM SNR median 37 dB.

Checked by playing: `ffprobe` reads the WAVs (PCM16 mono at the descriptor's rate); MuseScore
(`mscore -o x.pdf song.mid`) imports all 60 GameCube songs, and renders one to audio.
`tools/test_xdl_media.py` edits textures, palettes, sounds, songs and banks of `car_5` and
`music_1` on each release and checks the refused cases (another texture size, WAV rate or channel
count, a base that isn't the extracted copy, schema errors).

## 13. Reusable components

A car pack carries its own copies of things many cars share: particle effects, stunt setups,
the HUD layout, weapon and pickup definitions, AI setups, engine and impact sound sets, damage
stages. In a project these become **components**: one file each, referenced by uid, reusable by any
car, world or mod.

* **Extraction finds them.** The extractor fingerprints object subgraphs (an emitter with its
  particle descriptions and textures, a sound set, a weapon definition) across every pack; a
  subgraph that occurs in more than one pack becomes one component (`effects/sparks_blue.jsonc`)
  and every pack that had it references it. One-off subgraphs stay inside their pack's files and
  can be promoted to components later.
* **Components can be extended** like any object (`"extends"`, §10.3): a new car takes the stock
  sparks with another colour without copying them.
* **The compiler copies them back in.** At runtime each pack keeps its own copy: the game writes
  into some of these objects (playback state), a BrainCore pack references at most two others,
  and packs load and unload on their own (§4.6). So sharing is a source-level idea: the compiled
  pack inlines each component it uses, and only read-only bulk data (pixels, samples) is shared
  in compiled output too.
* **Kinds of component** (first list; the fingerprinting pass will show what really repeats):
  particle and emitter setups; trails and coronas; sound sets; weapons and projectiles; pickups;
  stunt and combo setups; HUD and interface fragments; AI setups; materials/shaders (animated
  textures); world props (a model with its collision); triggers and event sequences used as
  templates.

## 14. Open questions

* Compression: zstd per section costs load CPU but saves disk. Data is ~1 GB per release
  uncompressed; worth measuring before deciding.
* Memory mapping vs. reading into the game heap (handle registry support).
* Half-float uvs; BC7 for higher-quality UI textures; ASTC/ETC2 targets.
* Whether to keep a release's original arrays at all (the port's converted geometry currently
  leaves the GameCube's arrays and display lists unused in the pack).
* PS2: geometry (VIF packets) and textures (GS formats) still need research before they can
  compile to this.
* Version policy: bump `version` on any layout change and rebuild; no in-place migration.
* Mixed builds (§9): how a base other than the interface pack's release affects code that asks
  `Port_DataRelease`; whether per-asset choices should be player-overridable (a settings page).
