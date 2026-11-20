# Custom worlds: a new world from a modder's model

Plan, 2026-11-20. Goal: a modder makes a world from their own geometry (Blender, exported as
glTF), levels of every mode can be based on it, and it ships in a `.xmod` with no game data.
Today a mod track can only point at a retail (or prototype) world (`schemas/Mod.schema.json`
141-183, `src/game/Port_Mods.c` 1028-1072). Paths below are in HWVX-SDL unless they start with
`docs/` or `src/convert` (the superproject's `hwvx-x` crate).

## 1. What a world is in the data

A level loads one **world pack** (type 4, in the level's archive, e.g. `worlds_\sp_\joyride_\world1`)
and, by the names it holds, three set-wide packs (`docs/worlds/README.md`, "Packs"):

| pack | where | holds | loaded |
|---|---|---|---|
| world pack (type 4) | level archive | `World` (`src/game/include/World.h` 534-571): quadrant table (`WorldQuad` 0x50, 400-428) with each quadrant's region (BSPN0), neighbour ids and contents (instances, effect points, pickups); tile cars (starts), tile paths, speakers, teleporters, tile lights, creatures, variables, timers, objectives, lights (vertex colour sequences), volumes and their grid (fog, reverb, force, creature), nav graph (0x78), trigger scripts, `WorldGameData` (431-488: fog, far plane, reverb, radar map, chase cameras, item pack name, pause interface, impact effects) | whole, `State_ProgressionManager.c` 405 |
| world textures (type 6, `w1_tex`) | `World.quadPackName` 0x84 | every texture and the shader list the quadrant packs index | whole, 422 |
| collision (type 5, `w1_col`) | `World.collisionPackName` 0x7C | one grid of cells, each a terrain BSP (mode 1), crease edges, surface materials (pointer table into a tail of spark/skid effects, sounds, textures); version 0xD | whole, 438-448 |
| quadrant packs (type 7) | archive `World.name` 0x8C (`w1_geo`) | one per quadrant: render geometry only, texture/shader relocations into the world textures | streamed, `FileStreaming.c` 532-571 (TOC: id, `numGeoms << 16`, version 0xB7) |

* **Sky** is hub geometry (quadrant 0, NULL region, always loaded; backdrop groups flag 0x8000).
  Battle arenas are a hub plus at most one regioned quadrant (`docs/worlds/README.md`).
* **Fog, far plane, reverb, music scale**: `WorldGameData`; local fog/reverb are volumes.
* **Lights**: none at run time for the world; retail bakes light into vertex colours/textures.
  `World.lights` drive vertex colour sequences (`VertexColorSeqManager`, 64 entries, no check).
* **AI lanes**: the nav graph (`NavHolder.c` 75-86: `numNodes` 0 is fine, a NULL graph is not);
  moving paths (`TilePath`) for scripted movers.
* **Start points**: tile cars (`TileCar`, port 1-4 players, 0 AI). **Items**: per-quadrant
  `QuadItem` lists plus the item pack named in `WorldGameData.itemPack`.
* **Triggers** and objectives: world pack records, read by the progression script's level.
* **Music**: started by the level's progression entry and trigger actions; streamed overrides
  per mod (§8.2.2). **Sounds**: speaker tiles, collision materials' ground sounds.

### What the tools read and write today

| piece | read | write | where |
|---|---|---|---|
| archives, packs, TOC | yes | yes | `hwvx-x extract` / `pack` (`src/lib.rs`, `src/pack.rs`, `src/archive.rs`) |
| world pack records | yes | yes (graph rebuild) | `xdlrs::graph` (Extractor/Builder), `xdlrs::level_pack`; Rust Builder refuses glTF (`docs/xdl-format/xdlrs.md` 58-59) |
| render geometry | GC, PC, PS2 (glTF export) | GC and PC from an in-memory `Model` | `src/gltf.rs` (export only), `src/convert/geometry.rs` 465 `build_gc`, 758 `build_pc`, 611 `read_gc` |
| world textures | yes | GC (prototype port, donor's shader tail) | `src/convert/texture.rs`, `src/convert/world.rs` 86 |
| quad packs + quadrant table | yes | yes, slot for slot over a donor | `src/convert/world.rs` 252, 325 (`hwvx-x port-world`) |
| region / volume BSPN0 | yes | yes (box, prism, solid-leaf builder) | `tools/xdlrs/src/bsp.rs` 27, 113, 163, 209, 234 |
| terrain BSP (collision) | yes | **Python only** | `tools/xdl_chunk.py` 994 `build_collision`, 1241 `write_collision`; `tools/bsp_build.py` |
| glTF in, chunking, regions, neighbours | | **Python only** | `tools/xdl_chunk.py` 302 (plain glTF), 662 (plan), 1326 (patch world) |
| nav, volume grid, sky, radar map | read (viewer overlays) | no | `src/port/viewer_world.c` |

**Precedents.**
* `tools/xdl_chunk.py` (`docs/worlds/README.md`, "The auto-chunker") already turns a glTF world
  into quadrant packs, collision and a patched world pack; its `arena` (468 triangles of our own)
  was driven as track 57 (ramps, walls, never fell through). But it is Python, GameCube only,
  and its packs hold base data (the world pack, collision materials, hub), so they are built on
  the player's machine and staged into the data folder: they cannot ship.
* **Prototype worlds as tracks** (`docs/xdl-format/README.md` §9.1.2): 48 worlds the retail list
  doesn't have, registered in ids.lock with `"world"` and loaded through a name prefix.
  `Port_ProtoWorldLoaded` (`src/game/Port_Proto.c` 776) rewrites the world pack in memory as it
  loads (records re-laid, pack names renamed to `~...`) before anything reads it. That is the
  model for a custom world: the port, not a build step, adapts the world pack.
* **Levels in memory** (`src/game/Port_Level.c`: 1647 `ApplyAppends`, 1864 `StartOverride`,
  2441 `Port_LevelWorldLoaded`): grows world arrays, moves pointers, undoes on release.
* **`@mod\<id>\<file>` names**: `DVDOpen` (`src/port/dvd.c` 276-298) asks
  `Port_ModsResolveFile` (`Port_Mods.c` 1529) first, so any archive name can come from a mod's
  folder. But a package may not hold archives: `AllowedName` (`Port_Mods.c` 628) allows only
  `.json .jsonc .png .wav .gltf .glb .bin .mid .md .txt .ogg .mp3 .flac`.

## 2. The minimum viable custom world

A **hub-only world over a retail base, composed in memory**:

* The mod ships its own archive (`world.xgc`) with only its own packs: **one hub quadrant pack**
  (render geometry, no region: the battle-arena shape, so no regions, neighbours or streaming),
  and **one collision pack** (grid, terrain BSPs, crease edges; material *indices* only).
* A `world.jsonc` says `extends` a retail level (a Joyride level: no AI, no objective) and gives
  starts, scale, the surface per material, and overrides.
* As the base world pack loads (next to `Port_ProtoWorldLoaded`, `State_ProgressionManager.c`
  405), a new `Port_World.c`:
  * replaces the quadrant table with one hub (`packId` 0xFFFF, empty `QuadContents`);
  * points `World.name` and `collisionPackName` at `@mod\<id>\world`; keeps the base's
    world textures (quadrant geometry uses base texture indices or vertex colour only);
  * takes the collision materials from the base collision pack (loaded too), so surfaces,
    sparks, skid marks and ground sounds are linked, never shipped;
  * moves the port-1 tile car to the given start, drops the others;
  * empties what lives in base coordinates: volumes (NULL holder is safe, `VolumeManager.c` 69),
    teleporters, creatures, speakers, tile paths, triggers not needed by the mode, nav (an
    empty graph, not NULL); radar map off (`mapTexture` 0).
* A track entry (`tracks` in mod.jsonc) whose `world` names the custom world, so
  `HWVX_START=race:mode=joyride,world=uid://ID` starts it.

**What breaks or is missing at this size**

| area | effect | fixed in stage |
|---|---|---|
| AI lanes | no nav: no AI cars, so only Joyride (and solo Mission/Challenge without opponents) | 9 |
| respawn | falling off resets via teleporters / nav: none left; need a reset point at the start | 1 (teleporter moved to the start, as the chunker's arena does) |
| camera | chase cameras are `WorldGameData`'s, fine; base scripted cameras dropped with triggers | - |
| minimap | base map would be wrong: off | 7 |
| streaming | hub-only: everything always drawn; fine on the port, past retail budgets (1.3 MB GC active set) on big worlds | 5 |
| draw distance | far plane and fog are the base's; may clip a big world | 7 |
| shadows | car stencil shadows work on any geometry; no baked world light unless vertex colours | 6 |
| items | no pickups: `QuadItem` lists empty; level `items` appends need a quadrant with a list and a template (`docs/xdl-format/README.md` §10.6.4 "Not yet") | 8 |
| sky | none: the hub has only the mod's geometry; clear colour/fog shows | 7 |
| textures | base palette only | 6 |
| music | depends on base triggers/level entry kept or dropped; to check | 8 |
| fixed tables | `ShaderHolder` 128, vertex colour sequences 64, no bounds checks | 5 (budget checks in the builder) |

## 3. Staged plan (one agent-day each)

The builder is Rust. It belongs in `hwvx-x` (it has the geometry and texture encoders, the
pack and archive writers) as `hwvx-x world ...`, called by xdlrs and the editor as a subprocess
as they already call `hwvx-x` (question 7). `tools/xdl_chunk.py` and `tools/bsp_build.py` are the
reference for the algorithms, ported, not called. Every game test is a deterministic run
(`tools/run_test.sh`, `tools/drive_test.sh`, `HWVX_DUMP` frames, `HWVX_DETERMINISTIC=1`), on GC
data unless said; each stage also checks the attract demo stays frame-identical with no mod.

**Stage 1: a generated ramp, drivable (vertical slice).**
`hwvx-x world demo OUT`: a ground plane, a ramp and a wall made in code (no glTF yet): one hub
quad pack via `build_gc`, one collision pack with a handful of cells (terrain BSP: port
`bsp.rs`'s solid-leaf builder to the 0x20 `TerrainBSPNode` layout, crease edges), written as an
archive with `hwvx-x pack`; a `world.jsonc`. Port: `Port_World.c` overlay as in §2, switched on
by `HWVX_WORLD=path/world.jsonc` + `HWVX_TEST_EXTRA_TRACKS` (as `tools/chunk_drive.sh`), archive
resolved from that folder. Tests: `cargo test` (the collision pack parses with
`src/convert/collision.rs` `walk`; down-rays hit the ground and the ramp top); a drive script
(gas held, 30 s): the car's height rises on the ramp and lands, never below the ground; frames
show the ramp; the log has no "not found".

**Stage 2: glTF in.** Plain glTF/GLB reader (serde_json, buffers, node transforms, triangles,
vertex colours, UVs; meters to game units by a `scale`), one geometry per mesh, a group per
primitive; materials named to base textures (`base://w1_tex/world_texture_0/texture/N`, as the
chunker's `tex_link`) or vertex colour; collision = the render mesh, or meshes named `col*` /
`extras.collision`; weld (1e-3), degenerate triangles dropped, open edges reported. A tiny
hand-written sample in `tools/world-samples/ramp` (no game data). Tests: cargo round trip
(`read_gc` of the built pack equals the input triangles); the sample driven as in stage 1.

**Stage 3: collision at any size.** The chunker's collision in Rust: grid over the bounds
(~250 x 200 x 250 squares), triangles clipped per cell, coplanar merge, BSP per cell (tolerance
1e-4, pre-order), crease edges per cell (box grown by 10), u16 limits, re-split deep cells;
`extras.material` / material names mapped to the base collision pack's surfaces (a table of the
base surfaces' names to find first, question 5). Tests: Rust ports of the `test_xdl_chunk.py`
cases (box on the ground, thin slabs, ray hits); a 3,000 x 3,000 arena driven 100 s
(`tools/autopilot_drive.sh` or a scripted drive): lowest height stays the road.

**Stage 4: shipping it as a mod.** `mod.jsonc` gets `worlds` (`uid` -> `extends` a base level,
`archive`, `world`) and a track's `world` may name `uid://<world>`; `Port_ModsSyncLock` writes
`@mod\<id>\...` names. `AllowedName` and `xdlrs modcheck` accept the mod's archive only after
decoding it: every pack type must be one the builder writes, geometry positions and textures are
hashed against the index (refuse retail data, e.g. a renamed `w1_geo`). Schema regenerated.
Tests: `xdlrs mod pack` of the sample; a renamed retail archive refused; `tools/mod_race_test.sh`
style: install the `.xmod`, `HWVX_START=race:mode=joyride,world=uid://ID`, drive; mod off: the
track is `"active": false`.

**Stage 5: quadrants and streaming.** Port the chunker's plan (strips, budgets: 5,000 triangles,
8,000 vertices, 250 KB per quadrant, 30,000 / 1.2 MB active set), regions as box BSPN0
(`bsp.rs` `box_bsp`), at most 4 neighbours, a hub for far/large meshes; the overlay builds the
quadrant table from `world.jsonc`'s regions. Budget checks (ShaderHolder 128 per active set).
Tests: a large generated world; `HWVX_LOADTIME` shows no synchronous loads on a drive; the
viewer's `regions` and `loaded` overlays; split screen with 2 players.

**Stage 6: own textures and light.** PNGs in the mod (allowed already), encoded at build time
into an own world texture pack, GC (CMPR/RGBA8) and PC (DXT) archives. Open: the shader table
entries (`src/convert/world.rs` takes the donor's two; their layout must be written from
scratch, or the port appends mod textures to the base pack at load instead). Optional baked
vertex lighting from a glTF light. Tests: textured cube frames on GC and PC data; modcheck
refuses a texture equal to a retail one.

**Stage 7: sky, fog, minimap.** Sky: the mod's own hub sky mesh, or the base level's hub by link
(the port loads the hub quad pack from the base archive: a per-quadrant archive). `world.jsonc`
`fog`, `farPlane`, `fogColor`, `reverb` written into `WorldGameData` at load. Radar map: a
top-down render (`src/render.rs`'s software renderer) to a PNG plus `mapScale/Offset/Half`.
Tests: frames at 3 points; the map blip moves with the car.

**Stage 8: levels on a custom world.** Level files `extends` a custom world's track (the
editor's level projects, `docs/xdl-format/README.md` §10.6): starts, items (the builder gives
every quadrant an empty item list; templates by link to a base pickup), effect points, volumes
(an own volume grid: `VolumeHolder`), triggers, objectives, music. Samples: `checkpoint-run` and
`gear-grab` rebased on the sample world. Tests: `tools/level_test.sh` win and lose runs;
`tools/level_modes_test.sh` for Challenge, Mission, Joyride.

**Stage 9: AI lanes, races, battles, drag.** Lanes from glTF curves/empties (or the editor) to
a `NavData` graph (release layout, 0x60 points; prototype conversion in §9.1.2 shows the fields),
respawn points along it, moving paths optional. Tests: AI opponents complete laps in a race
(car logs), a battle arena with bots, a drag race finishes; `autopilot_drive.sh compare` twice:
identical.

**Stage 10: editor, "New world from a model".** In `tools/browser`: File > New world... (next
to New level, `level_edit.rs` 580): pick a glTF, scale and up axis, preview through the viewer,
map materials to textures and surfaces, place the start ("To the ground" via `startcheck`),
see the quadrant plan and budgets (overlays), Test drive (builds with `hwvx-x world`), Save as
mod. Tests: `cargo test` in `tools/browser` (a test like `tests/testdrive.rs`: new world from
the sample, built, driven, saved; reopening the project).

**Stage 11: editor, world editing.** Lanes and respawn points drawn and moved, hub/region
hints, re-import a changed glTF keeping placements, and "New level on this world". Tests: as
stage 10, plus undo/redo.

## 4. Risks and unknowns

* **Biggest: collision from arbitrary modeller meshes.** The terrain BSP is solid-leaf: open or
  non-manifold meshes, overlapping parts and near-coplanar faces give invisible walls, holes and
  floors facing down (the chunker hit all of these: `docs/worlds/README.md`, "What
  reconstructing the retail collision taught"). Needs strong cleaning, a report the editor
  shows, and drive tests on real Blender exports, not only generated shapes.
* **The base world pack under the overlay.** Its triggers, objectives, variables and the
  progression entry assume the base geometry. Which records each mode can lose without a
  crash or a stuck level is unknown; Joyride is the safest base, other modes need checking.
* **Collision materials by link.** Assumed: the mod's cells can index the base pack's material
  table, with the base collision pack resident. Unverified; surface names are not known yet.
* **Mixed encodings.** Assumed: a GC-encoded mod archive loads on PC and PS2 data the way mixed
  builds do (§9); otherwise the builder writes one archive per release.
* **Shader entries** of a world texture pack are only known from donors (stage 6).
* **Budgets.** Fixed arrays (4 neighbours, 11 streamed packs, ShaderHolder 128, 0x7FF texture
  slots) corrupt memory silently when exceeded: the builder must refuse, not warn.
* **Packaging rule.** Allowing archives in `.xmod` widens what a package can hold; the check
  must decode them fully, or a retail pack could ride along.
* Online play and replays on custom worlds: not checked (`Port_Online.c` uses world data).

## 5. Questions for the user

1. Ship compiled archives in the `.xmod` (recommended; needs `AllowedName` and the mod check
   extended), or ship glTF and compile on the player's machine (launcher runs the builder)?
2. Base for "extends": one fixed template per mode (Joyride `world1` for free roam), or the
   modder picks any level?
3. Retail budgets (worlds that would fit a GameCube) or port-only limits?
4. Is base-palette texturing plus vertex colour acceptable for the first release (stages 1-5),
   with own textures later?
5. Which surfaces matter (road, dirt, grass, ice ...)? A list of base collision materials by
   name is a research task for stage 3.
6. Priority after Joyride: missions/challenges (stage 8) before AI races (stage 9)?
7. Builder in `hwvx-x` (has encoders and archives) or move those into xdlrs (the editor's
   crate)?
8. Retire `tools/xdl_chunk.py` once stage 5 matches it on the arena?

## 6. Stages 1 and 2 as built (2026-11-20, branch `custom-worlds`)

**Built.** `hwvx-x world demo OUT [--gltf]`, `world build PROJECT.jsonc OUT [--heightmap]`,
`world inspect MODEL`, `world probe PROJECT X Z` (`src/custom_world/`: `mod.rs` packs and
archive, `terrain.rs` collision, `gltf_in.rs` glTF and project). HWVX-SDL:
`src/game/Port_World.c` (the overlay of §2, as specified, hooked in `State_ProgressionManager.c`
states 0, 2 and 7 and `dvd.c`), `tools/world_drive.sh`, `tools/world-samples/ramp` (the demo as a
plain glTF with its project file). The world's archive is `@world\<archive>` beside
`HWVX_WORLD`'s file (not a mod yet: stage 4). Tests: `cargo test custom_world` (packs read back
with `read_gc` and `walk`, down rays on the ramp, the demo through glTF gives the same archive,
placement and mirroring, `HWVX_TEST_LAVA_CAGE=<project>` for a real level, skipped without it);
drives on GC data; the attract demo frame-identical (1,246 frames).

**Results.** Demo: the car climbs the ramp (y 2 → 76 at z -228 where the slope is at 74.4), flies
to 106, lands, stops at the walls; lowest y 2 over 1,836 log lines. A real modeller's level (a
1.5 km arena from a Godot game, FBX → glTF once with Blender, 9,312 triangles, 20 meshes): 3,506
cells, 2.1 MB collision, built in 0.8 s; 99.84% of a 200 x 200 raster of its surfaces seen from
above match the collision within 0.5; driven 4,000 units over its terrain (y -139 .. 120), no
fall-through.

**Found on the way** (for stages 3 and 10):

* **Units and axes.** The game is right-handed and Y-up like glTF and Godot (steering right
  turns a car facing +z towards -x), forward is (sin h, 0, cos h): a glTF goes in unmirrored.
  Cars are ~9 x 6 x 21 units (Twin Mill: 8.96 x 6.2 x 21.2), so **about 8.5 units a metre**
  for a car-scaled model; a project's `scale` sets it.
* **Grid headroom.** Hits outside a cell's box are dropped, so a grid ending 1 unit above the
  highest surface (the chunker's) lets a car on it lose its ground contact: the ramp's launch
  changed (85 → 106 high) once the grid reached a square's height above it. The builder adds one.
* **Modellers' meshes need a union per cell.** One solid-leaf tree of everything in a cell (the
  chunker's way) breaks where meshes overlap: a tower pushed into the ground left the ground
  beside it empty (below the ground, in front of the tower's buried walls, reads as air), and a
  roof slab through its walls (one mesh, two solids) left the slab empty past the walls. Now
  each connected piece of a mesh node is a solid; in a cell, each piece's polygons are cut along
  the others' planes and the parts **under** another piece (the first of its triangles straight
  above faces up) are dropped. A tree's own inside test (the first try) is no good for that: an
  open mesh's tree is only right near its surfaces. Single-sided decks count as solid below.
* **Polygon order.** An open surface's tree depends on the order (a cliff face picked early cut
  through the plateau above it): each cell tries the source order, walls first, floors first,
  largest first, and keeps the tree that best matches "under a surface" on a 6 x 6 x 6 raster.
  99.03% (union only) → 99.43% (pieces) → 99.84% (order). What is left is at the rim.
* Untextured groups (`texture: null`, vertex colour only) draw fine; the base world's fog and
  far plane are dark and short for a big world (stage 7). The base level's teleporters,
  volumes, speakers, creatures and radar map are moved or dropped as §2 says; the radar map's
  texture slot is written by the refresh, so it is cleared at state 7.

**Collision quality, second pass (branch `worlds-collision`).** Driving the real level showed
poor collision at pillars and at a ramp between them. The surfaces seen from above were already
99.84% right; the trouble was inside the masses, which a top-down check cannot see:

* **A mass whose top is in the square above read as air in the square below** (only its walls
  and some unrelated sheet were in that cell, so nothing said "solid here"); a car pressed into a
  wall at a square's height, or climbing a slope across a square boundary, met air and snagged or
  bounced. Measured on the real level: 2.2% of points inside the cells wrong that way, 28% in a
  ring of pillars. Fix (`terrain::box_caps`): each cell's solid is **closed by the parts of the
  cell's walls that lie inside the solid** (cut along the cell's surface planes, each piece kept
  if "under a surface"), so the tree is exact in any polygon order; the solid squares next to
  surfaced ones get a full box, so a car pushed through a wall meets solid.
* **One-sided sheets.** "Under a surface is solid" (the inside test) filled the room under a
  floating walkway down to the ground, and a sheet's plane ran on past its edges. Fixes: a sheet
  piece (only up-facing triangles, with open edges) with more than 16 units of room under most of
  it is made a **slab 4 thick** (`solidify_decks`: a bottom and walls round its open edges), and
  every open edge of a floor gets a **skirt** down to the grid's bottom (`skirts`), so the solid
  under a floor stops at its footprint. Skirts of two unwelded pieces meeting at a seam face each
  other on one plane and drop out.
* **Self-checks** (`world build` prints them): points on a 7^3 raster of every cell, and a unit
  in front of / behind each face, against "under a surface" — phantom (solid where nothing is)
  and hole (open inside a solid). Synthetic meshes in `cargo test synthetic` (an open-bottomed
  hex pillar, a one-sided ramp between two pillars, unwelded and overlapping seams, a floating
  deck, a ring of pillars under a grate; each repair off/on), plus a mass with a sheet through it.

| real level (8.5 units/m, 9,312 triangles) | phantom / hole in cells | at faces | from above | cells, pack |
|---|---|---|---|---|
| stage 2 builder | 1.87% / 2.18% | 5.45% / 0.97% | 99.84% | 3,504, 2.1 MB |
| + decks | 0.96% / 2.31% | 1.04% / 1.11% | 99.87% | |
| + skirts | 0.02% / 3.05% | 1.04% / 0.92% | 99.89% | |
| + caps (now) | 0.01% / 0.00% | 0.96% / 0.07% | 99.94% | 6,122, 4.2 MB |

(The face phantoms left are on open-topped tubes: inside a tube's wall the inside test sees no
face above and calls it air; the collision is right.) Drives: up the ramp between the pillars
the car used to bounce back off air at y 144 and now climbs onto the plateau and on 3,800 units;
along the pillar row it used to hang in the air at y 161 stuck (wheels spinning) and now meets
the pillars' sides and drives on; no run went below a surface. The ramp demo drives as before.
The collision pack doubled (the capped and the solid squares); the port has no limit, a console
budget would need fewer solid squares.

**Phantom hits, third pass (branch `worlds-edges`).** Rescaled to a third (8.5 → 2.8333 units
a metre) the same level drove badly: the car's nose lifted on smooth ground. The point checks
read clean; the game's own ray did not:

* **Cause: near-coplanar polygons kept as nodes of their own.** The tree builder (the exporter's
  rule) sent a polygon in, or a float hair off, the splitter's plane to the front as a node of
  its own. That node's back leaf is a float-thin solid slab behind the plane, everywhere in the
  cell. `RayHitsBSP2` splits a ray at each plane it crosses and hands the piece ending exactly
  on that plane down the tree; landing a hair behind the twin plane, it reports a hit: a face in
  open air, anywhere along that plane. Gentle surfaces made of many small triangles put many
  near-equal planes in one cell (a smaller model scale, or a finer mesh, packs more into a
  cell): on the rescaled level 44% of all tree nodes repeated an ancestor's plane, at the old
  scale 20%. Fix (`build_tree`): a polygon facing the splitter's way within 0.26° and 0.01 units
  of its plane is dropped (it says nothing the splitter does not). One a hair off facing the
  other way stays (the other side of a thin sheet).
* Found on the way, all general: a mesh that runs through itself (a ledge's drop face going on
  down inside the cliff beyond it, one welded piece) left faces buried in its own solid, through
  which a box corner sunk into the slope left the solid, pushed further in (the car drove into a
  cliff): the union now clips each piece against its own solid too. Skirts only hang where it
  is open past the edge (a T-junction is no open edge). Crease edges whose faces are buried
  are dropped (20% of them). Each cell tries more polygon orders (smallest first, three fixed
  shuffles) scored also half a unit either side of each polygon. Deck pieces in a fixed order.
* Game side (C, custom worlds only, `Port_WorldActive()`): when a ray starts inside the solid,
  `RayHitsBSP2` reports the plane of the last node, often a cell wall closing the solid far off;
  it now reports the nearest plane the point is behind (the face it sank through).
* **New checks** (`world build` prints them): the game's ray (`terrain::game_ray`, the C
  branches in f32) through 20,000 short segments in open space just above the surfaces, and the
  tree nodes repeating an ancestor's plane. In-game: `HWVX_CARLOG=3` logs the car's pitch and
  the terrain contacts (edge or box corner) every frame, `=4` each corner contact with its plane
  and cell; `tools/world_pitch.sh` turns a drive into pitch spikes and contacts per 100 units.
  `hwvx-x world smooth OUT --scale S`: rolling ground and a long hump in 25-unit quads, at any
  scale; `--without decks,skirts,caps,fill,coplanar` on `world build` and `world smooth` to
  measure what each repair buys.

| | game ray hits air | nodes repeating a plane | pitch spikes / 100 units | corner contacts / 100 |
|---|---|---|---|---|
| smooth world ×0.33, before / after | 2.79% / 0.01% | 23,373 / 28 | 2.73 / 0.13 | 2.88 / 0.24 |
| smooth world ×1 | 0.64% / 0% | 18,252 / 33 | 0.91 / 0.02 | 0.34 / 0.08 |
| smooth world ×3 | 0.06% / 0% | 10,013 / 34 | 0 / 0 | 0 / 0 |
| real level, 2.8333 units/m (main's build / now) | 0.23% / 0% | 17,914 / 2 | start 9.14 / 1.74, ramp 5.94 / 0.97 | 2.19 / 0.65, 2.26 / 0.49 |
| real level, 8.5 units/m | 0.04% / 0% | 20,038 / 1 | start 16.4 (stuck) / 0.74, ramp 1.04 / 0.39 | 1.67 / 0.16, 0.43 / 0.45 |

The real level at 2.8333: 18,004 nodes (from 41,090; most 503 a cell, from 1,450), 0.9 MB, 100.00%
of its surfaces from above right. Crease edges were not the cause (edge contacts stayed near 0 on
every drive); the level's low-poly floors still carry ~800 shallow creases (retail: 77-168).
Drives into a cliff at either scale: before, the car ended inside the rock (8.5: stuck at once);
now it climbs or turns away. The ramp demo now launches to y 85, where stage 1's first build did;
the 106 measured since was a phantom kick at the lip.

**A car stuck on a flat plain (branch `position-hud`).** The user's car sank and jammed on a wide
flat plain (y 226.66 at 2.8333 units/m), which lay 0.14 above a row of squares' floor (226.52).
The cell above held the plain and, closing its solid, a cap on its floor facing down: a slab 0.14
thick. A box corner grazing down through the plain made a ray whose piece past the plain left the
slab through that cap, and `RayHitsBSP2` reports where a piece that entered the solid leaves it:
the cap, facing down. The car was pushed down into the plain, back up by the cell below, down
again (`transmissionFlags` 0x10000 is the force-feedback collision flag, set on every hit).
Retail cells are open below, so their trees never close a solid a hair under a surface.

* Builder: each cell's tree now reaches 16 units past its square (`CELL_OVERLAP`: surfaces clipped
  to the grown box, the caps on it): a ray that entered the solid ends inside it, the entry is
  reported, and a hit out there is dropped (the game keeps the square's own box). And the grid is
  shifted, per axis, by up to a square so the fewest flat faces lie within 2 units of a wall
  (`shift_off_faces`).
* Game (custom worlds): a piece of a ray that starts on a parent's plane inside the solid is
  reported as at its start, so the parent reports its own plane (the entry), not the exit.
* Check: `world build` fires grazing rays down through the surfaces from the cell a corner is in
  and counts faces looking away from the ray. The user's world: 0.40% overall, 11% in the stuck
  spot's cell; now 0.01% and 0. Driving the user's approach: the car sank to y 225.6 and jammed
  with 77,948 down-facing contacts; with either fix it crosses at 228.5-228.7 with none.
* Collision of the level grew to 1.2 MB (the overlap); the other checks stay clean (0.00% / 0.00%
  inside cells, 99.98% from above).

**For artists** (any world, no per-world settings): collision is built from the drawn meshes,
each connected piece a solid; closed meshes are best, open-bottomed ones standing in the ground
are fine; a floor with room under it may be one-sided (it becomes a slab); nodes named
`*-colonly` / `*-convcolonly` (Godot's import suffixes) or with `extras.collision: true` are
solid and not drawn, `collisionOnly` / `renderOnly` name patterns in the project do the same.
A separate collision model beside the render one is not read yet.

**Nothing of the base level left (branch `world-base-clean`).** Lava Cage over track 0 still
showed Monument City's train and a spinning tire: the world pack's tile paths (movers) spawn at
the start whatever the quadrants hold. `Port_WorldLoaded` now drops by kind, for any base (no
names): movers (`numTilePaths` 0), AI spawns (tile cars of ports other than 1-4: `dynamic` 1,
`mode` 0, so neither the start, an action nor `AI_CarA`'s near-a-camera spawner brings them),
the nav graph (`numNodes` 0), volumes (`numVolumes` 0 too: fog, reverb, force, light), tile
lights, vertex colour lights, the weather emitter kept ahead of the camera (`cameraFx`),
teleporters' effects; and from the level's TAC triggers (before the refresh activates them)
every trigger whose test or a condition is on the base's places (airboxes, destructibles,
items, movers, creatures) and every action on them (airbox, destructible, fx point, item,
mover, nav point, speaker, vertex colour, script light, teleport, AI path, sound sequence,
creature, the target arrow, a camera sent anywhere but behind the player, a placed item given);
lists of tile cars lose the dropped AI cars. The players' cars start side by side (16 apart)
across the start. Kept: the level's rules (timers, variables, objectives, interfaces, music,
the end of the level, battle rounds), chase cameras, fog, the item pack, surfaces. **Sky**: the
base's sky is in its hub quadrant with its level geometry and is not drawn (black above the
fog): a world brings its own (decision for the user). The log has a `world: from the base level:`
line with the counts per kind and the triggers and actions dropped by type; `HWVX_THINGLOG=N`
logs the things alive by type every N frames (any level). `tools/world_drive.sh` takes `MODE=`.

| base (custom world over it) | things alive before (beyond the car) | after | dropped (examples) |
|---|---|---|---|
| 0, Joyride world1 (ramp, Lava Cage) | 3 movers (the train, the tire) | none | 3 movers, 813 volumes, 20 of 26 triggers, 1 camera action |
| 19, first battle arena | 3 AI cars, weather emitter, debris, a mover-like physical | none | 3 AI spawns, 64 nav points, weather |
| 39, Challenge 1 | 3 movers | none | 3 movers, 285 nav points, 18 of 26 triggers |
| 5, first mission | 3 movers, 2 AI cars | none | 11 AI spawns, 324 nav points, 20 of 41 triggers, 14 actions |
| 29, first drag race | 3 AI cars, debris | none | 3 AI spawns, 94 nav points, 16 of 29 triggers |

All 51 bases (0-26, 29-34, 39-56) swept with the ramp sample: only the player's car and the
camera's looker alive (one global sound sequence at 0,0,0 once on 32); the ramp drives the
same over each (y 85 at the lip, never below 1). Track 10's script gave the car one of the
base's placed items (`ActionVehicleGiveItem`, type 0) and crashed main's build; that action is
dropped. Retail unchanged: the attract demo frame-identical, Joyride world1 and battle bat2_l9
races (carlog and frames) identical to main's. PC and PS2 data: the same drops and drive.
Not covered: a world's own volumes, AI, routes or music (none exist yet); a base's rules that
need what went (a challenge's checkpoints, a drag race's opponent) cannot be completed.

**Still open.** Surfaces are all material 0 (question 5); textures are base indices only
(stage 6); a mod cannot ship the archive yet (stage 4); hub only, no streaming (stage 5).

## 7. A car with its own model (Headline)

Today a mod car is a base car (`extends: base://HW_Cars/car_N`) with merge patches on its
records (`patches`: `CarAInfo/0/accel` ...) and colour recipes on its textures
(`schemas/Mod.schema.json` 55-95, the editor's Content tab). Its geometry, skeleton, wheels and
collider are the base car's. The user's car **Headline** (a Godot car: `cars/headline.tscn`,
`cars/models/headline.fbx`, `cars/headline/materials`, `cars/headlinestats.tres`) is the
example of a car with a model of its own. Not built; what it would take:

**What a car pack's model is** (`docs/beta-cars/research/objects.md` §2, `CarA.h`):

| part | where | Headline has |
|---|---|---|
| geometry | `CarAInfo.modelRef` → `MotionAnim`: `MotionPart`s (geometry + bone), a `Skeleton` of `AnimNode`s (quat + position), `KeyAnim`s (steering 0x48, three additive anims) | 26 objects in one FBX, no skeleton: one bone each for the body, the wheels, and any part that moves |
| breakable parts | `MotionAnim.0x38` `CarPartList`: parts with a damaged geometry and debris (`damagePercent` 75, `damageLevel` 20) | none (Roarcage cars do not shed parts): an empty list or the intact mesh twice |
| wheels | `CarAInfo.wheels` (`CarTrailBone` 0x50 each: offset, widths, `restLength`, `springRate`, `damping`, `friction`, `maxFriction`, `handbrakeGrip`, `driveBlend`, driven/skids/dusts) plus a bone and geometry each | `wheel.tscn` (one model for four), positions FrontLeft (-0.42, 0.239, -0.683) ... BackRight (0.42, 0.239, 0.965) m |
| collider | `CarAInfo.collider` (`CarACollider` 0x4C): type 0 box (min/max) or a hull, mass, `maxHealth`, `damageThreshold`, `inertiaScale`, gravity | `BoxShape3D` 1 x 0.55 x 2 m |
| LODs | each geometry's LOD list (cars: 2-3 levels) | Godot generates LODs at import; the FBX has one |
| shadow | `CarAInfo.0x54` (`reflectionGeom`, drawn under the car in the garage) and the car's stencil shadow volume (an edge list: `build_gc_shadow`) | `shadow_mesh` in the scene; a closed hull is enough |
| textures | GC CMPR / RGBA8 in the car pack, by texture relocations | toon shader with a base colour per material (`car_paint` red 0.69, 0.03, 0.02), a carbon texture |
| the rest | engine (`CarAEngine`: cameras, torque curve, steering), HUD and stat bars (`CarAHud`), accel, stunt, sounds, AI scripts, slaves (weapons, pickups) | kept from the base car the mod extends |

**What importing Headline would take.**

1. Convert once (Blender) to glTF, as the level was; the glTF's nodes name the parts.
2. A car project (`car.jsonc`, like a world project): `extends` a base car (everything not
   below comes from it), `model` (glTF), `scale` (8.5 units a metre puts its 2.5 m on the base
   cars' ~21 units), which node is which wheel (or one wheel mesh and four positions), the body
   nodes (merged into one geometry per bone, a group per material), material → colour or PNG,
   the collider box.
3. `hwvx-x car build`: writes the car's own pack data the way `port-car` writes a converted
   prototype car (`src/convert/car.rs`): geometry with `build_gc` (+ `build_pc` for PC data), a
   skeleton of one root bone plus four wheel bones at the wheel positions (the steering
   `KeyAnim` turns the front two: copy the base car's tracks, retargeted by bone index), the
   collider box, a shadow hull, LODs (the same mesh, or a decimated one), textures encoded from
   PNGs (stage 6's encoder), everything else copied from the base car's records.
4. Shipping: like a world, a mod would carry the built pack (stage 4's checked archives), never
   the base car's geometry or textures.
5. The car select's thumbnail: `hwvx-x render-car` renders it from the built pack (`roster`).

**Godot stats → `CarAInfo`** (`headlinestats.tres`; Roarcage's `rust/sim/src/stats.rs` and
`tuning.rs` say what each is; HWVX's from `docs-pending/car-stats-inventory.md`). The physics
models differ (Roarcage: PhysX-like wheel colliders in m/s; HWVX: rpm and gears), so this is a
starting point to tune by driving, not a conversion:

| Headline | value | HWVX field | how |
|---|---|---|---|
| `max_health` | 500 | `collider.maxHealth` (1000-1700) | ×2-3 into the retail range |
| `max_speed` | 50 m/s | `maxRpm` (7100-10000; top speed follows it) | rank against the other Roarcage cars into 7100-10000 |
| `max_boost_speed`, `max_boost` | 75, 200 | `turboRpmScale`, `turboTorqueScale` (4WD/turbo) | the boost-to-top ratio 1.5 as the turbo's |
| `motor_force` (Tuning `mass` 1500) | 1250 | `collider.mass` (15-40), `engine.torqueCurve` | force / mass as acceleration; mass into 15-40 |
| `brake_force` | 3000 | `accel.brakeDecel` (rpm/s) | scaled |
| `max_stunt_speed` | 400 | `stunt` pitch/roll rate (1.8-2.6), spin = pitch + 0.6 | rank into the range |
| `front_stiffness`, `back_stiffness`, `max_steer_angle` | 4, 6, 12° | wheel `friction` front 2.3-4.7 / rear 1.9-3.5, `engine.steerRate` | ratio kept |
| `jump_force`, `jump_count` | 20000, 1 | none (no jump in HWVX) | dropped |
| attack, crit, items, abilities, weapons (`shotgun`, `atom_blaster`) | | slaves (weapon slots) / items | the base car's; weapons are a separate mod question |
| stat bars | | `hud` bytes SPEED / ARMOR / GRIP / STUNT (25-100) | computed from the above, as the editor's Content tab shows them |

**Stages.** (1) A car project and `hwvx-x car build` with the base car's skeleton and records,
only the body geometry replaced (one bone), wheels the base's: drivable, tested like a world
(drive, car log). (2) Own wheels (geometry and positions), collider box, shadow hull. (3) Own
textures (PNG → CMPR/DXT, with stage 6). (4) Stats mapping as a table the editor shows and
edits. (5) Shipping it in a `.xmod` (with stage 4's checks). (6) Breakable parts and LODs. The
same licensing rule as the level: the user's car is read from a path given in an environment
variable by any test, never committed.
