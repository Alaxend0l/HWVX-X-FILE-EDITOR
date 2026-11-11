# The port's data format: one layout for every release

HWVX-SDL loads the GameCube (`.xgc`), PC (`.xpc`) and, later, PS2 (`.xps`) releases' data packs,
and brings each into **one in-memory layout** as it loads: little-endian (host order), the PC's
indexed geometry, host-order sound samples. This file specifies that layout and the exact rules
that produce it from each release, so that a future offline **`.x*` compiler** can do the same
conversion once, ahead of time, and write packs the port (or a tool) reads without converting.

* The SDL code is the reference implementation: `src/game/Port_Data.c` (per-release load steps),
  `src/game/Port_Geometry.c` (GameCube geometry), `src/game/Render_Geometry_.c` and
  `src/port/gx_mesh.c` (the indexed draw), `src/port/gx_tex.c` (textures), `src/port/ax.c`
  (the mixer). Branch `pc-data`.
* `scripts/geom_check.py` applies the geometry rules to a whole extracted tree, without game code.
* Release formats in detail: [`../beta-cars/research/pc_formats.md`](../beta-cars/research/pc_formats.md)
  (PC car packs), [`../pc-version/README.md`](../pc-version/README.md) (PC data and code), the
  [root README](../../README.md#format-notes) (archives and packs).

## 1. What the releases share

All three use BrainCore's pack format, version 0xB7, with the same file names (`HW_Cars`,
`worlds_/geo_/w1_geo`, …). The PS2 keeps them under `XPS/ENGLISH/`.

* **Archive:** a table of contents (`0x020100A0, 0x040100AE, 0xB7, count`, then entries), packs
  at sector-aligned offsets.
* **Pack:** a header (counts, sizes, offsets, two pack references), then the relocation tables
  (texture ids, GX objects, sounds), the data blob, texture-pointer relocations and the pointer
  list. Pointer slots in the blob hold blob offsets until the loader adds the blob's address.
  The pack-level alignment is the same on GC and PC (tables padded to 32; sound region and blob
  64- and 32-aligned). Only objects *inside* the blob differ (GC 32, PC 16).
* **Object graph:** the same structs on all releases (CarAInfo, things, triggers, interfaces,
  worlds, collision). GC is big-endian, PC and PS2 little-endian.

Byte order is the only difference for most of the data. The rest of this file is about what
differs beyond it: geometry, textures, sounds and music.

## 2. The layout the port uses

### 2.1 Byte order

Little-endian, host order, for the tables, the object graph and sound samples. A GameCube pack
is byte-swapped as it loads (`Port_Swap.c`): pointer slots and the known field types first, then
every other aligned word as a u32. That swapper is heuristic (it guesses field sizes from the
object graph) and is the part an offline compiler would most improve: run once and checked, its
output can be verified instead of trusted.

Exceptions the port still keeps big-endian today (a compiler could normalise them, with the
matching readers changed):

| what | why |
|---|---|
| GameCube texture pixels (CMPR, RGBA8, …) | `gx_tex.c` decodes the GX formats as the hardware stores them |
| texture headers' u16s, in memory byte-reversed | `LoadBTF_RAM` reads them with `__sthbrx`, as the GameCube code did |
| song (`RIFX` 'MIDI') and bank headers | `MIDI.c` swaps its own copies (`PortSwap_MIDI`) |
| GameCube display lists | no longer drawn (geometry is converted, §2.2) |

### 2.2 Geometry: the indexed layout

The PC release's layout. A geometry and its levels keep their GameCube/PC headers; what a level
points to changes.

**Geometry header** (0x40): `u16 magic` (GC `0x2003`, PC `0x0002`, never read by the game),
`u16 numLevels` (≤ 8), `u16 skinned` (nonzero: drawn by `Gfx_Anim.c` with a matrix per bone, not
by `Render_Geometry`), `u16 0`, `ptr levels` (0x18 each; the PC puts them at header + 0x40),
`ptr f32 lodDistances[numLevels]`, bounding box and centre (0x10–0x3F).

**Level** (0x18):

| off | type | indexed layout |
|---|---|---|
| 0x00 | u16 | **layout**: 0 in pack data; the port marks indexed levels `0x5043` (`PORT_LAYOUT_INDEXED`) |
| 0x02 | u16 | number of groups |
| 0x08 | ptr | groups (0x20 each) |
| 0x0C | ptr | **vertex buffer** (below) |
| 0x10 | ptr | skinned: **per-bone vertex counts**, u32[8] (the PC's tables are 4 counts and 4 zero words) |
| 0x14 | ptr | texture-name list (never read at runtime) |

**Group** (0x20): `u16 0, u16 flags, u8 colour[4], u32 0, ptr uvScroll, u32 triangleCount (0x10),
u16 blendMode, u16 sortKey, u16 textureOverride (0x18), u16 texture (0x1A), ptr indices (0x1C)`.

* Flags: `0x4000` shadow volume (left as it is, §2.6), `0x81` alpha, `0x04` lit, `0x02`
  env-mapped, `0x10` env overlay, `0x800/0x1000/0x2000` which arrays the level has.
* **Indices:** `u16[3 * triangleCount]`, a triangle list, counter-clockwise. If the first index
  has bit 15 set the list is strip records instead: `{u16 count (bit 15 on the first only),
  count indices}` up to `0xFFFF`. No group of either release uses strips, so a compiler should
  write lists and keep every level under 0x8000 vertices.
* **Texture:** the id sits at **0x1A**; 0x18 is the override `ShaderHolder` writes for animated
  textures (0 = none). The PC's relocations store the id as a u32 at 0x18 (its draw code binds the
  low half), so a PC pack's group texture relocations are flagged to store at 0x1A
  (`PORT_TEXRELOC_U16`). A compiler should write GC's arrangement.

**Vertex buffer** (16-aligned): `ptr positions, ptr colours (or 0), ptr uvs, ptr normals (or 0)`,
16 zero bytes, then the arrays, each 16-aligned:

| array | stride | content |
|---|---|---|
| positions | 16 | f32 x, y, z, pad (the PC's pad is `0xCDCDCDCD`; the port writes 0) |
| colours | 4 | u8 R, G, B, A |
| uvs | 8 | f32 s, t |
| normals | 16 | f32 x, y, z, pad |

A vertex is the tuple of all present attributes; every group of a level indexes the same arrays.
A **skinned** level's vertices are sorted by bone; bone *k*'s matrix is GX position matrix *3k*
(`Gfx_Anim.c` `matrixlookup`), i.e. shader slot *k*, and its vertices are the next
`counts[k]` of the array.

How a group's vertices are drawn depends on its kind, as the GameCube's vertex setups had it:

| kind (`Render_Geometry_.c`) | reads |
|---|---|
| opaque unlit | position, colour RGB (alpha 255), uv |
| alpha | position, colour RGBA, uv |
| lit (`flags & 4`) | position, normal, uv; skinned: its bone's matrix |
| env-mapped (`EnvMap`) | position, normal, colour RGBA (uv generated from the normal); skinned: its bone's matrix |

### 2.3 GameCube geometry → indexed (`Port_Geometry.c`, `geom_check.py`)

A GameCube level's vertex buffer is `u32 offsets {uv, colours or 0, normals or 0, end}` then
packed arrays from +0x10 (positions and normals 12 bytes, uvs 8, colours 4; each array runs to
the next one or the end). Its groups point to **GX display lists** (0x10: byte size, 0x1C: list).

1. **Find the geometry** by scanning the blob for headers: u16 `0x2003` with pointer slots at
   0x8 and 0xC. Many geometries are reached through offsets the game relocates itself (thing
   records, car parts, motion parts, a car's shadow model), not the pointer list, so following
   pointers misses most of them. (PC: `0x0002`, and the levels must sit at header + 0x40; other
   little-endian structs start with u16 2 and pointers at 0x8/0xC too.)
2. **Skip** shadow levels (first group flagged `0x4000`).
3. **Walk each group's list**: commands `0x90` triangles, `0x98` strip, `0xA0` fan (low 3 bits:
   vertex format), `u16 count` (big-endian), then the vertices; `0x00` is padding. A vertex is
   three big-endian u16 indices **A, B, C**. In **skinned** geometry, the vertices of lit and
   env-mapped non-alpha groups (the kinds drawn with `GX_VA_PNMTXIDX` in their setup) start with
   a **u8 matrix index**; bone = index / 3.
4. **What A, B, C are**, by group:
   * lit or env-mapped (`flags & 0x16`, not `0x81`): position A, normal B, uv C, colour white;
   * any other group: position A, colour B, uv C (no normal).

   These are the PC exporter's rules ([`pc_formats.md`](../beta-cars/research/pc_formats.md)
   §1.5, §1.9); they reproduce the GameCube's own rendering exactly (below).
5. **One vertex per distinct (kind, bone, A, B, C)** of the level, in first-use order; strips
   and fans become triangles with the winding kept (strip: odd triangles swap their first two).
6. **Skinned:** sort the vertices by bone (stable), renumber the lists, write the eight counts.
7. **Limits:** a command other than the three, an index past its array, a bone ≥ 8 or more than
   0x7FFF vertices leave a level unconvertible. None does.

**Validated:**

* `geom_check.py` over the whole GameCube release: 512 packs, 21652 geometries (120 skinned),
  **22855 drawn levels, all convertible** (121 skinned, bones 0–7), 33 shadow levels,
  1.63 million triangles, at most 6593 vertices in a level. With `--pc` over the PC release:
  20520 geometries (106 skinned), 21816 levels, all in the indexed layout already, no strips.
* HWVX-SDL with `HWVX_DETERMINISTIC=1` through the title's demo: every dumped frame, the 3D race
  included and with skinned geometry in view (`HWVX_SEED=3`), **byte-identical** to the
  GameCube's display-list path, with equal draw and vertex counts. The PC release's demo shows
  the same frames' content with its own data.

### 2.4 Textures

| | GameCube | PC | port |
|---|---|---|---|
| header | 0x10, u16 format/width/height/mips **little-endian** in the BE file, padded to **0x20** | 0x10, LE, **packed** | the pack's stride by its release (`DataPack.portFormat`) |
| formats | 4 CMPR, 1 RGBA8, 0/2 RGB565 (GX, tiled, BE) | 4 DXT1 (drawn as RGB), 5 DXT5, 0 RGB888, 1 RGBA8888 | PC formats become `PORT_TEX_PC | format` (0x100+), decoded by `gx_tex.c` |
| mips | stop at 16×16 | full chain to 1×1, levels back to back | as stored |
| colour | the PC's squared: `gc = pc² / 255` (RGB, not alpha) | | a setting: GameCube colours (square at decode, default) or the PC's |

A compiler targeting the GPU directly would store **BC1/BC3** (DXT1/DXT5) for both: GameCube CMPR
is DXT1 with GX's block order and big-endian colours, so it re-tiles losslessly
(`pctex.gc_cmpr_to_dxt1` in `../beta-cars/research/scripts/pc/`), at the cost of keeping the
GameCube's darker colours unless converted with `pc = sqrt(255 · gc)`.

### 2.5 Sounds and music

* **Samples are host order** (`ax.c` reads plain `s16`). GameCube PCM16 sound-effect samples and
  song-bank samples are swapped once as their pack loads; the PC's are already LE.
* **SFX descriptors** (0x20): `ptr data, u32 length in bytes, u32 loop (bytes or ~0), u32 rate,
  u8 0, u8 bits (16), u16 volume`, 12 zero bytes. GameCube: big-endian fields; PC: LE.
* **Songs**: `RIFX` 'MIDI' on every release (big-endian container around a standard MIDI file).
* **Instrument banks**: GameCube `RIFX` 'sndb' (BE), PC `RIFF` 'sndb' (LE; unused header words
  `0xCCCCCCCC`). Header: size 0x4, version 0xC, count 0x10, table size 0x14 (samples follow the
  table), 0x18/0x1C unused (GC 1 and 0). Per instrument (0x20): u32 sample offset, length, loop,
  rate; u8 0x10, bits 0x11, u16 0x12; program 0x14; key range 0x16/0x17; u16 envelope
  0x18–0x1E. The port gives a PC bank the GameCube's header form in place (`MIDI.c` reads that)
  and leaves its samples LE. A compiler would write one form; the readers decide which.
* No IT modules (`IMPM`) ship in any release.

### 2.6 Left as they are

* **Shadow volumes** (a car's shadow model, groups flagged `0x4000`): packed f32[3] positions
  and 0x20-byte edges `{f32 n0[3], u16 v0, u16 v1, f32 n1[3], s32 0}`, read directly by
  `Render_DrawShadows`. The PC adds boundary edges (`n1 = 0`) the GameCube lacks.
* **Collision, worlds, interfaces, scripts**: the same structs, swapped by byte order only.
  (PC collision packs have not been compared field by field.)
* `VertexColorSeqManager` registers a quadrant's vertex-colour sequences, but nothing reads that
  list, so it does not constrain the vertex layout.

## 3. Release content is not interchangeable

Converting the format does not make the releases' *content* the same. A compiled pack must
record which release it came from, and the port checks where it matters:

* PC interfaces: PC wording ("Press Enter", key hints), one car-select header per pack (GC: two
  in if6/if7), a keyboard-remap screen, no Multi-Player row, "Exit to Desktop", no Sound Mode
  row in Game Settings. The port's menu patches (`Port_Menu.c`) recognise screens by their
  content.
* PC game logic differences are in [`../pc-version/README.md`](../pc-version/README.md).

## 4. Towards a compiler

What a `.x*` compiler would add on top of these rules:

* **Run on the player's machine only.** The output derives from their copy of the game; it is a
  cache, never distributed. Mods ship as changes or source assets.
* **One target format**: the layout above (little-endian, indexed geometry, host-order samples),
  with the remaining big-endian leftovers (§2.1) normalised and textures as BC1/BC3.
* **A small extra header**: source release, source file hash, converter version, so a newer
  converter rebuilds the cache.
* **The byte-swapper's work done once and checked**: today's heuristic field typing
  (`Port_Swap.c`) is the main source of wrong values in GameCube data; a compiler can verify its
  output (for example against the PC release, whose object graph is the same structs in LE).
* **Reading it in the port**: a fourth `PORT_DATA_*` format whose load steps do nothing, the
  levels already marked indexed and the texture/sample/bank forms already final.
* **Mod overlays**: a mods folder checked before the base data, per pack (and later per asset).
* **PS2**: one more converter into the same layout. Its TOC header is the PC's; its geometry
  (VU/VIF packets) and textures (GS formats) still need research.

## 5. Checking a conversion

* `python3 scripts/geom_check.py <extracted GC tree> [--verbose]`, `--pc <extracted PC tree>`:
  every geometry, what converts, what wouldn't and why.
* HWVX-SDL: `HWVX_DETERMINISTIC=1` (fixed seed and frame step) with **no input** runs the title
  into its demo, a 3D race; `HWVX_DUMP=dir` saves frames; compare two builds' frames byte for
  byte. `HWVX_SEED=n` picks another demo (3 has skinned geometry). `HWVX_GEOMLOG=1` logs the
  geometry conversion per pack; `HWVX_AUDIODUMP=file` the mixed audio.
