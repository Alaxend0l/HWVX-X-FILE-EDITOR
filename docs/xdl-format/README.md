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
  index); the CPU path in `gx_mesh.c` remains for any data not in buffers.
* `gx_tex.c`: XDL texture formats upload as block data; no decode.
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
| GameCube memory card (Dolphin `.gci` exports; raw card images still to add) | `GHWE78` file `hwvx`: one 0x2000 block after the 0x40-byte `.gci` header; name and comment, banner and icons (0x800), then at +0x840 the save in the same layout as the PS2's, **big-endian**, checksum verified | **decoded** |
| PC saves (`My Documents\VelocityX Saved Games\velocityX`) | one file, 8 slots of the GameCube's `HotWheelsGameData` in little-endian after a 0x194-byte header and settings block, checksum verified ([`../pc-version/README.md`](../pc-version/README.md) "Save files") | **decoded** |
| PS2 memory card (`.ps2` card images) | the card's file system (`ps2_mcd.py`), then `BASLUS-20412/BASLUS-20412`: the GameCube's save layout in little-endian, checksum verified ([`../ps2-version/README.md`](../ps2-version/README.md) §5.5) | **decoded**; `.psu`/`.max` exports still to add |

All three releases store the same file: `SAVE_FILE_MAGIC` 4, size, the game's checksum (sum of
byte + 1), `GeneralGameSettings` (0x150; the PC's 0x38 bytes longer), then 8 × 0xF4
`HotWheelsGameData` slots. GameCube big-endian, PC and PS2 little-endian. Saves made after the same
progress (Challenge 1) on all three hold the same tracks, items, extras and script values; the
cars differ only by the releases' defaults (GameCube 3–5, its 0–2 coming from the GBA link; PC
and PS2 0–5). An import keeps the union, so a GameCube player importing a PC save gains cars 0–2.

Retail ids are the same on every release (§8.1), so each save maps through its release's numbers
to the same `base://` ids. Several saves **merge as a union** (an unlock in any counts; for counters
and best results the highest) with a preview and the choice to import one save only. The
original files are never changed. An export back to a release's own format remains possible
where the content is retail-only.

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
* Extraction uses the same per-release readers as the compiler, so a PS2 pack extracts to the
  same project shape as its GameCube and PC counterparts; `--release` picks one when the player
  has several, or a mixed extraction applies the rule table of §9.

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
