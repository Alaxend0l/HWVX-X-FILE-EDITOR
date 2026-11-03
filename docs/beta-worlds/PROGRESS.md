# Porting a PS2-prototype world level to the GameCube build — progress log

Goal: get the prototype's version of a world level into the shipped GameCube
game so the retail engine streams the prototype's art and collision.
First target: **CH1_L2** (chapter 1, level 2 — the first race you play).
Test bed: `HWVX-SDL` (build, then run with `--data HWVX-SDL/orig/files`).

The decision that shapes everything: **replace content, keep the host**. The
GameCube `ch1_l2.xgc` world pack (splines, roads, props, triggers, missions,
level logic, its own 193 textures) stays untouched except for 21 × 8 bytes of
quadrant table; everything the loader *streams* — quadrant geometry, the world
texture pack, the collision pack — is rebuilt from the prototype. That keeps
all the gameplay logic (which is data in the world pack) working and confines
the format work to the three content pack types that were already converted.

## 1. What the prototype has (2026-11-03)

53 world archives in the prototype (`WORLDS_/SP_/…`, `WORLDS_/MP_/…`) against
84 in the GameCube release — every prototype level has a GameCube archive of
the same name, the release just has 31 more (chapter 3, extra challenge and
drag levels). Chosen first: `SP_/CHAL1_/CH1_L2.XPS` (v179, 24 packs):

| Packs | Type | Notes |
|-------|------|-------|
| `00_collision_0` | collision (5) | one `TerrainGrid` |
| `01…21_geometry_0_qNNNN_g0` | geometry (7) | 21 quadrant packs, quad ids `906` + `5780..5799` |
| `22_world_texture_0` | world_texture (6) | 153 textures + 45 palettes |
| `23_world_0` | world (4) | retail-shape world data (v179 records are 0x40, GC 0x50) |

GameCube side: `worlds_/geo_/w1_geo.xgc` (21 quad packs, quad ids `0xffff` +
`1..20`), `worlds_/col_/w1_col.xgc` (1), `worlds_/tex_/w1_tex.xgc` (153
textures, no palettes), `worlds_/sp_/chal1_/ch1_l2.xgc` (the host).

Quad ids are useless for pairing (906/5780… vs 65535/1…), so pairing is by
**slot order** — see §3.

## 2. Geometry (2026-11-03)

The car work already produced a validated PS2-prototype → GameCube model
converter (`convert::geometry`, `read_beta` + `build_gc`, checked against 437
models present in both builds), so each prototype quad pack converts with one
call and an **identity texture remap**: prototype texture relocations index the
world's own texture pack, and that pack is rebuilt from the same textures in
the same order, so index *i* still means the same artwork.

What was new is the **quad pack container** (`join_quad`/`split` layout, not
the ordinary pack layout):

```
0x20 header (word 0 = PTR slot into the geometry table)
geometry table, stride 0xC: [geometry id, 0, ref-reloc slot for the model]
padding to 32
models, grafted 32-byte aligned
```

Two things cost time:

* the table area must be reserved (`data.resize(align(0x20 + 12·n, 32))`)
  **before** any model is grafted, otherwise the first graft lands on the table
  and writes over it;
* the third table word is a `ref_relocs` slot, not a size: `put_u32(relOff)`
  plus `Reloc { slot: entry + 8, value: model index }`.

Beta quad packs carry **no shader relocations** (`shader_relocs` is empty in
every one of the 21), so the rebuilt packs don't either — vertex formats come
from the model headers, like the retail packs that stream world content.

Geometry ids: the donor's own table ids are reused positionally (so world-pack
records that name a geometry still find something at that slot); models beyond
the donor count get fresh ids above the largest id anywhere in `w1_geo`
(`0x2d4c`), because ids must be unique across the whole archive. Prototype
model ids (`0x141d..0x168c`) never collide with GameCube ones, but reusing
them would collide with other worlds' ids, so they are not used.

Model counts per slot, GameCube donor → converted (21 slots):

```
GC   17  7 17 19 49 15 37 63 26  9  4 61 24 31  8 80 40 36 54 23 32
new  17 13 15 26 41 15 38 50 33  9  4 78 14 20  8 92 49 23 44 12 23
```

The prototype's quadrants are visibly different pieces of level: slot 15 grew
80 → 92 models, slot 12 shrank 24 → 14, slot 11 61 → 78.

## 3. The streaming contract (this is the important one)

`HWVX-SDL/src/game/FileStreaming.c:538-552` — the loader looks a quad pack up
in the geo archive's table of contents and accepts an entry when:

```c
type == 7 (geometry) && world->id == entry's archive world
                     && entry id == quad->packId
                     && (entry id >> 16) == quad->numGeoms      /* record +0x0C */
```

so **the model count is part of the pack's identity**. The GameCube `World`
object holds `numQuads` (0x15) at `world_data + 0x20`, a pointer to the record
table at `+0x24` (0x400), records 0x50 apart, `numGeoms` at `+0x0C` and
`packId` at `+0x38`, and `packId = (numGeoms << 16) | quadId`.

`hwvx-x pack` writes the archive TOC from the extract's `manifest.json`, so
`port-world` updates all three copies of that number together: the world
record, the geometry header, and the manifest `sub_id` of the quad pack.

Gotcha found while testing: `patch_world` validates the low half of `packId`
(@ `+0x38`) against the record's word 0. It must not — they differ. GameCube
slot 0's record word 0 is `0x38a`, while its `packId` low half is `0xffff`;
the quad id used for streaming is the one in `packId`.

Slot pairing proof (`tests/orig_worlds.rs`, added before the converter): GC and
PS2-retail (v183) quadrant records are word-identical per slot, and the v179
prototype's table sits at the same offset with the same slot order (0x40
records). Per slot, the model count parsed out of the quad pack equals that
slot's `numGeoms` in **all three** builds — 21/21 slots each. Combined with the
GC↔retail rebuild matching structurally (LOD/group structure, triangle counts
within 2 %), that fixes slot *i* as the same corner of the map in all three.

## 4. World texture pack

`w1_tex` is one pack with a shader table at the base of `data`, texture header
blocks, pixel data, and a small tail. Both builds hold **153** textures — so
the identity remap is safe on both sides.

* GameCube: `texture_data_offset = 0x20` (a 32-byte shader table), then 153
  blocks of 0x20 (BTF header: little-endian u16 format/width/height/mips even
  on GC, then a big-endian pixel pointer), 32-aligned CMPR/RGBA8 pixels, then a
  308-byte tail at `0x1b2120` holding the two shader entries that the pack's 7
  texture relocations (values `0x75..0x7b`) point through.
* Prototype: 0x10-stride headers, `+0xC` = palette index, 45 palettes at
  `palette_offset`, linear 8-bit index pixels.

Rebuild: donor shader table + fresh 153 × 0x20 blocks pointing at pixels
re-encoded from the prototype art (same `choose_format` / CMPR / `gc_tone`
path as cars) + donor tail, 32-aligned, with a single rebase delta applied to
the head pointer slots, the tail's internal pointer slots and values, and the 7
texture relocations. No palette support (GC has none); no texture-index
patching anywhere in the world host, because the counts match.

## 5. Collision pack (2026-11-03, earlier commit)

`convert::collision`. The prototype's layout is identical to the GameCube's
(verified: PS2-retail and GC `w1_col` differ only by byte order), so the object
graph converts by swapping fields with the exact claim map from `Port_Swap.c`
(`Claim_Collision`, `Claim_CollisionBSP`, `Claim_Surface`, `Claim_Sound`).
What actually changes:

* the prototype keeps its own 8 terrain textures and 9 ground sounds inside the
  pack (reloc values pack-local); the retail GameCube collision takes its art
  from the shared pack and has 1 sound. Keeping the prototype's inline art is
  what `DataPack_RefreshTextures` / `RefreshPtrs` expect for pack-local reloc
  values, so textures are re-encoded CMPR/RGBA8 (GC tone) and sounds decoded
  from VAG and re-stored as big-endian PCM16;
* the prototype's indexed textures need palettes and GC stores none, and the
  surface skid-texture ids are all zero, so the prototype's texture headers,
  pixels and palettes are dropped and the re-encoded textures are appended as
  fresh GameCube 0x20-block textures;
* sound records use the prototype's voice categories → remapped with the same
  table as the car converter (10 records fixed);
* the pack's `refs` (`HW_share`) come from the donor.

Converted: 1729 cells, 37532 BSP nodes, 6375 edges, 1 material, 8 inline
textures, 9 sounds (sound region 20 388 B → 233 120 B, PCM16).

## 6. World host patch

21 records × 8 bytes in `ch1_l2`: `+0x0C` = converted model count,
`+0x38` = `(count << 16) | low half of the old packId` (so the streaming quad
ids `0xffff`, `1..20` survive). Archive version stays 183 / `0xB7` everywhere;
the world archive is byte-identical in size (7 783 428 B) because only in-place
words changed.

## 7. CLI

`hwvx-x port-world <beta extract> --geo … --col … --tex … --world …`
(`b3a418c`): reads all four prototype inputs out of the single beta extract,
rewrites the four GameCube extracts in place, keeps a `<name>.bak` copy of each
original pack folder, and rewrites the manifests (including the new `sub_id`s).
`pack --force` then rebuilds the archives; `pack` has no built-in backup, so
the recipe copies `.xgc` → `.xgc.bak` first.

Bug found while running the recipe for real (`2a516a5`): the prototype platform
was read with `pack_platform()`, which looks for a `manifest.json` in the
*parent* of a pack folder — that fails on an extract root, which is what
`port-world` is given. Now it reads the extract's own manifest.

## 8. End-to-end run and boot check (2026-11-03)

Recipe from `README.md`, run from fresh extracts of the four untouched
archives. Result:

| Archive | Retail | Ported |
|---------|--------|--------|
| `w1_geo.xgc` | 3 491 376 | 2 968 936 |
| `w1_col.xgc` | 1 381 944 | 1 843 804 |
| `w1_tex.xgc` | 1 779 020 | 1 924 320 |
| `ch1_l2.xgc` | 7 783 428 | 7 783 428 |

`hwvx-x info` on the new `w1_geo` shows the new counts and pack ids
(`01_geometry_0_q1_g13`, …), i.e. the TOC `sub_id`s agree with the counts, as
the loader requires. `cargo test --release` is green: 27 unit tests,
`orig_roundtrip`, `orig_worlds`, `orig_world_port` (convert → join → split →
parse every model → patch the table, against the original archives).

`./HWVX-SDL/build/hwvx --data HWVX-SDL/orig/files` boots through the movies,
the menus and car select with no errors in the log. **Entering the CH1_L2 race
is interactive, so the level itself has not been looked at yet** — the geometry
has to be eyeballed for scale, textures and culling before this counts as
verified.

## Status

* Done: collision converter, quad/texture/world converter + end-to-end test,
  `port-world` CLI, archives installed in `HWVX-SDL/orig/files/worlds_/`
  (retail twins kept as `.xgc.bak`).
* Open:
  - in-game check of CH1_L2 (visuals, collision feel, streaming hitches);
  - 31 more shared levels in both builds — the converter takes any of them,
    but each level's quadrant count, texture count and shared references need
    checking (`w2_tex` has a different shader table; drag/mission/joyride/battle
    archives pair with `wN_geo` differently);
  - world-pack content of the prototype (its own props, splines, triggers,
    textures) is *not* ported on purpose — only streamed content;
  - collision sounds are PCM16 (≈8× VAG); the collision archive is 34 % bigger
    than retail, which is fine in the port and untested on hardware.
