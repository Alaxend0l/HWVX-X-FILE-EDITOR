# Components: which object subgraphs repeat across packs

Measurements behind the component kinds of [`README.md`](README.md) §13. Every pack of the PC
release was walked and its object subgraphs fingerprinted. A sample of the results was then
checked on the GameCube release (§5). Scripts are in [`scripts/`](scripts/) (§7).

## Recommendation

"Families": a world family is every world pack built on one world (`world1`, `world1l1..3`,
`ch1_l2..8` all name `w1_geo`). A family is 19 worlds in all. For cars, items and interface
packs, the family is the pack itself. So "repeats" means the same subgraph in different cars or
different worlds, not only in the level variants of one world. "Near-dup" means the same
structure with scalar fields that differ: it gives the median number of differing fields out of
the median field count.

| component kind | repeats (PC) | variants | near-dup | component? | why |
|---|---|---|---|---|---|
| **effect** (EventSeq with its particle emitters, particles, line effects, tracers, attached models, blasts) | 3,173 effects in 131 packs; 92% of instances use a variant found in 2+ families (max 39) | 255 | 105 variants; 6 / 1,209 fields | **yes** | 4.5 MB stored, 0.22 MB distinct. The game never writes to them. Make the EventSeq the component and keep emitters and particles inside it: ParticleEmitter has 706 variants in only 10 shapes, with a median of 8 of 20 fields differing, so the parts are too varied to be useful alone |
| **sound sequence** (SoundSeq + Sound) | 3,023 sequences in 313 packs, 92% shared | 316 (Sound: 455 in 3 shapes) | 3 / 16 (Sound) | **yes** | 0.8 MB stored, 42 KB distinct; read-only. A Sound differs mostly in its sample (0x14), volume (0x8) and type bytes |
| **car sound set** (CarASoundSet list) | 33 cars, 79% shared | 13 | 11/11 near-dups differ **only** in the samples (Sound 0x14) | **yes, as a template** | A component that a car extends with its own samples. Gears are the same: only the shift sound's sample and volume differ |
| **interface script** in worlds (pause menu, mission and race HUDs: IMHeader subtree) | 483 headers in 51 world packs, 96% shared; all 19 families | 59 | 37 variants; 8 / 834 | **yes** | **The largest gain: 20 MB stored, 0.27 MB distinct.** At runtime the game writes into it (§4), so compiled packs keep their own copy, as §13 already says |
| **car HUD** (CarAHud + its 3 IMHeaders) | 33 cars; the 3 headers are **1 template** for every car | 32 CarAHud | 31/31 differ only in **CarAHud 0x34** (4 stat-bar bytes) | **yes, extends** | 2.5 MB stored, 40 KB distinct. A car's HUD is the template plus 4 bytes. Same runtime-write rules as interface |
| state / object animation (StateAnim, ObjAnim) | 66 k / 35 k instances; **79% / 37% are copies inside the same pack** | 624 / 1,096 | 7 / 48 | **no** (as components); deduplicate within the pack | They are small (52 bytes) and part of the interface component. The extractor should fold identical tracks inside one pack, not create files for them. Dialogue code overwrites some of them (§4) |
| font | 623 in 107 packs, 100% shared | 14 | 4 / 20 | **yes** | Small, but referenced everywhere; one file per font |
| **world prop** (ThingRecord + DestructibleInfo + its geometry) | 3,450 records in 85 packs; 72% shared across families (max 14) | 662 (geometry in worlds: 1,163 variants, 837 of them in 2+ families) | 505 variants; 3 / 991 (worlds), mostly the model and DestructibleInfo boxes | **yes** | Object graph 5.6 MB stored, 0.5 MB distinct; world geometry 32.8 MB stored, 8 MB within families, 2.8 MB distinct. DestructibleInfo is written once as a lazy cache (§4) |
| AI setup (AICarScript + state tree) | 204 scripts in 61 packs, 91% shared; state infos 98% | 48 / 20 | 6 / 200 | **yes** | Read-only. The same scripts serve cars and world AI slots. TileCar (the slot: position, difficulty) is placement data, never repeated (165/165), so **no** |
| weapon: projectile, blast | 140 / 304 in 79 / 96 packs; 79% / 89% shared | 26 / 32 | 6 / 1,008; 7 / 48 | **yes** | The car weapon set (CarASlaveSet weapons) is unique per car (it carries mount points and models): **no**, but it references these |
| stunt setup (CarAStuntInfo) | 33 cars, 76% shared | 14 in 2 shapes | 12/12 near-dups differ in **0x0–0x8** only (3 of 1,824 fields) | **yes, extends** | 235 KB stored, 6 KB distinct: a few score values on one shared message and reward table |
| car tuning parts (CarAAccel, gears, cameras, force points, start slaves) | 33 cars; 8 / 15 / 24 / 4 / **1** variants | — | 1–2 fields | **no** (use `extends` from a base car) | A few KB in total; the differences are the tuning a modder edits. Start slaves are one variant used by every car |
| car body (CarAEngine, collider, wheels, model ref, motion anim, skeleton, car geometry) | never repeated (33/33; geometry 613/615) | — | collider 24 / 1,349 | **no** | Per car by nature. The car-select screen writes into `CarTrailBone.restLength` (§4) |
| pickup / item (ItemRecord, WeaponInfo) | 48 item packs, 0% repeat | 48 / 16 | 6 / 489 | **no** | Each item already has its own pack, which is the reuse unit; their effects and sounds are components |
| shader (animated texture) | 62 in 40 packs, 68% shared | 26 | 4 / 25 | **maybe** | 6.7 KB in total. Worth it only for the editor (pick a water or lava scroll); the runtime writes go into the GeometryGroup, not the shader (§4) |
| radar blip | 68 in 59 packs, 91% shared | 11 | 6 / 24 | **maybe** | Tiny; fold it into the effect or HUD component that uses it |
| TAC trigger / event template | 1,387 triggers in 51 world packs; **10%** shared (max 5 families) | 1,255 | 649 variants; 6 / 523 | **no** (as components) | Triggers are mission logic, written at runtime (state 0x24, action counts). Near-dups suggest editor *templates* ("new trigger like this one"), not shared files |
| world game data, tile paths, world AI slots | per world (50/51, 288/650, 165/165) | — | — | **no** | Only the pause menu inside world game data repeats, and that is an interface script |
| briefing audio (HWB_M*) | 165 sequences, 1 repeat | 164 | — | **no** | |
| world quadrant geometry (`w*_geo`), collision | not typed; nodes: geometry 34.3 MB → 27.7 MB distinct (19%); collision 0.7% | — | — | **maybe** (geometry), **no** (collision) | Race variants (`w1r1_geo`) reuse pieces of the main world; collision never repeats |

Biggest surprises:

* **Interface data is most of the duplication.** World packs carry the same menus and HUDs again
  and again (20 MB → 0.27 MB). A car's HUD is a fixed 3-screen template plus 4 bytes
  (2.5 MB → 40 KB). Particles and sounds, the first examples in §13, matter far less in bytes
  (≈ 7 MB stored in total).
* **Within car packs, most of the object graph repeats.** Geometry and textures excluded,
  the 33 CarAInfo subgraphs total 3.9 MB: 1.7 MB are copies inside the same pack (in-pack HUD
  and FX duplicates), 2.0 MB also appear in another car, and 0.23 MB are distinct. The 6% of
  §4.6 is the share of the whole blob, geometry included (car geometry never repeats).
* **Many duplicates sit inside one pack.** 80% of StateAnims, 28% of ParticleEmitters and 37%
  of AttachData are byte-equal copies of another object in the same pack. The original tools
  copied them rather than pointing to one.
* **Near-duplicates are real but have few clean cases.** HUD, stunt setup, sound set and gears
  differ in 1–3 fields: ideal for `extends`. Most effect parts differ in about 40% of their fields
  (emitters 8/20, particles 9/49), so a variant there is a new object, not an override.

## 1. Method

`comp_survey.py` reads every pack of an `hwvx-x extract` tree and does four things:

1. **Types.** It runs the Port_Swap walkers ported to Python
   (`../beta-cars/research/scripts/walk.py`, `walk_im.py`): car roots, the shape scans for event
   sequences, motion and key animations, and interface headers. It adds roots that were missing:
   the world (tile cars and AI scripts, quad statics → ThingRecord, FX points → EmitterData, tile
   paths, lights, creatures, WorldGameData with the pause menu and cameras), items (ItemRecord,
   WeaponInfo, shared slaves), audio sequences, TAC triggers (header `trigger_offset`) and shaders
   (`shader_offset`). Geometry roots come from the walkers plus local ref relocs (the share packs'
   export tables).
2. **Nodes.** The blob is cut at every pointer target (`pointers.bin` + local ref relocs). A node
   runs to the next target, or further if a walker knows its struct is larger.
3. **Fingerprints (Merkle).** *Deep* fingerprint: md5 of a node's bytes, with
   * pointer slots replaced by the target's deep fingerprint;
   * texture slots replaced by the texture's content hash (format, size, top level);
   * sound slots replaced by the sample's hash (rate, loop, PCM in host order);
   * imports from HW_share, the HW_Cars share pack and the HW_Items share pack resolved to the
     same content hashes.

   Position in the blob doesn't count. *Shape* fingerprint: the same with scalar words, texture and
   sound slots zeroed and models as one token. Back edges hash as a fixed token (at most 28 per
   pack).
4. **Instances.** Every object carrying a component label is an instance. Only *maximal*
   instances count, so an EventSeq nested in another EventSeq is not counted twice. Per kind:
   * packs, families, distinct deep (variants) and shape fingerprints;
   * variants seen in 2+ families;
   * in-pack copies;
   * bytes: the nodes reachable from the instances, without geometry and texture pixels. These
     are counted as stored, deduplicated within a family, and deduplicated across all packs.
     They overlap between kinds: a trigger's bytes include the interface it plays.

   For one instance per variant the subgraph is flattened (node label, offset, value) in DFS
   order. A near-duplicate is diffed against the most common variant of its shape.

Coverage of the walked blobs (PC): cars 9% textures, 78% geometry, 13% objects; worlds 58% / 21% /
21%; interface packs 95% textures / 5% objects. Next to nothing is left unreached (≤ 1%, audio 17%:
sequence headers).

## 2. Per kind, all PC packs

Columns: instances, packs, families, variants, shapes, variants in 2+ families, instances using
such a variant, in-pack copies, KB stored / deduplicated within a family / deduplicated across all
packs, near-duplicate variants, median differing fields / fields.

| kind | inst | packs | fam | var | shapes | var 2+ fam | inst in such | in-pack copies | KB stored / fam / all | near-dup var | diff / fields |
|---|---|---|---|---|---|---|---|---|---|---|---|
| EventSeq | 3173 | 131 | 99 | 255 | 174 | 141 | 92% | 0 | 4483 / 1392 / 222 | 105 | 6 / 1209 |
| ParticleEmitter | 15471 | 122 | 90 | 706 | 10 | 450 | 92% | 4379 | 1135 / 270 / 48 | 704 | 8 / 20 |
| Particle | 7446 | 122 | 90 | 454 | 86 | 289 | 91% | 9 | 1790 / 555 / 81 | 420 | 9 / 49 |
| SlaveTrigger[] | 1989 | 133 | 101 | 357 | 129 | 65 | 83% | 433 | 1525 / 887 / 266 | 263 | 4 / 268 |
| EmitterData | 620 | 90 | 58 | 43 | 36 | 23 | 90% | 4 | 411 / 150 / 26 | 12 | 4 / 117 |
| AttachData | 2303 | 126 | 94 | 322 | 39 | 68 | 83% | 866 | 422 / 162 / 32 | 307 | 4 / 40 |
| LineDef | 454 | 86 | 54 | 20 | 5 | 14 | 95% | 25 | 36 / 14 / 1.2 | 18 | 4 / 16 |
| Tracer | 203 | 37 | 36 | 8 | 4 | 4 | 98% | 66 | 36 / 21 / 1.1 | 6 | 5 / 40 |
| Blast | 304 | 96 | 64 | 32 | 10 | 10 | 89% | 0 | 159 / 131 / 10 | 28 | 7 / 48 |
| Projectile | 140 | 79 | 54 | 26 | 23 | 5 | 79% | 0 | 165 / 120 / 36 | 4 | 6 / 1008 |
| SoundSeq | 3023 | 313 | 281 | 316 | 63 | 110 | 92% | 0 | 801 / 324 / 42 | 275 | 5 / 28 |
| Sound | 7173 | 313 | 281 | 455 | 3 | 230 | 97% | 7 | 533 / 202 / 22 | 454 | 3 / 16 |
| CarASoundSet*[] | 33 | 33 | 33 | 13 | 2 | 6 | 79% | 0 | 3.6 / 3.6 / 1.2 | 12 | 1 / 28 |
| IMHeader | 603 | 105 | 73 | 83 | 59 | 50 | 94% | 0 | 25347 / 5496 / 729 | 37 | 8 / 834 |
| EventDef | 2161 | 71 | 39 | 34 | 17 | 21 | 99% | 1194 | 14275 / 2464 / 352 | 22 | 1 / 20 |
| StateAnim | 65757 | 107 | 75 | 624 | 62 | 552 | 100% | 52192 | 7908 / 887 / 91 | 602 | 7 / 48 |
| ObjAnim | 35078 | 107 | 75 | 1096 | 46 | 1007 | 100% | 12826 | 7589 / 1252 / 106 | 1083 | 6 / 52 |
| CarAHud | 33 | 33 | 33 | 32 | 1 | 1 | 6% | 0 | 2581 / 1261 / 40 | 32 | 1 / 20024 |
| Font | 623 | 107 | 75 | 14 | 7 | 14 | 100% | 0 | 103 / 67 / 2.5 | 9 | 4 / 20 |
| RadarBlip | 68 | 59 | 48 | 11 | 3 | 6 | 91% | 0 | 6.3 / 5.4 / 0.9 | 10 | 6 / 24 |
| Shader | 62 | 40 | 29 | 26 | 17 | 9 | 68% | 0 | 6.7 / 5.3 / 2.5 | 17 | 4 / 25 |
| AICarScript | 204 | 61 | 52 | 48 | 20 | 30 | 91% | 0 | 55 / 41 / 7.1 | 40 | 6 / 200 |
| AIStateInfo | 202 | 61 | 52 | 20 | 4 | 15 | 98% | 0 | 32 / 25 / 3.1 | 18 | 12 / 40 |
| TileCar | 165 | 51 | 19 | 165 | 7 | 0 | 0% | 0 | 52 / 41 / 19 | 164 | 7 / 360 |
| CarAInfo (whole car, no geometry) | 33 | 33 | 33 | 33 | 33 | 0 | 0% | 0 | 3887 / 2226 / 233 | 0 | – |
| CarAStuntInfo | 33 | 33 | 33 | 14 | 2 | 6 | 76% | 0 | 235 / 152 / 6.1 | 14 | 3 / 1824 |
| CarACollider | 33 | 33 | 33 | 33 | 2 | 0 | 0% | 0 | 174 / 135 / 18 | 32 | 24 / 1349 |
| CarTrailBone[] | 33 | 33 | 33 | 33 | 1 | 0 | 0% | 0 | 10 / 10 / 10 | 33 | 14 / 80 |
| CameraSubMode[] | 66 | 33 | 33 | 24 | 2 | 6 | 73% | 0 | 7.8 / 7.8 / 2.2 | 24 | 2 / 36 |
| CarAGear[] | 33 | 33 | 33 | 15 | 2 | 8 | 79% | 0 | 3.9 / 3.8 / 1.4 | 14 | 2 / 28 |
| CarAAccel | 33 | 33 | 33 | 8 | 1 | 6 | 94% | 0 | 3.1 / 3.1 / 0.8 | 8 | 1 / 24 |
| CarAStartSlave[] | 33 | 33 | 33 | 1 | 1 | 1 | 100% | 0 | 3.1 / 3.1 / 0.1 | 0 | – |
| CarASlaveSet[] (FX / weapons) | 33 / 33 | 33 | 33 | 33 / 33 | 33 | 0 | 0% | 0 | 450 / 345 / 88; 422 / 341 / 56 | – | – |
| ItemInfo / WeaponInfo | 48 / 16 | 48 / 16 | | 48 / 16 | 32 / 16 | 0 | 0% | 0 | 197 / 179 / 112; 116 / 104 / 91 | 21 | 6 / 489 |
| ThingRecord | 3450 | 85 | 53 | 662 | 195 | 163 | 72% | 53 | 5601 / 1446 / 504 | 505 | 7 / 1023 |
| DestructibleInfo | 3449 | 84 | 52 | 277 | 97 | 146 | 83% | 829 | 3871 / 842 / 190 | 196 | 11 / 918 |
| Trigger | 1387 | 51 | 19 | 1255 | 730 | 26 | 10% | 18 | 25130 / 11776 / 9693 | 649 | 6 / 523 |
| WorldGameData | 51 | 51 | 19 | 50 | 46 | 0 | 0% | 0 | 28783 / 9758 / 6176 | 7 | 14 / 74148 |
| TilePath | 650 | 36 | 10 | 288 | 30 | 0 | 0% | 0 | 518 / 101 / 68 | 269 | 32 / 975 |
| Geometry (packs with objects) | 17877 | 129 | 97 | 1862 | 1157 | 859 | 87% | 115 | 57714 / 32167 / 25988 | 922 | – |
| Geometry in car packs | 615 | 34 | 34 | 613 | 490 | 2 | 1% | 0 | 23977 / 23235 / 22818 | – | – |
| Geometry in world packs | 17131 | 51 | 19 | 1163 | 611 | 837 | 90% | 115 | 32763 / 7983 / 2758 | – | – |
| MotionAnim / Skeleton | 261 / 207 | 94 | 63 | 260 / 190 | | 0 | 0% | | 1396 / 622 / 393; 178 / 165 / 160 | | 2 / 3206; 1 / 72 |
| KeyAnim | 641 | 98 | 67 | 227 | 110 | 8 | 9% | 87 | (see §6) | 138 | 20 / 76 |

Per category (cars, worlds, items/share, interface, audio), the same table comes from
`comp_summary.py`. By category:
* **Effects** repeat across cars (all 33 cars share the same 2 projectiles, 5 blasts, 4 tracers and
  6 EmitterData variants) and across worlds (EventSeq 3.6 MB → 0.18 MB).
* **Item packs** share little with each other (EventSeq 66%, AttachData 18%), but use the same
  sounds and particles as cars and worlds.
* **Variants shared across categories:** 174 ObjAnim, 123 StateAnim, 42 Sound, 29 Particle,
  28 ParticleEmitter, 21 geometry, 17 SoundSeq, 9 AI states, 8 fonts. So a component library
  should be global, not per category.

## 3. What near-duplicates differ in

The most frequent differing fields, against the most common variant of the same shape:

| kind | fields that differ (count of near-dup variants) | `extends` fit |
|---|---|---|
| CarAHud | 0x34 (31/31): the stat bars | perfect |
| CarASoundSet / CarAGear | Sound 0x14 sample (11/11, 13/13), Sound 0x8 volume (gears) | perfect: same set, other samples |
| CarAStuntInfo | 0x0, 0x4, 0x8 (12/12) | perfect |
| CarAAccel | 0x14 (6/7) | perfect |
| Camera lists | type-4 params 0x4/0x8, type-0 params 0x4 | good |
| AICarScript | 0x0, 0xC, 0x14, 0x18, 0x1C, 0x24 (≈ 6 of 50 words) | good |
| Sound | 0x14 sample, 0x8 volume, 0x0 type bytes, 0xC pitch range, range array | good (3 of 16) |
| ThingRecord | the model (273 of 467), DestructibleInfo boxes and material | fair: same prop, other model |
| ParticleEmitter, Particle | 0x8/0xC/0x18–0x24 rates and ranges, colour keys, anim params | poor (40% of fields) |
| StateAnim / ObjAnim | key positions and colours | poor; they are keyframe data |
| TileCar | position 0x0–0x14, vehicle 0x30, far distance 0x3C | not a component (placement) |
| Trigger | type/param/action data | editor templates at most |

## 4. Runtime writes: which shared subgraphs need per-pack copies

Stores into pack data in `HWVX-SDL/src/game`, found by an audit of typed stores, offset stores and
`memcpy`/`memset` into pack pointers. They confirm §13's rule: components are a source-level idea,
and the compiled pack inlines its own copy. Read-only, so sharing in memory would also be safe:

* effects (EventSeq, emitters, particles, projectile, blast, tracer, attach, line, radar blip,
  light);
* Sound and SoundSeq;
* AI scripts and state trees;
* ItemRecord, WeaponInfo, CreatureInfo;
* MotionAnim, KeyAnim, Skeleton (AnimNode flags are written only on a stack copy);
* shader definitions;
* fonts, IfaceRecord;
* car parts other than the two below.

Written at runtime (a per-pack copy is needed):

| struct | field | kind of write |
|---|---|---|
| SharedSlaves (type-10 slave data) and its first SlaveTrigger | `active` 0x0 = 1, `holder` 0x1D (ThingManager.c:371-372, FileStreaming.c:312-313) | per instance, never reset. **This applies to slave lists inside effect components that contain a type-10 entry** |
| IfaceVar | `offset` 0x0C, saved menu selection (InterfaceManager.c:249) | persistent state |
| IfaceEventDef | `id` 0x06 (DropShip timeout, :5123/:5146) | state |
| IfaceObjectDef | `stateIndex` 0x18 (SetObjectState) | idempotent |
| IfaceDialogChar, IfaceStateAnim, IfaceObjAnim, IfaceText | dialogue playback state; StateAnim/ObjAnim key counts and first keys overwritten per dialogue character (:2739-2865) | per dialogue. Shared state/obj anims would leak between dialogues |
| mission objective strings | `#` replaced by digits in place (:5221-5268) | mutates string bytes: never deduplicate strings across objects |
| interface headers, objects, events (menu patches) | `Port_Menu.c`, `Port_Carousel.c` in-place patches | port patches, run on each screen create |
| GeometryGroup / uvScroll | `textureOverride` 0x18, scroll, colour, every frame (ShaderHolder.c:131-143) | per shader. Geometry shared between packs would follow one pack's shader |
| geometry, texture headers (load) | Port_Geometry / Port_Data / Port_PS2 conversions; arrays in blocks owned by the pack; PC texture header swap is **not idempotent** | per pack |
| DestructibleInfo | `type` 3 → 0, box 0x44 from geometry bounds (Destructible.c:118-153) | lazy cache, deterministic |
| CarTrailBone | `restLength` 0x0C = 0.3 on the car-select screen (GameSpecificInterfaces.c:324) | changes the car while its pack is loaded |
| CarAEngine | 0x0C = 1.0 (CarA.c:295) | idempotent |
| world: TileCar, TileCreature, quads and quad contents, triggers and actions, variables, timers, objectives, nav points, volumes, teleporters, tile paths, vertex-colour sequences, collision sweep stamps | many | game state: world data is never shared |

## 5. GameCube check

The same run on the GameCube tree (`--rel gc`; 346 packs, its 6 extra `race*l1` worlds included)
agrees kind by kind:

| kind | PC variants | GC variants | PC / GC % instances in a variant shared by 2+ families |
|---|---|---|---|
| EventSeq | 255 | 257 | 92 / 92 |
| Particle / ParticleEmitter | 454 / 706 | 456 / 705 | 91 / 92, 92 / 92 |
| Sound / SoundSeq | 455 / 316 | 457 / 315 | 97 / 97, 92 / 93 |
| IMHeader | 83 | 108 | 94 / 86 (release-only HUD text, see README §8.1) |
| CarAHud | 32, 1 shape, only 0x34 differs | the same | |
| ThingRecord / DestructibleInfo | 662 / 277 | 689 / 279 | 72 / 72, 83 / 84 |
| Geometry | 1,862 | 1,870 | 87 / 87 |
| AICarScript / Trigger | 48 / 1,255 | 48 / 1,303 | 91 / 92, 10 / 10 |

Byte totals differ only through the GameCube's 32-byte alignment (e.g. interface scripts, all packs:
41 MB stored → 1.1 MB distinct, against 25 MB → 0.7 MB on PC). Untyped quad geometry repeats a little
less on GC (22.9 MB → 20.2 MB).

## 6. What was not walked or classified

* **Collision packs and world quadrant geometry** (`w*_col`, `w*_geo`, the battle worlds' geometry)
  have no typed walk, only the untyped node pass (`comp_quads.py`, numbers in the table).
* **The MCP script pack** (`mcp/link`) is not walked by type: the MCP walker is not ported.
  Briefing **dialogue lists** (`DialogList`) are labelled but not followed.
* **Untyped world structures** are inside some kind's closure but are not kinds themselves, so
  they are not reported separately: volumes, nav nodes, visibility BSP, speakers, teleporters,
  variables, timers, objectives, quad contents. No PC world has tile creatures or tile lights, so
  CreatureInfo and LightDef have no instances. `TileCar.info` is 0 in the data: the game resolves it
  into a car pack at load.
* **Strings** are not a kind. Interface text is mostly per release (README §8.1).
* **Heuristic types.** EventSeqs, motion animations and key animations found by shape scan
  (as in Port_Swap) could be missed or misread. Node extents come from pointer targets. Where an
  object has interior targets, or its data is followed by an untargeted block, a node can include
  unrelated bytes. This is visible in PC **KeyAnim** bytes: 18 MB stored in worlds against 1.4 MB
  on GC, most likely because some PC key data is followed by a block no pointer targets (not traced further). Variant counts are not affected
  in a way that matters (PC 227, GC 184).
* **Cycles**: at most 28 back edges per pack, hashed as one token, so two subgraphs that differ only
  behind a back edge would count as equal.
* Sound samples and texture pixels are compared by exact content only. The README §4.6 figures
  for them stand.

## 7. Scripts

| script | does |
|---|---|
| `comp_survey.py SURVEY_ROOT [--rel pc\|gc] [--only ARCH,...]` | walk, fingerprint and record instances. Writes `SURVEY_ROOT/components/<rel>_instances.jsonl`, `<rel>_variants.pkl` (flattened variants) and `<rel>_bytes.json` (per-kind bytes, coverage). About 3 min per release |
| `comp_summary.py SURVEY_ROOT [--rel pc] [--kind LABEL]` | the tables of §2 per category, coverage, and variants shared across categories; `--kind` prints one kind's detail |
| `comp_quads.py SURVEY_ROOT [--rel pc]` | untyped node repetition in quad-geometry and collision packs |

They use `survey_common.py` and the research walkers in `../beta-cars/research/scripts/`. Run them
with plain `python3`. Scratch output is in `HWVX-SDL/build-survey/components/`.
