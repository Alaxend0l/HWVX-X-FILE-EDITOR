# Worlds: quadrants, regions and streaming

Research for new worlds (the auto-chunker). Code is HWVX-SDL's game C (`src/game`), from the
GameCube decomp; `BSPN0.c` is a matching unit, so its code is the original's.

## Quadrants

A world (`World`) is a set of quadrants (`WorldQuad`, `World.h`), **a graph, not a grid**: each has
a list of neighbouring quadrant indices (`neighbors`), its geometry (`numGeoms`/`geoms`, loaded
from a quad pack), its contents (`QuadContents`: instances and effect points) and a **region**
(`bsp`).

* `World_Render` (`World.c`), every frame, per camera: is the camera still in its current
  quadrant's region? If not, which neighbour's region holds it? If none, which quadrant's
  (all of them, skipping the "world quadrant")? The quadrant found is shown
  (`World_ShowQuadrant`); none found leaves the camera without a quadrant.
* `FileStreaming_SetActiveQuadrants` keeps the current quadrants and their neighbours loaded and
  releases the rest (up to 12 at a time).
* `State_ProgressionManager.c` uses the same region test for the player's car.

## Regions: `BSPN0`

`BSPN0_CollideWithVector(node, point)` answers "is this point inside the region". A node is 0x20
bytes:

| off | type | meaning |
|---|---|---|
| 0x00 | Vector4 | plane: normal x, y, z and distance (`Distance_Plane` > 0: in front) |
| 0x10 | u8 | flags: bit 0, a node follows for the front side (`node + 1`); bit 1, a node follows for the back side (`node + next`) |
| 0x12 | u16 | `next`, in nodes |

In front of a plane: continue at `node + 1` if bit 0, else **outside**. Behind it: continue at
`node + next` if bit 1, else **inside**. A solid-leaf BSP: the back side is solid.

* **A convex region** (the intersection of "behind" half-spaces) is a chain: every node has bit 1
  with `next` = 1, the last none; bit 0 is never set. So a generated cell needs only its
  bounding planes, normals pointing out.
* Concave regions use front branches (bit 0).
* The same nodes serve the fog and reverb volumes (`VolumeManager.c`: point in volume) and
  collision against planes (`BSPN0_CollideWithSegment`, `CollisionThing.c`: the first plane a
  segment enters through, and the clipped end point).

## Chunking: quadrant packs, regions, collision, budgets

How it was found: the game code (`World.c`, `FileStreaming.c`, `DataGamePackLoad.c`,
`State_ProgressionManager.c`, `CollisionTerrain.c`, `World.h`, `Collision.h`), and every retail
world of the three releases measured with `tools/world_stats.py` and `tools/bsp_build.py`
(they read the archives in place; output in `build-worldstats/`, gitignored):

    tools/world_stats.py                    # all releases in sources.cfg -> build-worldstats/stats.md, raw.json
    tools/world_stats.py --rel gc --out build-worldstats/gc
    tools/bsp_build.py check --rel gc       # rebuild every retail BSP -> bsp_check.md
    tools/bsp_build.py rules --rel gc       # layout facts of the retail trees
    tools/bsp_build.py demo                 # a box and an L-shaped prism as BSPN0

GC 57 levels, PC 51, PS2 57 (PC has one level, not two, of each race set). 11 streamed sets
(`w1_geo` .. `w5r1_geo`) and 8 single-quadrant battle arenas.

## Packs: what a set shares and what a level has

| pack | archive | type | scope | holds |
|---|---|---|---|---|
| world pack | the level (`ch1_l2`, `world1l1`) | 4 | the level | the quadrant table (`WorldQuad`, 0x50 each), every quadrant's region (BSPN0) and neighbour ids, every quadrant's **contents** (instances, effect points, item points), records, volumes (fog/reverb, BSPN0 or sphere) and their grid, nav data, start tiles, paths, scripts/triggers, variables, timers. 1.9 - 8.5 MB |
| quadrant packs | the set (`w1_geo`) | 7 | the set | one per quadrant: its **geometry only** (render meshes, vertex data, display lists) plus relocations: texture ids (into the world texture pack), shaders (into its shader list), pointers. Id `numGeoms << 16 \| packId`; the hub's packId is 0xFFFF |
| collision pack | `w1_col` (named by the world pack) | 5 | the set | one grid of cells over the whole world, each cell a solid-leaf BSP (or poly list), plus the crease edges and materials. **Not per quadrant**, loaded whole at level start |
| world textures | `w1_tex` (named by the world pack) | 6 | the set | the textures and shaders all the set's quadrant packs use, loaded whole |

Levels of a set (`world1`, `world1l1`, `ch1_l2` ...) share the set's quadrant packs, collision
and textures; their world packs differ (quadrant ids differ per level, packIds are the same).
The quadrant packs' two shared-pack references (`QuadPackHeader.refs`, type 8 packs) are unused
in every retail pack (0 texture relocations with a pack id); a quadrant's textures are all in
the world texture pack.

**Global, always in memory**: world pack (incl. all quadrants' contents and regions), collision,
world textures/shaders, the hub quadrant's geometry. **Streamed**: only the quadrant packs'
geometry.

## Streaming: how quadrants are picked

Every frame, `World_Render`, for each camera (at most 5, `Gfx_GetCameraPositions`):

1. Still inside its current quadrant's region (`BSPN0_CollideWithVector`)? Keep it.
2. Else the first of the current quadrant's neighbours (list order) whose region holds it.
3. Else (or no current quadrant) the first quadrant in table order, skipping the hub, whose
   region holds it; this search also makes this frame's loads synchronous (not threaded).
4. None: the camera has no quadrant; only the hub is drawn for it.

The found quadrant and all its neighbours are *visible*. Visible and loaded ones are drawn;
visible and not loaded ones are loaded (`FileStreaming_LoadQuadPacks`, on the loading thread
unless step 3 ran); `FileStreaming_SetActiveQuadrants(visible)` spawns the contents of newly
visible quadrants (destructibles, emitters, coronas; static scenery only gets its culling
radius) and releases the things of quadrants no longer visible, queueing their packs for release.
The hub is drawn every frame and loaded at level start.

Because the current quadrant is kept while the camera stays inside it, **overlapping regions
give hysteresis**: about 10% of the retail drive points (nav points and start tiles) lie in two
or more regions, 0% in none (every level, every release).

At level start (`State_ProgressionManager.c`, loading state 3): if the world has at most 11
quadrants (`FileStreaming_GetNumAvailableQuadPacks`) all are loaded; otherwise the hub, the
region holding player 1's start tile (`TileCar.port == 1`) and its neighbours.

Quadrant packs are read from the set's archive by TOC: type 7, the world's id, `packId`, and
`id >> 16 == numGeoms` (a mismatch is the "repack the world using the latest BrainCore" error);
each pack's version must be 0xB7. Memory is a plain `Allocate(size + 0x40)` per pack, no pool.

## Quadrant regions (inferred from the data)

All 127 streamed quadrants of all three releases (regions are identical on GC, PC, PS2):

* **Vertical prisms.** Every region plane is horizontal (top, bottom) or vertical (walls),
  except 65 sloped nodes in `w5` and 3 in `w2`: a 2-D outline in X/Z extruded between two
  heights. Planes per region 6 / 7 / 32 (min / median / max), nodes 12 / 12 / 87; 92 of 127
  convex (a box is the median).
* **Built from a triangle mesh, one node per triangle**: a 12-node region is a 6-face box in 12
  triangles; the second triangle of a face is the first's front child (same plane). See "BSP
  build rules" below.
* **Coverage**: regions cover every drive point; in `w1` 37% of the collision bounds (sampled
  on a 192 x 192 x 3 grid) is in no region (sky, out-of-bounds), 58% in one, 5% in two or three.
* **The hub** (quadrant 0 in every streamed world, NULL region, packId 0xFFFF): backdrop and
  far geometry seen from everywhere (0 - 17 geometries, 108 B - 184 KB) and the instances that
  lie in no region (all hub instances lie in no region).
* **Battle arenas** are one hub quadrant with everything (bat1_l1: a hub with no geometry and
  one regioned quadrant with no neighbours).

### Who owns what

* **Objects** (instances, effect points, item points; per-quadrant `QuadContents`): the
  **first region in quadrant-table order that holds the object's position**, else the hub.
  No exception in any level of any release (objects inside two or more regions: GC 4,495,
  PC 4,282, PS2 4,495, all owned by the first region holding them).
* **Geometry**: assigned by the authors, not by a rule we can recompute: a geometry's centre is
  in its own region for ~88% (w1: 563 own / 19 only in another region / 53 in none); of the
  rest most have their box overlapping their own region. Geometry ids are unique across a
  level's packs (no geometry in two packs).

### Neighbours

* At most **4** per quadrant in every world (median 4; race loops 2).
* **Regions that touch are listed** (all touching pairs but one in w3, w4, w5: 1 pair each), and
  the rest of the 4 are further quadrants (w1: 47 listed pairs, 27 touching): what can be seen
  from inside. 66 of 80 links in w1 are two-way.
* A camera in Q draws Q, Q's neighbours and the hub only, so **everything visible from inside a
  region must be in it, a neighbour, or the hub**.

## Collision packs (inferred from the data)

`WorldCollision` (`World.h`), version 0xD:

* **One grid over the world's collision bounds**, `numX x numY x numZ` squares of exactly
  `(max - min) / n` (the bounds are a whole number of squares in every pack). Square sizes
  174 - 436 (x, z) by 115 - 383 (y); grids 3 x 2 x 9 to 27 x 4 x 33.
* `cellIndex[z * numX + numZ * (y * numX) + x]`: 0 for an empty square, else its own cell. No
  cell is shared by two squares; each cell's box is exactly its square.
* **mode 1** (every pack but `bat2_l9`): a cell is a terrain BSP (`TerrainBSPNode` 0x20:
  plane, material index, u16 front/back, 0xFFFF none; d >= 0 is in front, a missing back child
  is solid), plus the indices (u16) of the **edges** in it. No polygon list (`numPolys` 0); the
  cell's 0x3C word is its node count. **mode 0** (`bat2_l9`, 1,492 polys): poly lists, no BSP,
  no edges.
* **Edges** (`TerrainEdge` 0x30: two points, material) are global and shared: an edge is listed
  in every cell it crosses (w1: 885 of 2,897 edges in two or more cells). They are the mesh's
  **crease edges**: where two non-coplanar surfaces meet (w1: 2,954 of 2,966 edge midpoints on
  two planes of the cell's tree, angles 0.9 - 180 deg, median 89 deg); used by the box tests
  (`OBoxHitsSegment`) so cars catch on corners.
* Nodes per cell 1 / 40 / 3,019, tree depth 1 / 15 / 274, edges per cell 0 / 4 / 429, edges per
  pack up to 8,118 (u16 indices: at most 65,535).
* The terrain exporter **merged coplanar triangles into polygons** before building (in 91,529
  cell nodes of w1, w2, bat3, only 42 nodes repeat an ancestor's plane; regions, built from raw
  triangles, repeat it in 1,804 of 2,008).
* The solid of a cell is open below and at the sides (terrain is an open surface): ray and box
  hits outside the cell's box are dropped (`RayHitsBSP`), so the solid only matters inside it.

Releases: GC and PS2 have the same trees except `w4_col` (same node counts, some floats
differ); PC differs in `w2_col`, `w3_col`, `w4_col` and `bat1_l1` (re-exported: e.g. bat1_l1
1,844 nodes vs 1,993). Majority (GC + PS2) is the reference.

## BSP build rules (both encodings)

What the retail trees are consistent with (`bsp_build.py rules`, `check`):

* **Splitter = the first polygon of the list**; the others go, in order, to the front list, the
  back list, or are cut in two. No balancing (depth / log2(nodes + 1): median 1.9 for regions,
  2.7 for cells, up to 22).
* **Coplanar**: a polygon in the splitter's plane goes **in front if it faces the same way,
  behind if it faces the other way** (regions: 1,804 same-way in front, 1 same-way behind, 2
  other-way behind, 0 other-way in front).
* **On-plane tolerance**: about 0.002 units for regions/volumes (0.001 and 0.004 rebuild fewer
  trees exactly), about 0.0001 for terrain cells.
* **Pre-order layout**: node, front subtree, back subtree (all 2,008 region, 2,108 volume and
  every cell node). BSPN0 needs it (front child = node + 1, back = node + `next`).
* Unit normals (4 of 2,108 volume nodes not); unused bytes are 0xCD (MSVC's uninitialised heap):
  BSPN0 byte 0x11 and 0x14..0x1F, terrain 0x18..0x1F.
* Retail meshes have faces whose two triangles are a hair out of plane (float error): the second
  triangle lands in front of the first with a slightly different plane, leaving **slivers**:
  thin wedges of solid that run out to the world's edge. In point tests they are below 0.05
  units thick where we sampled; harmless, but a generator should not copy them.

### Rebuild check (`bsp_build.py check`, all retail trees)

The source meshes are not in the data, so each tree's input polygons are recovered from the
tree itself: per node, the hull of the solid's surface on its plane inside its cell (else the
plane inside its cell). Built with the rule above, in node order:

| GC trees | identical bytes | equivalent (shuffled rebuild) |
|---|---|---|
| regions, all levels (105 distinct) | 105 (100%) | world1: 20/20 trees, 100% of 39,610 points |
| volumes, all levels (162 distinct) | 161 (99.4%) | world1: 24/24 trees, 100% of 47,235 points |
| terrain cells, w1_col (first 40 / first 200) | 34 (85%) / 173 (86.5%) | 40 cells: 99.3% of 74,482 points, 97.6% of 19,000 segment first hits |

(The full run over every cell of the three releases, `bsp_build.py check`, takes hours on a
shared machine and was not finished; re-run it for the complete table.)

*Identical*: the rebuilt bytes equal the pack's (including the 0xCD fill). *Equivalent*: the
tree with nearly coplanar planes snapped together, its surface closed by the box, rebuilt in a
**shuffled order** with tolerance 1e-4: share of sampled points (not within 0.05 of a plane)
with the same inside/outside answer; for cells also the share of segments with the same first
hit. So the builder reproduces the exporter's trees from their polygons, and any polygon order
gives the same solid (different trees, about half the nodes for regions since faces are
built whole).

## Budgets

Measured on every retail world (min / median / max; `build-worldstats/<rel>/stats.md` per set):

| per streamed quadrant pack (127) | GC | PC | PS2 |
|---|---|---|---|
| pack bytes | 1,544 / 165,352 / 376,904 | 21,132 / 247,560 / 461,900 | 2,056 / 274,520 / 494,472 |
| triangles (first LOD) | 26 / 4,489 / 8,599 | 502 / 4,489 / 8,599 | 26 / 4,489 / 8,597 |
| vertices | 50 / 7,017 / 15,170 | 532 / 7,017 / 13,235 | 50 / 8,127 / 15,507 |
| geometries | 1 / 11 / 86 | 1 / 11 / 86 | 1 / 11 / 86 |
| groups (draw calls) | 2 / 71 / 259 | 5 / 76 / 259 | 2 / 71 / 259 |
| textures used | 2 / 26 / 88 | 5 / 26 / 88 | 2 / 26 / 88 |
| most vertices in one geometry level | 6,973 | 6,973 | 8,311 |
| region nodes / planes | 12 / 12 / 87, 6 / 7 / 32 | same | same |
| instances | 0 / 39 / 256 | 0 / 38 / 256 | 0 / 39 / 256 |

| active set: a quadrant + its neighbours + the hub | GC | PC | PS2 |
|---|---|---|---|
| bytes | 249,528 / 800,112 / 1,286,128 | 378,740 / 1,230,728 / 1,750,232 | 419,064 / 1,339,200 / 1,996,112 |
| triangles | 7,403 / 22,180 / 33,582 | 7,403 / 22,025 / 31,213 | 7,403 / 22,179 / 33,582 |
| vertices | 9,390 / 33,903 / 63,834 | 9,389 / 33,770 / 53,222 | 12,054 / 38,671 / 65,239 |

| per set / level | GC | PC | PS2 |
|---|---|---|---|
| quadrants (with hub) | 6 / 6 / 25 | 6 / 6 / 25 | 6 / 6 / 25 |
| world textures (count, bytes) | 23 / 80 / 144, 0.49 / 1.32 / 2.28 MB | 0.44 / 1.30 / 2.30 MB | 0.86 / 2.51 / 4.22 MB |
| world pack bytes (level) | 4.07 / 6.24 / 8.48 MB | 3.21 / 6.41 / 8.05 MB | 1.91 / 4.81 / 7.61 MB |
| collision pack bytes | 77 KB / 272 KB / 3.28 MB | 75 KB / 271 KB / 3.28 MB | 63 KB / 258 KB / 3.26 MB |
| collision cells, nodes per cell | 36 / 104 / 880, 1 / 40 / 3,019 | same | same |
| battle arena (one pack) | 7 KB - 887 KB, up to 9,761 triangles | | |

Every streamed quadrant has one level of detail drawn (all-LOD triangle counts equal the first).

### Hard limits in the code

| limit | where |
|---|---|
| **4 neighbours** in practice: 2 cameras x (1 + 4) = 10 streamed quadrants is exactly `g_LoadedQuadrants[11]` with the hub's slot; more overruns it | `FileStreaming_SetActiveQuadrants` (also `release[12]`, `g_pQuadPackToBeReleased[11]`, `g_QuadrantsToLoad[11]`) |
| 20 visible quadrants per frame over all cameras (`active[20]`, `load[20]`), 5 cameras | `World_Render` |
| at most 11 quadrants: all loaded at level start; more: hub + start region + neighbours; `pQuadsToLoad[50]` | `State_ProgressionManager.c` |
| exactly one quadrant with a NULL region (the hub); the full search skips it | `FileStreaming_InitializeAllQuads`, `World_Render` |
| quadrant pack id 16 bits, `numGeoms` 16 bits, version 0xB7, archive header 0x020100A0 / 0x040100AE, world format 0xB7 | `LoadQuadrants` (the port's TOC is unlimited; the original read 50 entries) |
| terrain node indices u16 (0xFFFF = none): < 65,535 nodes per cell | `TerrainBSPNode` |
| edge indices u16: at most 65,536 edges per collision pack | `TerrainCell.edges` |
| BSPN0 `next` u16: a back subtree within 65,535 nodes | `BSPN0.c` |
| collision version 0xD | `State_ProgressionManager.c` (level load) |

No byte budget is enforced: quadrant packs are heap allocations. The GameCube's 24 MB is the
real limit; retail active sets stay under 1.3 MB (GC) next to a 4 - 8 MB world pack, 1 - 3 MB
collision and 0.5 - 2.3 MB of world textures.

## Cross-quadrant references

* Quadrant pack -> world textures and shaders by index (texture and shader relocations); never
  to another quadrant pack.
* Contents -> records (`ThingRecord`, world pack); static scenery shares `SharedSlaves` across
  quadrants (activated once).
* Scripts (triggers, world pack) change objects in any quadrant by pointer to its entry
  (`ActionDestructibleChangeMode`, `ActionFXPointChangeMode`, `ActionItemChangeMode`): the mode
  is stored and applied when the quadrant is spawned.
* Neighbour lists hold quadrant **ids** (`WorldQuad.id`, per level), turned into indices at
  load.
* Collision, volumes, nav, paths are world-wide: cars far from the camera still have ground.

## Chunker algorithm (glTF world -> set + level)

Input: render meshes (materials, textures), a collision mesh (or render meshes tagged as
collision), objects (instances, effects, items), start tiles and nav points, optional author
hints (region outlines, hub-only meshes).

1. **Clean**: weld vertices (1e-3), drop degenerate triangles, check the collision mesh is
   closed or at least closed downwards (open edges only at the world's rim) and report open
   edges.
2. **Collision pack** (world-wide, not per quadrant):
   1. bounds = collision AABB padded by a little; per axis `n = ceil(span / target)` with target
      ~250 (x, z) and ~200 (y), then square size = span / n exactly.
   2. per square: clip the collision triangles to the square's box; nothing left -> cellIndex 0.
   3. merge coplanar, edge-sharing pieces (same material) into convex polygons.
   4. build the terrain BSP: first polygon is the splitter, coplanar same-facing in front,
      opposite behind, tolerance 1e-4 in double precision, nodes in pre-order, materials per
      node, front d >= 0. Re-split the grid where a cell exceeds ~3,000 nodes or depth ~250.
   5. edges: every crease edge (dihedral above ~0.5 deg) of the collision mesh, once, listed
      (u16) in every cell its segment crosses.
3. **Regions**:
   1. footprint: the X/Z projection of the drivable surface (collision triangles facing up) and
      of everything the camera can reach.
   2. split it recursively (longest axis, at the weighted median of render triangle area) until
      each leaf's render cost fits the per-quadrant budget (target: <= 5,000 triangles,
      <= 8,000 vertices, <= 60 textures, <= 250 KB GC pack) **and** the active set fits
      (<= 30,000 triangles, <= 1.2 MB GC).
   3. leaf -> prism: its rectangle (or the outline of the track inside it), grown by an overlap
      margin (~5% of its size, at least 10 units: hysteresis), between the lowest collision
      point - 10 and the highest camera reach (+ ~100 above the highest drive surface).
   4. check every drive point, start tile and a sampling of camera positions along the paths is
      in at least one region.
4. **Neighbours**: all regions touching or overlapping (symmetric); if any quadrant has more than
   4, split differently (merge small leaves, move the cut); then add visibility neighbours up to
   4: the quadrants whose geometry is within draw distance of the region (by box distance).
5. **Assign**:
   * objects: the first region in table order holding the position, else the hub.
   * render meshes: per mesh, the region holding its box centre (first in table order); a
     mesh larger than a region is cut at region walls and each piece assigned by its centre.
   * the hub: skybox, backdrop, meshes flagged hub-only, anything visible from more than its
     own region + 4 neighbours, objects in no region.
   * check: for each region R, every mesh within draw distance of R's box is in R, a neighbour
     of R or the hub; else move it to the hub (or add a neighbour if R has fewer than 4).
6. **Region BSPN0**: convex prisms as the minimal chain (`convex_region`: one node per outward
   plane, each the back child of the last); concave outlines as the prism's triangles through
   `build` (exact planes per face, so no slivers).
7. **Write**: quadrant packs (type 7, version 0xB7, the world id, `numGeoms << 16 | packId`,
   hub 0xFFFF and index 0), the world texture pack (type 6), the collision pack (type 5, version
   0xD, mode 1), the world pack (quadrant table with ids, neighbour ids, regions, contents).
8. **Verify**: `world_stats.py` on the output against the budget tables above; `bsp_build.py`'s
   checks on the generated trees; the game: drive the route with `HWVX_LOADTIME` and watch for
   synchronous loads (a camera with no quadrant).

## Risks

* **A camera outside every region** sees only the hub and forces a synchronous full search every
  frame: regions must cover all camera space (chase cameras, replays, cut-scenes), not just the
  road.
* **More than 4 neighbours** or 2 cameras over 10 streamed quadrants overrun fixed arrays (memory
  corruption, not an error). Split-screen with 3 - 4 cameras in a streamed world was never
  shipped; keep streamed worlds to 2 cameras or cap the visible set.
* **Pop-in**: a mesh visible from a region but not in it, its neighbours or the hub appears when
  the camera crosses; the visibility pass is only as good as its distance test (no occlusion).
* **Open collision meshes**: a solid-leaf BSP extends a triangle's back side past its edges
  inside the cell; open edges inside a cell give invisible walls or holes.
* **Near-coplanar triangles** (from DCC float noise) give slivers and extra nodes: weld and
  merge coplanar faces first; the builder's tolerance (1e-4) is tight, 1e-3 and above lumps
  real terrain detail together.
* **Deep or large cells** (retail max 274 deep, 3,019 nodes): the box and ray tests recurse;
  big cells cost CPU per car per frame; re-split the grid instead.
* **Big quadrants for the consoles**: retail never exceeds a 377 KB (GC) / 494 KB (PS2) pack
  or a 1.3 MB / 2.0 MB active set; generated worlds meant to run on the original hardware should
  stay inside these (the port itself has no such limit).
* The geometry assignment of the retail worlds is hand-made; our centre rule matches it for
  ~88% of geometries, so generated worlds will not stream exactly like retail ones.
* Not covered here: the volume grid (`VolumeHolder`), nav data, paths, and the per-platform
  geometry encodings (those are `xdl_gltf.py`'s).

## Objects in a world

Triggers and world records (TileCar, TilePath, WorldQuad, volumes, teleporters, variables, timers, objectives) are decoded in [../fields/objects.md](../fields/objects.md).

## The auto-chunker (tools/xdl_chunk.py), as built


For docs/worlds/README.md, a new section after "Chunker algorithm" (before "Risks"); the
format facts at the end also belong in docs/xdl-format/README.md §10.8 (packs) and the
"Collision packs" section here. Branch `chunker` of HWVX-SDL.

```
xdl_chunk.py extract LEVEL PROJECT          a retail level as a project, from the player's data
xdl_chunk.py arena PROJECT                  a generated test world (ground, walls, ramps, pillars)
xdl_chunk.py plan PROJECT                   quadrants, neighbours, budgets -> PROJECT/plan.json
xdl_chunk.py build PROJECT OUT [--base-collision]   hwvx-x extract folders + ids.lock
xdl_chunk.py stage PROJECT OUT DATA STAGED  `hwvx-x pack` into a data folder linking DATA's files
xdl_chunk.py stats PROJECT [--retail LEVEL] the plan next to a retail level's quadrants
tools/chunk_drive.sh BUILD STAGED OUT NAME LEVEL [SECONDS]   drive it as a registered track
tools/test_xdl_chunk.py [--retail [LEVEL]]  pieces on generated geometry; arena; retail collision
```

**The project** (`world.jsonc`): `set` (the archives `worlds_\geo_\<set>_geo`,
`worlds_\col_\<set>_col`), `level` (the level archive), `base` (the retail level whose world pack
the new one is made from: scripts, volumes, nav, records, and its collision pack's materials),
`textures` (the world texture pack, default the base's: textures are links,
`base://w1_tex/world_texture_0/texture/N`, `base://hw_share/share_1/texture/N`), `render` (a
folder of glTFs: xdl geometry glTFs as `xdl_gltf.py` writes them, or plain glTF, one geometry per
mesh node, a group per primitive, unlit and vertex coloured, `extras.hub` or a `hub*` name for
the hub), `collision` (a glTF: triangles with `extras.material`; LINES with `extras.creases`, else
the creases are computed), `objects` (`base`: the base's objects placed by position; `none`),
`hub` (`base`: the base level's hub geometry, sky and backdrop, read from the player's data at
build time), `start` (start tiles moved: x, y, z, yaw), `track` (an id and alias: `build` writes
an `ids.lock` registering the level as a track, §10.5.1), `chunk` (budgets; `grid: "base"` keeps
the base's collision grid). Nothing in a project is game data except what `extract` writes from
the player's own copy (never committed).

**What `build` does** (the algorithm above, as far as it goes):

* **Quadrants are strips**, not a grid: slabs across the whole world along its long axis, cut
  where the next geometry (by box centre) would exceed the per-quadrant budget (5,000 triangles,
  8,000 vertices, 250 KB, 60 textures); the budget is lowered until every active set (a strip,
  two each side, the hub) is within 30,000 triangles and 1.2 MB. A strip touches only its two
  neighbours, so the neighbour list is the two on each side: at most 4, the 10-slot limit holds,
  and no "split differently" step is needed. A geometry reaching past the strips next to its own
  is cut by triangle centroids (the first piece keeps its id).
* **Regions** are boxes (6-node convex chains): the strip grown by 5% (at least 10) on its inner
  walls for hysteresis, the outer strips and the other two axes reaching 20,000 past the world, from
  1,000 below the lowest point to 5,000 above the highest. So every camera is in a region.
* **Objects** go to the first region holding them (the game's rule); the base's hub objects stay
  in the hub. The world pack keeps all its bytes; a new quadrant table, regions, neighbour ids
  and contents are appended, every pointer (and local ref relocation) into a moved object entry
  follows it, and the names of the quad and collision archives are new strings.
* **Quadrant packs**: header (word 0 the geometry table), the table (id, 0, geometry; the third
  word a local ref relocation), the geometries 32-aligned, encoded by `xdl_gltf.encode`, with
  their texture and shader relocations (a group's shader relocation sits at the group record).
* **Collision**: squares of about 250 x 200 x 250 over the mesh; per square the polygons
  clipped to it, in the source's order (the first is the splitter), built with
  `bsp_build.build` (tolerance 1e-4); the crease edges listed in every cell whose box, grown by
  10, the edge crosses; the base pack's materials and everything they use (effect geometry,
  sounds, textures) moved behind the new grid.

**Round trip of a retail world** (GameCube `world1`, Joyride 1): 652 geometries extracted,
re-chunked into 22 strips + hub (retail: 20 + hub), built, staged, started as a registered
track (`HWVX_TEST_EXTRA_TRACKS=alias=0`, `HWVX_TEST_EXTRA_TRACK_WORLD=worlds_\sp_\joyride_\cworld1`,
`HWVX_START=race:mode=joyride,world=5`):

| | retail world1 | re-chunked |
|---|---|---|
| quadrants | 20 + hub | 22 + hub |
| triangles per quadrant (max) | 5,797 | 4,983 |
| pack bytes (max) | 232 KB | 207 KB |
| active set triangles (max) | 28,064 | 25,129 |
| neighbours | <= 4 | <= 4 (2 for the end strips) |
| collision cells, nodes per cell (median / max) | 472, 40 / 3,019 | base grid: 472, 44 / 1,209; own grid: 1,512, 22 / 980 |
| collision pack | 1.36 MB | base grid 1.6 MB, own grid 3.0 MB |

* With the retail collision pack (`--base-collision`) the car's 25 s drive logs the same 234
  lines as on the retail level and the frames match but for smoke and a moving prop (objects
  spawn in another order).
* With the rebuilt collision, points 2 units above / below the surfaces agree with the retail
  trees 98.1% / 99.2% (the difference is mostly the retail slivers, snapped away); random points
  in the cells 98.5% (own grid), 99.98% (retail grid); downward rays onto floors
  report a floor facing up 97.0% (retail 98.6%: its own slivers). The car drives, jumps, scrapes
  walls and never falls through (drives of 45 and 100 s, lowest point the road); its path differs from
  retail after a few seconds (a different tree reports other planes at the same surface).

**The arena** (`arena`: a 3,000 x 3,000 ground, walls, 6 ramps, 8 pillars, 468 triangles of
our own; textures and sky links into `w1_tex` and world1's hub): 4 strips + hub, 169 collision
cells, packs of 5 - 6 KB, world pack world1's with no objects and the start moved; registered by
its `ids.lock` (`HWVX_IDS_LOCK`) as track 57, level 5 of Joyride. The car drives, takes a ramp
(y 2 -> 65 -> lands) and stops at the walls.

**What reconstructing the retail collision taught** (for anyone feeding a BSP-derived mesh back):

* The open terrain surface alone does not rebuild: the solid of an open surface depends on the
  polygon order, which the trees keep only partly. `extract` writes each retail cell's solid as a
  **closed** surface instead (`boundary_polys` + `box_caps` of the cell's tree), one glTF node
  per cell with the cell's box in `extras.sourceBox`.
* **float32 positions** are not enough at world coordinates (an ulp is 5e-4 at 5,000, the
  builder's tolerance 1e-4): each cell's vertices are relative to its centre (node translation),
  and the exact planes ride along (`_PLANE`, with `_POLY` grouping a polygon's triangles).
* **Slivers** (a face's two triangles a hair out of plane) leave thin solid / empty layers; built
  into another tree their surfaces become the plane a ray reports (a floor facing down pushes the
  car through it). `bsp_build.snap_tree` before extracting removes them.
* Closed solids of two sources in one new cell **cannot share a tree**: a splitting plane of one
  leaves the other's inside unbounded. The cell is cut along the sources' box walls first (a kd
  split, axis-aligned nodes) and each part gets its source's tree; a part with no polygons is all
  solid or all empty as its source's tree says.
* A polygon lying in a side of the cell and facing into it (a neighbour's surface or cap) would
  declare the cell empty: dropped.

**Format facts found on the way** (xdl-format §10.8 / collision packs):

* A **local ref relocation** (`ref_relocs`, pack 0) is a pointer the loader fixes up
  (`*slot += base`): the slot holds a data offset, the record's value is the target geometry's
  index. Moving data means moving the slot's value too (the collision packs' two point at their
  materials' effect geometry; the world packs have about 600).
* A collision pack's cell lists the crease edges whose segment crosses its box grown by 10
  (w1_col: 455 of 472 cells exactly, the rest list a few more); the header's 0x44 is the cell
  count with cell 0, a cell's 0x3C its node count (`OBoxHitsBSP` skips a cell whose 0x3C and
  0x40 are both 0).
* The collision materials are a pointer table into the pack's tail (surfaces with spark and skid
  effects, their geometry, sounds and textures): a new collision pack reuses the base's tail.

**Risks and what is left**:

* **View distance**: a camera draws its strip and two each way; in world1 that is 630 - 1,330
  units along the strips' axis (whole width across). Farther geometry is not drawn (pop-in) unless
  it is in the hub. Retail picks neighbours by sight; the strips trade that for the guaranteed
  neighbour limit. A grid of regions with a visibility pass is the next step for open worlds.
* **Collision is equivalent, not identical**: same solid (to ~1%), different trees, so the car's
  path differs from retail; collision packs are 1.2x (retail grid) to 2.2x (own grid) retail's size. Cells fully inside a solid
  of a *plain* mesh (no source boxes) are left empty (a car tunnelling through a thick slab would
  fall); closed or ground-only meshes are fine.
* **The level is the base's world pack**: its scripts, volumes (fog, reverb), nav, paths,
  teleporters and objectives are the base level's, in the base's coordinates. Fine for a
  re-chunked retail level; a new world needs them authored (a world1-based arena keeps world1's
  teleporters and radar map). Objects come from the base or none; there is no object list in
  the project yet.
* **GameCube only** (big-endian, GC display lists); PC encoding exists in `xdl_gltf`, PS2 does not.
* **Archives only from the data folder**: the built archives are staged next to linked base
  files; a mod cannot bring world archives yet (`dvd.c` opens only the data folder), and the
  built packs hold base data (materials, hub, world pack) so they are built on the player's
  machine, never shipped.
* The project is the chunker's own (`world.jsonc` + glTF), compiled straight to `hwvx-x` packs,
  not an `xdl_graph` project.
