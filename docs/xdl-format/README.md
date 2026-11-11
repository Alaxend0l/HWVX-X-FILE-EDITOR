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
  u32   compression             0 none, 1 zstd   (Open: worth it? see §9)
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

### 4.3 Textures

* Stored as **BC1** (opaque, DXT1) or **BC3** (alpha, DXT5) with the full mip chain, the format
  the PC release already uses. GameCube CMPR re-tiles to BC1 losslessly; RGBA8/RGB565 encode to
  BC3/BC1 (or stay raw RGBA8 where quality matters: fonts, UI, **Open**).
* Colour look decided at compile time: GameCube colours or the PC's (`pc = sqrt(255 · gc)`).
  Switching rebuilds the cache, or both are stored (**Open**: size vs. instant switching).
* The texture header in the graph points into GPU_TEXTURE: `{format, width, height, levels,
  offset}`. **Open:** devices without BC support (some mobile GPUs) would need ASTC/ETC2 or a
  decode fallback; the compiler can choose per target.

### 4.4 Audio

* SFX samples: PCM16 host order in an AUDIO section; descriptors point into it.
* Music: songs (standard MIDI inside the RIFX wrapper today, **Open:** store plain little-endian
  headers) and instrument banks in one host-order form, samples in AUDIO.
* **Open:** compressed audio (ADPCM or Opus) for size; today's mixer reads PCM only.

### 4.5 Worlds and streaming

* Quadrant packs stay separate PACK/GPU sections per quadrant, so streaming keeps loading one
  quadrant at a time; the section table lets a quadrant's GPU data upload on its own.
* Collision: host-order structs as they are. **Open:** precomputed acceleration data.

## 5. Loading in the port

* `dvd.c`: `s_formats` gains `PORT_DATA_XDL` (`.xdl`, the cache folder), tried first.
* `Port_Data.c`: the XDL steps do nothing; the graph is final.
* `Render_Geometry_.c`: an XDL level draws from the pack's uploaded buffers (base vertex, first
  index); the CPU path in `gx_mesh.c` remains for any data not in buffers.
* `gx_tex.c`: XDL texture formats upload as block data; no decode.
* Fallback: if a cache file is missing or stale, the port loads the source as today (and can
  compile it in the background).

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

`hwvx-x compile <source folder or disc image> <cache folder>` (the editor), or the port itself on
first run:

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

## 8. Mods

* A mods folder checked before the cache and the source, per file and later per pack id.
* Mods are sources (glTF, PNG, JSON menu patches, pack folders from `hwvx-x extract`) that the
  compiler turns into `.xdl` on the player's machine, so no game data is redistributed.
* Room past retail limits: more cars than 33 (`pc_exe.md`), bigger TOC caches, widescreen UI.

## 9. Open questions

* Compression: zstd per section costs load CPU but saves disk. Data is ~1 GB per release
  uncompressed; worth measuring before deciding.
* Memory mapping vs. reading into the game heap (handle registry support).
* Half-float uvs; BC7 for higher-quality UI textures; ASTC/ETC2 targets.
* Whether to keep a release's original arrays at all (the port's converted geometry currently
  leaves the GameCube's arrays and display lists unused in the pack).
* PS2: geometry (VIF packets) and textures (GS formats) still need research before they can
  compile to this.
* Version policy: bump `version` on any layout change and rebuild; no in-place migration.
