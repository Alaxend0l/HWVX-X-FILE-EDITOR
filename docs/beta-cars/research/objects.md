# HWVX car packs: object graph, beta (PS2 v179, LE) vs GC (v183, BE)

Scope: everything in a car pack's blob except geometry internals, texture
pixels and sample data, plus cross-pack imports and the pack header.
All paths below are relative to SCRATCH
(`SCRATCH`).
Run the scripts from SCRATCH (`python3 research/<script>.py ...`).

## 0. Summary

* **Every object type reachable from CarAInfo has the same layout in both
  builds.** Field offsets, pointer-slot positions, u8/u16 field positions and
  array strides are identical. The only systematic difference is
  **allocation alignment: the beta packs every object on 16 bytes, GC on 32**.
  The extra GC bytes are always zero (checked on every object of all 33 GC cars).
  So a converter can swap the beta blob in place, with no re-layout.
* What does differ is **content**: the camera modes (beta has a camera type 1
  that GC does not handle), the HUD interface template, `CarAInfo.explosionThing`
  (0xB0: always set on GC, always 0 in the beta), a few enum-like u8 values,
  and how the shared FX block is split between HW_Cars and HW_share (sounds 0-7
  and 11 moved to HW_share on GC).
* A Port_Swap-equivalent per-byte classifier (`research/swapmap.py`) gives a
  swap plan for 100% of each blob. For the three target cars, 8-14% of the
  blob is object data and the rest is geometry or textures, which other people
  handle. About 79% of the object-data words lie inside walker-typed structs, 13.5%
  in typed struct tails and 7.5% in untyped pointer-reached data, all of which is
  plain 32-bit data.
* All cross-pack imports of the three targets have a GC mapping
  (`research/import_map.json`) except HW_Cars texture #11 (a white 16x16 trail
  texture). For that one, ship a local texture.

## 1. Method and scripts (`research/`)

| script | what it does |
|---|---|
| `walk.py` | Python port of `Port_Swap.c`'s claim walkers (CarAInfo and below, shape scans for motion anims, key anims and event sequences, string heuristic), working for either endianness. It adds per-byte object-type labels, struct extents and the walk order. |
| `walk_im.py` | Port of the interface walkers (IMHeader / IMObject / events / actions / fonts / state anims), used for the car HUDs. |
| `classify.py` | Walk, then the texture region, the geometry closure (chunks reachable from geometry roots), struct tails, and pointer-reached untyped chunks. Labels every byte. |
| `swapmap.py` | Per-byte swap plan (W32 / H16 / KEEP / GEOM / TEX), using the rules of `Swapper_Finish`. `apply()` swaps a blob. `--selftest` round-trips a GC pack to LE and re-walks it (0 claim differences). It writes `research/out/<car>.swapmap`. |
| `coverage.py` | Coverage breakdown per pack. |
| `pair.py`, `fieldcmp.py` | Side-by-side check of a beta car against its GC twin (pointer masks, u8/u16 values, key-frame data). |
| `typemasks.py` | Union of pointer / texture / sound slot offsets per type over all 36 beta and 33 GC cars, flagging structural conflicts. |
| `extents.py`, `padcheck.py` | Object extents per type (this is how the 16/32 alignment was found) and a check that the GC padding is zero. |
| `relocs.py`, `imports.py` | Where texture, sound and ref relocs sit (type+field), and the per-car import list (`imports_targets.txt`). |
| `pathvotes.py` | Maps import indices beta to GC by structural path from the root over 16 paired vehicles (`pathvotes.json`). |
| `share_exports.py` | HW_Cars export tables, both builds. |
| `texdecode.py`, `texmap.py`, `pairsheet.py`, `sheet.py` | Minimal PS2 (8bpp CLUT/CSM1, 32bpp) and GC (CMPR, RGBA8) decoders, thumbnail matching, contact sheets (`tex_cars_sheet.png`, `check1.png`). |

Paired vehicles (beta to GC, by skin texture names): 01-01, 04-04, 05-05,
07_car_6-10_car_9, **08_car_7-02_car_1 (AT)**, 09-03, 10-07, 11-08, 14-11, 16-16,
20-17, 23-20, 26-23, 29-26, 31-28, 32-29. None of the three targets has a GC
twin (Fandango, Sweet 16 II and Jester are beta-only).

## 2. Object types (GC layout; beta status)

Legend: `P` pointer slot, `T` texture-reloc slot, `S` sound-reloc slot,
`R` ref-reloc slot (local geometry or HW_Cars export), `u8`/`u16` small fields.
Everything not listed is a 32-bit u32/s32/f32. "Same" means the same
offsets and pointer and small-field positions in all 36 beta and 33 GC cars
(`typemasks.py`: no conflicts). "Ext" is the blob extent beta/GC, which shows
only the 16/32 alignment.

| type (size) | fields that are not plain words | beta |
|---|---|---|
| Root `{count, table P}` + table `{obj P, id}` | | same (1 entry, id = car index) |
| **CarAInfo** (0x120) | u8 0x10-0x13 (damagePercent, driveMode); u16 0x14 skidTexture, u16 0x16; T 0x44, T 0x50 (skid textures, low half; the beta has 0xCCCC garbage in the high half, which the loader overwrites); P 0x48 steerAnim (KeyAnim), R 0x54 reflectionGeom, P 0x60 rollSound, P 0x6C skidColor (u8x4), P 0x78 modelRef, P 0x7C grip f32, P 0x80 engine, P 0x84 hud, P 0x88 accel, P 0x8C stunt, P 0x90-0x9C AI scripts, P 0xA0 collider, P 0xA4 forcePoints, **P 0xB0 explosionThing**, P 0xD4 wheels, P 0xDC gears, P 0xE4 slaves, P 0xEC weapon slaves, P 0xF4 pickup slaves, P 0x104 startSlaves, P 0x10C startItems, P 0x114 shiftSounds, P 0x11C soundSets | same layout. 0xB0 is always 0 in the beta (GC: an EventSeq with the explosion SoundSeq). u8 0x10 damagePercent is 99 in every beta car and 75 in every GC car. |
| CarAModelRef (0x20) | P 0x10 MotionAnim, P 0x14/0x18/0x1C additive KeyAnims | same |
| MotionAnim (0x3C) | P 0x4 skinned geoms[], P 0xC MotionPart[] (8: R geom, u16 bone 0x4, u16 0x6), P 0x10 KeyAnim, P 0x14 Skeleton, f32 fps 0x18, u8 0x1C-0x1F, P 0x20-0x28 MotionRange (f32x2), SlaveList 0x2C {count, P}, P 0x38 CarPartList | same. In 3 beta cars the 0x2C slave list is non-empty. |
| CarPartList {count, P} -> CarPart (0x1C) | u8 0x0-0x3, u16 0x4, u16 0x6, R 0x8 geom, P 0xC offset | same |
| Skeleton (0x14) -> AnimNode (0x20) | f32 quat+pos, u8 0x1C-0x1F | same. GC has nonzero junk in 0x1D/0x1F (runtime flags); beta has zeros. |
| KeyAnim (0x2C) | P 0x1C per-bone u16 channel flags, P 0x20 f32 times, P 0x24 key data (f32), frames 0x28 | same. Key data is f32 and bit-identical for the AT pair. |
| CarAEngine (0x90) | CameraList 0x80 and 0x88 {count, P -> 0xC entries: u8 type, P params} | **same layout, different content**, see 3.1 |
| Camera params | type 4: Vector4 offset, u16 bone 0x14, u8 0x16-0x17; type 0/2/3: f32 | beta type 1 is not handled by GC |
| CarAHud (0x38) | P 0x10, 0x18, 0x1C IMHeader; u8 0x34-0x37 stat bars | same layout, different template, see 3.2 |
| IMHeader / IMObject (0x6C) / ObjSub / TextInfo / Font / EventDef / StateAnim / ObjAnim / key tracks | as in `Port_Swap.c` (u16 0x0/0x2 header; IMObject u8 0x0-0x9, u16 0xA/0xC/0xE; key tracks with u8 active/interp words; Font u8 0x8-0xB; T in ObjSub 0x4 and Font 0x0) | same layouts (`typemasks.py`). The beta HUD uses no code variables or conditions (0 CodeVar / VarValue against 18 / 43 per GC car). |
| CarAAccel (0x3C), CarAStuntInfo (0x58), f32 grip | StuntInfo: P 0x38 rewardThings u32[], P 0x40 message tables -> P -> strings, P 0x48 failMessage, P 0x50 failThings | same. failMessage (0x48) is always 0 in the beta, which is fine: the game then uses g_stuntLossString. |
| AICarScript (0x34) | u8 0x24-0x27; state tree 0x28 {P info, count, P children (0xC)}; info u8 0x0-0x7, P 0x58 limits (u8 0x10-0x13) | same |
| CarACollider (0x4C) | u8 0x10-0x13; P 0x30 material, P 0x38 endFx (P[] to EventSeq), P 0x44 box, P 0x48 hull | same. In the beta, endFx[0] carries the explosion sounds (see section 4). |
| CarAForcePoints (0xC) -> point (0x30) | u8 0x10-0x13 | same |
| CarTrailBone / wheels (0x50) | u8 0x3C-0x3F, u16 0x40, u8 0x42-0x43 | same |
| CarAGear[] (8) | f32 | same |
| CarASoundSet / SoundSeqSound (0x1C) | u8 0x0-0x7 (type, ?, ?, owner), f32 0x8-0x10, S 0x14 sample, P 0x18 range f32[2] | same layout. u8 0x1 differs: beta 1/5/2/10 correspond to GC 10/6/9/1 (votes over paired sounds), and byte 2 is sometimes 1 against GC 0. GC code never reads byte 1 (`pad01`), so this is cosmetic. |
| CarASlaveSet (0xC) {u32, SlaveList} x11 / weapons / pickups | | same |
| CarAStartSlave (0x50) | u16 0x30, u16 0x32 tag (not claimed by Port_Swap; added in walk.py) | same |
| SlaveTrigger (0x20) | Vector4, u8 type 0x10, u8 0x11, u16 id 0x12, f32 0x14, P data 0x18, u8 0x1C-0x1F | same |
| slave data | 1 EventSeq, 2 EmitterData (u8 0x0-0x3, P SoundSeq 0x20), 3 SoundSeq, 5/6 AttachData (u8 0x0-0x3, **R/T 0x8 model: local geom, HW_Cars export or texture**, key tracks 0xC/0x14/0x1C), 7 Tracer (u8 0x20-0x23, colour keys 0x44/0x4C, T 0x60), 8 MotionAnim, 9 RadarBlip (u8 0x0-0x3, T 0x14, T 0x18, P GXColor 0x1C/0x20), 10 shared slaves | same |
| EventSeq {count, P entries (0xC: type, P a, P b)} | b emitters: particle (0x30: u8 0x20-0x22, P 0x28 adv {u8 0x8-0xB, P data (u8 0x10)}, P 0x2C info (u8 0x10)), position (0x20: u8 0x18-0x1B, P 0x1C range); a by type: Particle (0x24: u8 0x0, P anim -> {u8x4, count 0x20, **P 0x24 texture-slot list (T)**, key tracks 0x28}, colour keys 0x14, P 0x20 LineDef list), Projectile (0xAC: u8 0x0-0xB, R 0x10 HW_Cars model, tracks 0x8C/0x94, slaves 0x9C, P 0xA8 u16x2), Blast (u8 0x0-0x7, P 0x1C {u8x2}, P 0x20 FlashDef, point keys 0x2C), Sound (type 10, same as SoundSeqSound: S 0x14), SoundSeq (26), ThingRecord (6/7) | same |
| ThingRecord (0x20) | R 0x0 geometry or MotionAnim, u8 0x4-0x7, P 0x10 DestructibleInfo, slaves 0x18 | same |
| DestructibleInfo (0x4C) | u8 0x10 type, u8 0x11 takesDamage; P 0x30 material, P 0x38/0x40 debris, P 0x44 box | same layout. In all cars the beta has type 3 ("box from geometry bounds") with takesDamage 0; GC has type 0 with takesDamage 1. Type 3 is valid on GC but reads the bounds from the converted geometry header. |
| SoundSeq (0x10) {u32, u8 0x4-0x5, u16 0x6, keys 0x8 (0x10: f32, u8 0x4-0x7, count, P list -> P Sound)} | | same |

Validation:
* `typemasks.py`: over all 36 beta and 33 GC cars, no offset is a pointer, texture or sound
  slot in one build and plain non-zero data in the other. Optional pointers
  that are only set in one build: CarAInfo 0xB0 (GC), 0x64 (one GC car),
  0xF4 (one beta car); StuntInfo 0x48 (GC); IMHeader 0x20/0x28 sound seqs (GC);
  EmitterData 0x20 SoundSeq (beta, 36x); MotionAnim 0x30 (3 beta); AIStateInfo
  0x58 (1 GC); CondList / TextInfo 0x8 (GC); Blast 0x28 and Font 0x24 (GC).
* `pair.py` and `fieldcmp.py` on AT (08_car_7 against 02_car_1): CarAInfo, Engine, Accel,
  Collider, ModelRef, MotionAnim, KeyAnim, Skeleton, gears, force points and
  wheels have identical pointer masks. Values differ only where tuning differs
  (CarAAccel 0/15 words differ, CarAInfo 7/45). Every u8/u16 field holds the
  same or a plausible value. KeyAnim key data is the same f32s.
* `swapmap.py --selftest gc_cars/02_car_1`: 0 claim differences after a GC to LE round trip.
* GC padding: zero in 100% of the non-array objects (`padcheck.py`).

## 3. Content differences a converter must handle

1. **Cameras (CarAEngine 0x80/0x88).** The beta 1P list has types `1,1,1,4,3,2`
   and the split list `1,1,4`. GC has `0,0,4` and `0,0`. GC `CameraManager`
   handles 0 (default chase), 2 (zoom), 3 (orbit) and 4 (bone). **Type 1 does not
   exist on GC**, and the beta type-1 params (0x18 bytes: 0.3, 230, 50, 3, 6, 0)
   do not match GC type-0 params (75, 25, 3.6, 40, 20, 0.235). Recommendation:
   replace both camera lists with a GC donor car's lists and params (copy
   objects). Beta types 2/3/4 params do match the GC structs (zoom 0x10, orbit
   0x10-0x1C, bone offset + u16 bone 0x14), but their bone index refers to the
   beta skeleton.
2. **HUD (CarAInfo 0x84).** Each build uses one fixed template for all its cars
   (beta: 3 headers with 41/36/20 objects, no code variables; GC: 44/38/24
   objects plus nested headers, 18 CodeVars). The beta HUD's HW_Cars/HW_share
   texture imports would also need remapping. Recommendation: **transplant the
   GC CarAHud subtree from any GC car** and keep the beta stat bars
   (CarAHud u8 0x34-0x37). This also removes the HUD-only texture imports
   (HW_Cars #4-#8, HW_share #67/#68/#81 in ObjSub).
3. **explosionThing (0xB0).** Optional. On GC it is an EventSeq with one entry
   `{26, SoundSeq(GC HW_Cars snd 3-8), PosEmitter}`, used when an AI car dies.
   If it is left 0 (beta), the game skips it (`CarA.c:2091`). To match GC, add an
   EventSeq pointing at the beta car's collider-endFx SoundSeq.
4. Enum-like u8s that differ but are not read or are valid: Sound u8 0x1
   (unused), DestructibleInfo type 3 (valid), damagePercent 99 (tuning).
5. Pointer and reloc garbage: beta texture-reloc slots carry 0xCCCC in the high
   half. The loader overwrites the whole u32, so they can be zeroed.
6. Alignment: in-place swapping keeps 16-byte object alignment. The GC game's
   C code only needs 4-byte alignment for these structs, and the SDL port does
   not care. Display lists and textures are rebuilt by the geometry and
   texture converters, which should 32-byte align their output. A full 32-byte
   re-layout is possible but needs object boundaries: every object starts at a
   pointer target, but some targets are interior (on GC 115 of 3322 targets are
   not 32-aligned).

## 4. Byte classification and coverage (target cars)

`python3 research/swapmap.py beta_cars/02_car_1 beta_cars/13_car_12 beta_cars/18_car_17`

| car | size | geometry (delegated) | textures + palettes (delegated) | object data | of the object-data words: walker-typed / typed tails / untyped |
|---|---|---|---|---|---|
| 02_car_1 Fandango | 1,073,164 | 76.1% | 15.1% | 8.8% (W32 80,772, H16 1,006, KEEP 12,427) | 79.0% / 13.5% / 7.5% |
| 13_car_12 Sweet 16 II | 1,170,988 | 84.0% | 8.2% | 7.9% (W32 79,436, H16 994, KEEP 11,931) | 79.0% / 13.6% / 7.4% |
| 18_car_17 Jester | 660,204 | 71.5% | 14.5% | 14.1% (W32 79,920, H16 1,010, KEEP 11,852) | 78.5% / 13.7% / 7.8% |

100% of each blob gets a class: 0 unreached bytes, 0 unlabelled pointer
slots. "Untyped" data is reached through pointer fields the walker does not
type (Sound range arrays, emitter ranges, KeyAnim times/data, DestructibleInfo
material/debris/box, CarPart offset vectors, Particle.anim texture lists).
These are all 32-bit data, exactly as Port_Swap swaps them on GC.
The geometry closure is "chunks reachable from geometry roots" (MotionAnim
skinned/rigid lists, CarPart 0x8, ThingRecord 0x0, CarAInfo 0x54, AttachData
0x8, Projectile 0x10, plus local ref_relocs). Beta geometry is the PS2 format
(header `0x0001_10_05`, VIF data), so it must be rebuilt, not swapped.

## 5. Cross-pack imports

Reloc tables (all slots hold u32). `texture_relocs` `(slot, pk<<16|idx)` are
filled with a texture id, `sound_relocs` with a sound id, and `ref_relocs` with a
pointer: when pk != 0 the loader stores the u32 at `index*4` of the shared pack's
blob, which is the shared pack's export table, itself made of local ref_relocs at
blob offsets 0..0x3C. Local ref_relocs (pk 0) are geometry pointers numbered
0..n, and the loader adds the pack base to them. The three targets import the
same set: **tex HW_share 31 indices, tex HW_Cars 15, snd HW_Cars all 50,
ref HW_Cars all 16** (`research/imports_targets.txt`). They import no sounds from
HW_share. The full table, with evidence, is in `research/import_map.json`.

* **ref HW_Cars (geometry exports): identity 0-15.** Both export tables list 16
  geometries in the same order (same texture names and bboxes; gadgets 4-7 are
  about 0.75x scale on GC). Path votes agree (all 14 voted indices map to themselves).
* **snd HW_Cars (50) -> GC.** 0->HW_share#2, 1->S#0, 2->S#5, 3->S#8, 4->S#4,
  5->S#3, 6->S#7, 7->S#1, 11->S#10 (moved to HW_share on GC); 8,9,10 ->
  HW_Cars#0,1,2; 12-17 -> #3-8 (the explosion SoundSeq, with the same 6 sounds in the same
  order); 18->#9; 19-43 -> #14-38; 44-49 -> #42-47. GC-only sounds: HW_Cars
  #10-13 (HUD events), #39-41 (weapons); HW_share #6, #9, #11-14. The samples were
  re-recorded on GC (different lengths), so this mapping is by role.
* **tex HW_share (82 beta / 81 GC):** 0-3 and 5-9 identity; **4->52** (all GC cars
  use #52 for this particle sprite); 10 has no exact counterpart (font page, not
  imported by the targets); **11..81 -> i-1** (19->18 keeps the beta orange ring;
  GC cars use HW_Cars #14, a blue recolour). For most used indices, path votes
  (`pathvotes.json`) and image distances (`texmap.py`) agree. The rest follow the sequence.
* **tex HW_Cars (20 beta / 27 GC):** 0->0, 1->2, 2->3, 3->4, 4->5, 5->6, 6->12,
  7->7, 8->8, 9->9 (font), 10->15 (radar blip; GC uses #15 at RadarBlip+0x14 and
  #16 at +0x18), 12->19, 13->20, 16->23, 17->25, 18->24, 19->26. Images were
  checked (`tex_cars_sheet.png`, `check1.png`). Indices 4-8 are used only by the
  HUD and drop out if the HUD is transplanted.
* **No GC counterpart:** HW_Cars tex **#11** (white 16x16, used by the tracer
  T 0x60 and the AttachData trail textures). GC cars use car-local textures or
  HW_Cars #17/#18 there. Ship it as an extra local texture, or remap it to #18
  and accept the look change. HW_Cars tex #14/#15 (solid blue) and HW_share #10
  have no counterpart but the targets do not import them.
* Local (pk 0) texture and sound relocs are indices into the car's own texture
  and sound lists. Renumber them if the texture or sound converters reorder the
  lists.

## 6. Pack header (pack.json)

| field | beta targets | GC cars | action |
|---|---|---|---|
| refs | [HW_Cars#2, HW_share#1] | same | keep |
| num_textures / texture_data_offset | 2-5 / set | 2-6 / set | recompute after the texture conversion (GC: 0x20-byte headers, 32-byte aligned) |
| num_palettes / palette_offset | = num_textures / set (PS2 CLUTs) | 0 / 0 | set both to 0, drop the CLUT data |
| num_sounds, sound_data, sound_pointers | 1-7 | 2-5 | sound converter. GC SFX descriptors: u32 0x0-0xC, u8 0x10-0x11, u16 0x12, u32 0x14-0x1C |
| num_shaders, shader_offset, num_triggers, trigger_offset, sfx_offset, song_offset, song_bank_offset | 0 | 0 | nothing to do (cars have no shaders, triggers or songs) |
| shader_relocs (numGX) | none | none | none |
| section order on disk | beta: ref_relocs + pointers before data, no padding | GC: data, then ref_relocs + pointers; reloc tables padded to 32 | handled by the hwvx-x `join` for platform gc |

## 7. Uncertainties

* Texture matches for near-transparent sprites (HW_share 0-5, 12-27) rest on
  low-resolution thumbnails plus the "i-1" sequence. Votes confirm most of them, but
  4->52 and 19->18 (or HW_Cars #14) are judgement calls. HW_Cars 6/7/8 votes
  disagree with the images; I chose the images.
* Sound mapping for 44-47 rests on path similarity and order; 12-17 is confirmed
  by the SoundSeq structure. The GC samples differ, so "same role" is the best
  available match.
* u8/u16 coverage is only as complete as Port_Swap's walkers, which list the
  fields the GC game reads as non-words. A u16 the game reads but Port_Swap
  missed would also be wrong on GC today. CarAStartSlave 0x30/0x32 was added
  here. The beta targets have none of the GC-only optional objects, so no
  unwalked types appear.
* Struct sizes in `walk.py` are lower bounds. Tails are inferred from the chunk
  partition (pointer targets), which is exact for swapping but not for naming.
* Interior pointer targets prevent a guaranteed 32-byte re-layout. If one is
  needed, treat any target inside a walker-known struct extent as interior.
* Beta camera type 1 semantics are unknown (no PS2 code); the donor-copy
  recommendation avoids needing them.
* Port_Swap does not walk the beta DestructibleInfo type-3 path, which reads
  geometry bounds. It depends on the converted geometry header keeping the bbox
  at the GC offset.
* `research/share_index_map.json` (another agent's `sharemap.py`) matches shared
  textures by best image only. For low-detail sprites (HW_share 0, 4, 12-14, ...)
  it disagrees with `import_map.json`. Where the two differ, prefer
  `import_map.json`: it is backed by structural path votes from 16 paired cars
  (`pathvotes.json`: the same slot in the same object on GC uses that index).
