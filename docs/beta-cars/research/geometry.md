# HWVX geometry: PS2 prototype (v179) -> GameCube (v183)

Status: GC side is from the decomp/port code (Render_Geometry_.c, Gfx_DisplayList.c,
Gfx_GeometryHandler.c, Gfx_Anim.c, Render_.c, DataGamePackLoad.c, port/gx_fifo.c) and was
checked against all 34 GC car packs. The PS2 side was reverse-engineered from the data and
checked against all 36 beta car packs. The beta->GC mapping was checked on every geometry
whose bbox matches exactly between the two versions (437 geometries, 5746 groups, ~330k
triangles).

Probe scripts are in `SCRATCH/research/` (SCRATCH = the session scratchpad):
`gclib.py` (GC parser), `betalib.py` + `vif.py` (beta parser), `gcgeo.py`/`betageo.py` (dumps),
`gcsurvey.py`, `gcidx.py`, `bboxmatch.py`, `compare.py`, `allpairs.py` (full beta-vs-GC check,
output `allpairs.txt`), `shadow.py`/`shadowall.py`, `bboxcheck.py`, `uvdbg.py`, `flipdbg.py`.
Example: `python3 compare.py ../beta_cars/01_car_0 ../gc_cars/01_car_0 BF10 76C0`.

Use these pairs for validation. Each one has exact bbox and triangle matches for most of its geometries:

| beta pack | GC pack | | beta pack | GC pack |
|---|---|---|---|---|
| 01_car_0 | 01_car_0 | | 19_car_18 | 13_car_12 |
| 03_car_2 | 06_car_5 | | 21_car_20 | 12_car_11 |
| 04_car_3 | 04_car_3 | | 22_car_21 | 19_car_18 |
| 05_car_4 | 05_car_4 | | 23_car_22 | 20_car_19 |
| 06_car_5 | 18_car_17 | | 24_car_23 | 21_car_20 |
| 07_car_6 | 10_car_9 | | 26_car_25 | 23_car_22 |
| 09_car_8 | 03_car_2 | | 27_car_26 | 24_car_23 |
| 10_car_9 | 07_car_6 | | 28_car_27 | 25_car_24 |
| 11_car_10 | 08_car_7 | | 29_car_28 | 26_car_25 |
| 12_car_11 | 09_car_8 | | 30_car_29 | 30_car_29 |
| 15_car_14 | 15_car_14 | | 31_car_30 | 28_car_27 |
| 16_car_15 | 16_car_15 | | 32_car_31 | 29_car_28 |
| 17_car_16 | 14_car_13 | | 33_car_32 | 27_car_26 |
| | | | 34_car_33 | 31_car_30 |
| | | | 35_car_34 | 32_car_31 |

**Warning:** beta 08_car_7 (AT_*) vs GC 02_car_1 is *not* an exact pair. GC rebuilt that car:
the bodies differ, GC has 5 LODs where beta has 1, the wheels are different, and only 3 small
geometries match. Do not validate against it.

All offsets below are blob offsets (pointer slots hold blob offsets and must appear in
pointers.bin). GC is big-endian and beta is little-endian.

---------------------------------------------------------------------------------------------
## 1. GameCube v183 geometry

### 1.1 Geometry header (0x40 bytes, 32-byte aligned in every pack)
| off | type | meaning |
|---|---|---|
| 0x00 | u16 | 0x2003 (constant; beta has 0x1005) |
| 0x02 | u16 | numLODs (1..5 in cars) |
| 0x04 | u16 | skin/bone flag: 0 = static. Non-zero = skinned (car_29 has 7, the same as beta "type"). Render_Geometry draws only when it is 0; skinned geometry goes through Gfx_DrawAnimGeometryAux |
| 0x06 | u16 | 0 |
| 0x08 | ptr | LOD array (numLODs x 0x18). Always placed directly after the header (geom+0x40) |
| 0x0C | ptr | f32 lodDistances[numLODs], 16-aligned. Copied unchanged from beta. The last entry is 1e8. A single LOD with distance 0 means "always" |
| 0x10 | f32[3] | bbox min (object space, exact AABB of LOD0 positions) |
| 0x1C | f32 | radius used by Render_Geometry's culling sphere (center = mid of the translated box). Usually max(\|min\|,\|max\|); in 223/616 cases it is slightly larger, from an unknown source. **Copy beta +0x1C** (beta always stores max(\|min\|,\|max\|)) |
| 0x20 | f32[3] | bbox max |
| 0x2C | f32 | bounding-sphere radius about `center` = \|max-min\|/2 (616/616). Used by Gfx_GetCullLODGX. Beta has 0 here, so **compute it** |
| 0x30 | f32[3] | center = (min+max)/2 (616/616). Beta has garbage (0xCC.. or zeros), so **compute it** |
| 0x3C | f32 | 0 |

LOD selection: dist = dot(center_world - camPos, camRight) * sinHalfFov. The game picks the first LOD i
with dist < lodDistances[i], and uses LOD i+1 in split-screen.

### 1.2 LOD (0x18 bytes each)
The runtime treats a LOD as a "Geometry" with numGroups at +2 and vb at +0xC.
| off | type | meaning |
|---|---|---|
| 0x00 | u16 | 0 |
| 0x02 | u16 | numGroups |
| 0x04 | u16 | "vtxFlags": passed to SetVertexBuffer as `unk4`, which ignores it. **0 in every car pack** |
| 0x06 | u16 | 0 |
| 0x08 | ptr | group array (numGroups x 0x20, 16-aligned) |
| 0x0C | ptr | vertex buffer (or the f32[3] position array for a shadow LOD, §1.7) |
| 0x10 | u32/ptr | 0. In skinned LODs it is a pointer to a small table (`{0x1E,0xF,0,0xF,..}`), probably the bone palette; not decoded |
| 0x14 | ptr | texture-name list: NUL-terminated .tga names, unique, in first-use order, ending with an extra NUL. Beta has the same field. Informational only (GC leaves out names used only by env groups) |

### 1.3 Group (0x20 bytes)
| off | type | meaning |
|---|---|---|
| 0x00 | u16 | 0 |
| 0x02 | u16 | flags (§1.5) |
| 0x04 | u8[4] | GXColor RGBA. The env-map tint for flags&2/0x10 (EnvMap passes it to Render_SetConstColor; mode 2 uses rgb>>2). 0 on plain groups. Same bytes as beta +4 |
| 0x08 | u32 | 0 |
| 0x0C | ptr | UVScroll* `{f32 u, f32 v, GXColor color, u32 flags (bit1: use color as const colour)}`, used as a texture-matrix translation. **NULL in all car groups** |
| 0x10 | u32 | displayList size in bytes (a multiple of 32) |
| 0x14 | u16 | blend mode for alpha groups (Render_StateBlendMode): 0/1 = opaque (ONE,ZERO), 2 = additive (ONE,ONE), 3/8/10 = multiply (ZERO,SRCCLR), 5/6/7 = alpha (SRCALPHA,INVSRCALPHA). Value 5 combined with texture type 4 gives alpha-test cut-out with z-write. Observed values: 0, 1, 2, 5. Copy beta +0x14 |
| 0x16 | u16 | sort/batch key (0x00, 0x16, 0x28, 0x35, 0xFFFF ...). The runtime does not read it as u16 at 0x16. Copy beta +0x16 (equal in 5565/5746) |
| 0x18 | u32 | **texture slot**. A texture_relocs entry `(group+0x18, index \| packId<<16)` makes the loader write the texture handle into the whole u32, so the handle lands in u16 0x1A and u16 0x18 becomes 0. Render uses `unk18 ? unk18 : unk1A`. With no texture the slot is the literal `0x0000FFFF` with no reloc. The pre-reloc content of relocated slots is junk (e.g. 0x00003801); write 0 |
| 0x1C | ptr | display list (32-byte aligned) |

### 1.4 Vertex buffer (16-byte aligned; data at +0x10)
```
+0x0 u32 uvOff    byte offset (from +0x10) of the f32[2] UV array; also = 12*numPositions
+0x4 u32 clrOff   offset of the RGBA8 colour array, or 0 = none
+0x8 u32 nrmOff   offset of the f32[3] normal array, or 0 = none
+0xC u32 size     0x10 + total bytes of all arrays (used for DCFlushRange)
+0x10 f32[3] positions[N] | f32[2] uv[M] | u8[4] rgba[C] | f32[3] normals[K]
```
The arrays are packed back-to-back in that order with no padding, so offsets are only
4-byte aligned. That works on hardware: SetVertexBuffer calls `GXSetArray(POS, data, 12)`,
`(TEX0, data+uvOff, 8)`, `(CLR0, data+clrOff, 4)` if clrOff is non-zero, and
`(NRM, data+nrmOff, 12)` if nrmOff is non-zero. Counts are implicit (next offset / end).
The arrays are de-duplicated: positions exactly equal the set of unique beta positions per
LOD. The UV and normal arrays contain a few duplicates, which is harmless.
Colours: when present, almost always one entry, 0xFFFFFFFF.

### 1.5 Group flags
Low bits (read by the runtime):
| bit | meaning |
|---|---|
| 0x0001 | alpha group: deferred, depth-sorted, Render_DrawAlphaGroup; uses blend +0x14 |
| 0x0080 | also alpha (same path as 1). Not seen in cars |
| 0x0002 | env-map-only: EnvMap(mode 1). Opaque, texture = envmapState.unkA, const colour = group colour, texgen from normals via the view matrix (sphere map). **Base texture slot is dropped (0x0000FFFF)** |
| 0x0004 | lit: GX lighting (dir light + up to 4 point lights + ambient). Material and ambient come from register, so vertex colour is not used |
| 0x0010 | lit + additive env pass afterwards: EnvMap(mode 2), blend 2, colour rgb>>2. Keeps its base texture |
| 0x0040 | split-screen path: not added as an opaque/lit group. Seen as 0x41 |
| 0x0100 | wireframe debug (not a DL). Never set |
| 0x4000 | shadow-volume group (§1.7) |
| 0x8000 | on group 0 of LOD 0: skip far-plane cull. Never set in cars |

High bits (not read by any runtime code found, but always consistent) describe the LOD's
vertex-buffer contents and are identical for every group in a LOD:
* 0x0800: VB has normals, no colours (every group is lit or env, none alpha/unlit).
* 0x1000: VB has colours, no normals (every group is alpha or unlit).
* 0x2000: VB has both.

Observed GC flags in cars: 0x804, 0x806, 0x814, 0x2000, 0x2001, 0x2002, 0x2004, 0x2006, 0x2010,
0x2012, 0x2014, 0x2041, 0x1001. LOD vtxFlags (+4) is always 0.

### 1.6 Display list
Exactly **one** draw command per group, then zero padding:
```
0x90            GX_DRAW_TRIANGLES | VAT 0       (no strips, no fans in any car pack)
u16 nverts      (multiple of 3; max seen ~5800)
nverts x { [u8 pnmtx if skinned] u16 idxA, u16 idxB, u16 idxC }   (big-endian)
0x00 ...        GX NOP padding to a 32-byte multiple; dlSize = roundup32(3 + 6*nverts)
```
All attributes are GX_INDEX16 (GXSetVtxDesc(attr, 3)) and VAT0 is always F32. The renderer
picks the vertex descriptor from the group flags, so the second index means different
things:

| path (group flags) | descriptor function | attributes, in order | idxA | idxB | idxC |
|---|---|---|---|---|---|
| flags & (1\|0x80) (alpha) | SetTriVtxDescIdxAlpha | POS f32 xyz, CLR0 RGBA8, TEX0 f32 st | pos | colour | uv |
| flags & 4, or 0x10 (lit) | SetTriNormVtxDescIdx | POS, NRM f32 xyz, TEX0 | pos | normal | uv |
| flags & 2 (env) | SetTriEnvVtxDescIdx | POS, NRM, CLR0 RGBA8 | pos | normal | (uv index, read as a colour index and ignored because the colour comes from a register) |
| none of 1/2/4/0x10 (unlit opaque) | SetTriVtxDescIdx | POS, CLR0 RGB8 (stride 4), TEX0 | pos | colour | uv |
| 0x10 second pass | EnvMap mode 2 = POS,NRM,CLR | | | normal | uv (ignored) |

So the converter always writes `(posIdx, flags lit/env ? nrmIdx : clrIdx, uvIdx)`.
With base-colour override (g_state.flags & 0x10) a lit DL is decoded as POS,CLR,TEX and the
colour index is read from the normal slot. That is harmless because a const colour is set.
In 0x800 LODs clrOff is 0, so EnvMap reads a stale CLR array; the colour is ignored anyway.

Skinned geometry (header +4 != 0): Gfx_DrawAnimGeometryAux_ sets g_IsAnim, which adds
GX_VA_PNMTXIDX (direct u8) before the indices, so each vertex is 7 bytes. The value is a
matrix row address (matrixlookup[i]: 0, 3, 9 seen). Only one geometry in all car packs uses
this (car_29 @0x40).

Winding: GC triangles have the same vertex order as beta triangles decoded per §2.4
(validated, 0 flips). They are counter-clockwise (right-handed cross product) with respect
to the vertex normals.

### 1.7 Shadow-volume geometry (CarAInfo+0x54)
The geometry header is normal (0x2003, 1 LOD, distance 1e8, bbox as in §1.1). LOD0:
`{u16 0, u16 numGroups, 0, 0, groups, f32[3] positions (no VB header, 32-aligned), u32 0, names}`.
Group (0x20): `u16 0, u16 flags = 0x4000, u8 col[4] = FF FF FF 00, u32 0, u32 0, u32 numEdges,
u32 0, u32 junk (write 0), ptr edges (32-aligned)`.
Edge (0x20 bytes):
```
+0x00 f32[3] n0     = -normalize(cross(b-a, c-a)) of the triangle containing directed edge v0->v1
+0x0C u16 v0, u16 v1   indices into the LOD position array
+0x10 f32[3] n1     = same for the triangle containing v1->v0
+0x1C s32 type     runtime scratch (CreateSilhouette writes 0/1/2), store 0
```
Render_DrawShadows extrudes the silhouette edges 300 units along (0.707, -0.707, 0) in
object space and builds a stencil-like alpha mask.
GC keeps every shared edge between non-coplanar faces, plus some coplanar ones (the exact
threshold is unknown). Including all manifold edges is safe: coplanar edges never become
silhouettes. No boundary edges occur in car shadow meshes.
The position array is de-duplicated per group (GC has 202 verts where beta has 190 unique
positions, for example), so per-group or global de-duplication both work.
The game detects shadow geometry by `groups[0].flags & 0x4000` (DisplayList_AddGeometry,
Render_DrawShadows, Port_Swap IsShadowGeometry).

### 1.8 Pointer slots and alignment checklist
Pointer slots: geom+0x8, geom+0xC, LOD+0x8, LOD+0xC, LOD+0x14 (and LOD+0x10 if non-zero),
group+0xC (if non-zero), group+0x1C. Shadow geometry: the same, with edges at group+0x1C.
Texture: one texture_relocs entry per textured group at group+0x18.
Alignment: geometry 32, lodDistances 16, groups 16, VB 16, DL 32 (and its size a multiple of 32),
shadow positions and edges 32.
GC packs lay a geometry out as header, LOD array, then per LOD (groups, VB, names), then
lodDistances, then all DLs. Nothing depends on this order.
Note: the motion-anim geometry list (anim+0xC, 8-byte entries) holds geometry offsets that
are **not** in pointers.bin in either version. They are relocated elsewhere (by the
object/anim loader, not covered here).

---------------------------------------------------------------------------------------------
## 2. PS2 prototype v179 geometry

### 2.1 Header (0x40)
| off | type | meaning |
|---|---|---|
| 0x00 | u16 | 0x1005 (4101) |
| 0x02 | u16 | numLODs |
| 0x04 | u32 | type: 0 = static, 7 = skinned (one in all cars). 8 is listed in the proto docs, not seen in cars |
| 0x08 | ptr | LOD array (numLODs x 24) |
| 0x0C | ptr | f32 lodDistances[numLODs] |
| 0x10 | f32[3] | bbox min, followed at 0x1C by f32 max(\|min\|,\|max\|) |
| 0x20 | f32[3] | bbox max, followed at 0x2C by 0 |
| 0x30 | 16 bytes | uninitialised (0xCC fill, zeros, or junk) |

### 2.2 LOD (24 bytes)
| off | type | meaning |
|---|---|---|
| 0x00 | u8 | 1 = static, 2 = skinned |
| 0x01 | u8 | 0 |
| 0x02 | u8 | numGroups |
| 0x03 | u8 | 0 |
| 0x04 | u32 | number of unique source vertices (pos, uv, nrm) in the LOD (informational) |
| 0x08 | ptr | groups (numGroups x 32) |
| 0x0C | u32 | hash or timestamp (ignore) |
| 0x10 | u32 | 0 (junk in skinned LODs) |
| 0x14 | ptr | texture-name list (same format as GC) |

### 2.3 Group (32 bytes)
| off | type | meaning |
|---|---|---|
| 0x00 | u16 | 0 |
| 0x02 | u16 | flags (same low-bit meaning as GC: 1 alpha, 2 env, 4 lit, 0x10 env-add, 0x40) |
| 0x04 | u8[4] | env colour (same bytes as GC +4) |
| 0x08 | u32 | 0 (possibly a uvScroll slot; never non-zero) |
| 0x0C | ptr | VIF/DMA packet |
| 0x10 | u32 | **triangle count** (equals the decoded kicks, 5746/5746) |
| 0x14 | u16 | blend mode (same values as GC +0x14) |
| 0x16 | u16 | sort key (same as GC +0x16) |
| 0x18 | u32 | texture slot. Pre-reloc value 0xCCCC0000 \| index. A texture_relocs entry at group+0x18 has the same format as GC |
| 0x1C | u32 | hash (ignore) |

### 2.4 Packet at group+0xC (16-aligned; size = 0x20 + QWC*16)
```
+0x00  16 zero bytes
+0x10  u32 DMAtag 0x6000_0000 | QWC (id 6 = ret), then u32 0 and two VIF NOPs
+0x20  VIF stream (QWC quadwords):
   BASE 0 (0x03000000), OFFSET 0x1DC (0x020001DC)
   repeat per batch (<= 79 vertices):
     STCYCL cl=4 wl=4      (0x01000404)
     UNPACK V4-32 n=1  addr 0 FLG   (0x6C018000)   -> GIFtag
     STCYCL cl=3 wl=1      (0x01000103)   -> 3-quadword vertex stride in VU memory
     UNPACK V4-32 n=N addr 1 FLG   (0x6C..8001)   -> x,y,z f32 + w u32 (bit15 = ADC)
     UNPACK V2-32 n=N addr 2 FLG   (0x64..8002)   -> s,t f32 (UV, same space as GC UV)
     UNPACK V3-32 n=N addr 3 FLG   (0x68..8003)   -> normal f32 xyz   (lit/env groups)
        or UNPACK V4-8 n=N addr 3 FLG (0x6E..8003) -> r,g,b,a u8      (alpha/unlit groups)
     MSCNT (0x17000000)
   VIF NOP padding
```
GIFtag (one per batch): NLOOP = N, EOP = 1, PRE = 1, FLG = PACKED, NREG = 3,
REGS = 0x512 (ST, RGBAQ, XYZ2). PRIM = 0x1C (TRISTRIP | IIP | TME), or 0x5C (adds ABE)
for alpha groups.
A group carries normals if (flags & 0x16) and not (flags & 1). Otherwise it carries RGBA.
Beta colours are half-scale PS2 values: GC_rgb ~ beta*2 (0x7F -> 0xFF exactly; others are
off by at most 1 from the lost source LSB, e.g. 0x12 -> 0x25), and GC_a = 0xFF for beta 0x3F.
Recommendation: `c >= 0x7F ? 0xFF : c*2` (or c*2+1). For alpha, `min(255, a*4+3)`.
No colour data was found in lit groups, and no UV scrolling.

Strip decoding (validated against GC, 100% winding agreement on exact pairs):
* The GS vertex queue restarts at every batch (the GIFtag has PRE=1).
* Vertex i with ADC = 0 (w & 0x8000 == 0) and i >= 2 emits a triangle from the last 3 vertices.
  A vertex with ADC = 1 emits nothing.
* A strip starts at two consecutive ADC vertices. Triangle k of a strip (k = index of its
  last vertex - strip start - 2) is (v[k], v[k+1], v[k+2]) when k is even, otherwise
  (v[k+1], v[k], v[k+2]).
* **Batch continuation:** when a strip is cut by the 79-vertex limit, the next batch starts by
  re-sending the previous batch's last two vertices (both ADC) and continues. The parity
  then continues from the previous batch: the strip position of vertex 0 is
  (previous strip position of that vertex). Detect this case with: previous batch ended on a
  kick, the new batch's v0,v1 equal the old last two (pos, uv, nrm), and the new v2 kicks.
  Ignoring this flips about 1 triangle per cut.
* The result is CCW (right-handed) with respect to the vertex normals.
`betalib.group_tris()` implements this.

### 2.5 Beta shadow source
CarAInfo+0x54 points to a normal static geometry in beta: 1 LOD, groups with flags 0x4,
colour bytes FF FF FF 00, and a texture slot. It holds the closed shadow hull. GC replaces it
with the shadow-volume form (§1.7) with the same group count (7/8 pairs).

### 2.6 Skinned (type 7, LOD byte0 = 2)
The group packet has a different layout: it starts with `{0x00030001, ptr, ...}` instead of
the 2-quadword DMA prologue, and the VIF stream presumably carries matrix indices. **Not
decoded.** Only car_29 @0x20 uses it.

---------------------------------------------------------------------------------------------
## 3. Mapping beta -> GC

### 3.1 Field mapping
| GC | from beta |
|---|---|
| geom+0x00 u16 | 0x2003 |
| geom+0x02 u16 | beta +0x02 |
| geom+0x04 u16 | beta +0x04 (u32 type, 0 or 7); +0x06 = 0 |
| lodDistances | copy (437/437 equal) |
| bbox min/max | copy (exact) |
| +0x1C | copy beta +0x1C |
| +0x2C | \|max-min\|/2 |
| center | (min+max)/2, w = 0 |
| LOD +0 / +4 / +6 / +0x10 | 0 |
| LOD +2 | beta byte +2 |
| LOD +0x14 names | copy the beta string list. Optionally drop names used only by env groups; a texture renamed for GC (e.g. twinmill256v7 -> v8) stays as beta |
| group +0 / +8 | 0 |
| group +2 flags | `f = beta; if (f & 1) f &= ~4;` then OR in the LOD class (0x800/0x1000/0x2000, §1.5) computed over the final group flags of the LOD (needsColour = any group with (f & 0x81) or !(f & 0x16); needsNormal = any group with (f & 0x16) and !(f & 0x81)) |
| group +4 colour | copy (5353/5746 equal; the rest are GC art re-tunes) |
| group +0xC | 0 (NULL) |
| group +0x14, +0x16 | copy |
| group +0x18 | if (f & 2): 0x0000FFFF and no reloc. Else: if beta has a texture reloc, emit one (index remapped to the GC texture table, by name) and write 0. Else 0x0000FFFF |
| group +0x1C, +0x10 | build the DL from the §2.4 triangles |
| VB | positions: unique beta xyz. UVs: unique beta st. Normals: unique beta normals (lit/env groups). Colours: unique converted RGBA (alpha/unlit groups) |

The DL index triple per vertex is (pos, nrm or clr, uv), as in §1.6. Strips become GX
triangle lists: each decoded triangle (a,b,c) becomes three vertices a,b,c. GC does not use
0x98 strips at all, and it reorders triangles (only 419/5746 groups keep beta's order), so
the order is free.
Shared-pack texture indices differ (beta HW_share 0x21 = GC 0x20 for gen_car_tread, for
example), so remap them by texture name.

### 3.2 Validation results (`allpairs.py`, `allpairs.txt`)
* Example `01_car_0` wheel (beta 0xBF10 vs GC 0x76C0): 3 LODs. Group flags 4,4,6,4 map to
  0x804,0x804,0x806,0x804. Triangles 80/40/266/2 \| 70/40/166/2 \| 80/40 are **identical
  sets (positions and UVs) with identical winding**. Unique positions are 230/160/62 on both
  sides and equal to the GC array sizes. (pos, normal) pairs are identical.
* Over all exact-bbox geometries: 3449 groups have identical triangle sets; 2074 more are
  identical once UVs are reduced mod 1; 141 have the same triangle count but slightly moved
  positions; 82 are re-modelled. By triangle count: 72% exact, 21% mod 1, 7% edited.
  Beta triangle count equals group +0x10 in 5746/5746 groups. Normals are identical in
  3546/3800 lit groups.
* UV "mod 1" differences: the GC exporter shifted some triangles' UVs by whole integers
  (e.g. beta v 0.87 vs GC -0.13). Beta's in-range UVs are equivalent under REPEAT wrap, so
  copy beta.
* Flag mapping counts: 4->0x804 (1029), 4->0x2004 (432), 6->0x806 (781), 6->0x2006 (553),
  0x14->0x814/0x2014, 5->0x2001/0x1001, 0x45->0x2041, 2->0x2002, 0->0x2000, 0x10->0x2010.
  The only systematic exception: the beta "Blacky.tga" geometry (flags 4, blend 0) became
  GC 0x1001 with blend 2 (additive, unlit, colour FFFFFF00). "pure_white.tga" (beta 5, blend 2)
  and "gen_car_brakelight.tga" (beta 5, blend 2) become 0x1001 by the rule above.
* GC env groups (flags & 2) lose their texture in 1208/1208 cases. GC VB colour arrays
  always match the converted beta RGBA.
* Shadow: in 8/8 checked cars, GC CarAInfo+0x54 is a shadow volume and beta's is a lit flags-4
  hull. The GC edges are a subset of the beta shared edges, normals match -N(face) to 2e-3,
  and group counts match in 7/8.

### 3.3 Data GC needs that beta lacks, with defaults
* center and +0x2C radius: compute from the bbox.
* LOD class bits 0x800/0x1000/0x2000: compute (§3.1).
* Index arrays and de-duplication: compute.
* Colour array for alpha/unlit groups: convert beta RGBA (§2.4). If a group has none, use one
  entry 0xFFFFFFFF.
* uvScroll: NULL.
* Shadow edge lists: generate from the beta +0x54 hull (§1.7).
* GC packs have extra LODs only where the GC artists rebuilt a car (02_car_1). Converted beta
  cars keep beta's LOD count, which is fine.

## 4. Uncertainties
* The high group-flag bits 0x800/0x1000/0x2000 are not read by any decompiled code, so their
  meaning is inferred. The rule above reproduces all 1819 GC LODs.
* GC +0x1C (min.w) sometimes exceeds max(\|min\|,\|max\|) and its source is unknown. Using
  beta's value (smaller in those cases) gives slightly tighter culling, which should be
  acceptable.
* Group +0x16: semantics unknown (copied). Beta 0x8: unknown (always 0).
* Shadow-edge coplanarity threshold, and the order of edges and vertices in GC, are unknown.
  Any order works.
* The exact beta->GC colour LSB cannot be recovered (±1).
* Skinned geometry (beta type 7, GC +4 = 7, LOD+0x10 table, 7-byte DL vertices) is only
  partly understood. Beta's skinned packet layout is not decoded.
* Whether GX on real hardware tolerates the stale or out-of-range CLR reads in env groups:
  the retail data does the same thing, so it is safe to copy the convention.
* Some GC groups (about 9 of 5746) changed lit/env/alpha flags or blend versus beta. These
  look like art fixes, not format rules.
* Texture index remapping (local and shared packs) needs the texture name tables, which are
  covered by the texture spec, not here.
