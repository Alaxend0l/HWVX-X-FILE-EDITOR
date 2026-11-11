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

## Still to research (task: world quadrant research)

* The collision packs in `worlds_/col_` and how they tie to quadrants.
* `QuadContents` (instances, effect points) and how scripts refer to them.
* The "world quadrant" (`pWorldQuadrant`, NULL region) and what it holds.
* Budgets: geometry and memory per quadrant and per active set (a quadrant and its neighbours)
  across the retail worlds; the retail regions' shapes (convex or not, plane counts).
