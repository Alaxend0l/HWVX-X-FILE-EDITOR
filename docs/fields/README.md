# Pack-data struct fields

What each field of the structs that pack data is made of is called, how sure the name is, and
which release reads it. The xdl JSON schema (`docs/xdl-format/README.md` §10.4) is generated from
these structs: named fields become JSON keys, fields read but not understood keep an offset name
(`unk_0x38`), and fields no release reads are left out of the JSON and taken from the base object.

Source of truth: the port's headers (`HWVX-SDL/src/game/include/*.h`, branch `field-names`). This
table is generated from them and from the load scans described below; regenerate it when a header
changes.

## How the columns were filled

* **GC / PS2 / PC**: does the release's code read the field?
  * `yes`: a load of it was found. GameCube: every named field (the port is the GameCube code, and
    names only what it uses) plus the scan; PS2 and PC: the scan.
  * `no`: checked and never read. For pads that means the port never touches them (GameCube) and no
    load through the struct's pointers reads them in the PS2 asm (`HWVX-PS2-Decomp/asm`, named
    functions) or the PC exe (`orig/pc-backup/Hot Wheels.exe`, disassembled).
  * `?`: not seen by the scan. The scan follows the struct's usual pointer path (e.g.
    `CarA.vehicleIface`, 0x3BC on GameCube/PS2 and 0x398 on PC, then `->engine` 0x80) register by
    register, so a field read through another path (a function argument, a copy on the stack, an
    array index) can be missed. `?` on a named field does not mean the release ignores it.
* **Confidence** (of the name): `certain` when the use leaves no doubt (a count, a pointer, a value
  passed straight to a named parameter), `likely` when the name follows from one or two uses, `guess`
  when it is a reading of the data rather than the code. Names that predate this pass were not all
  re-reviewed: counts and pointers are listed `certain`, the rest `likely`. `-`: unnamed pad.
* **Notes**: the header comment, which for pads gives the values found in every GameCube pack
  (33 cars, the worlds, 49 items, read with `build-survey`).
* The scan tool: `HWVX-SDL/build-fields/scratch/scan.py` (GC PPC, PS2 MIPS and PC x86, a pointer
  chain or a function argument as the seed), `gen_doc.py` writes the tables below.

## Findings

* **CarAInfo 0x14 is the skid mark life, not a texture** (`skidLife`): it is passed as
  `Trail_AddSegment`'s `life`, stored `<< 4` ms; 600 = 9.6 s in every car. **0x50 is the skid
  texture** (`skidMarkTexture`, a texture-reloc slot), not an enable flag. The surface's u16 at 0x2
  (type 2) overrides the life, not the texture.
* **CarAEngine 0x38 / 0x5C** (`maxImpactVel`, `damageScale`) were named only in a local overlay in
  `ThingManager.c`; all three releases read them. `damageScale` is per-car bumper damage tuning,
  0.04-0.15.
* **PC-only data uses** (the PC port's creature damage and third handling mode):
  * WorldGameData 0x70/0x74/0xF8/0xFC: effects of a car running a creature over, picked by speed
    like the car-car `impactFx` (5..100, two ids in every world); 0xE8/0xEC: creature hit effects,
    `[2]` under 60 damage, `[3]` from 60 (no world has any, so it never fires).
  * CreatureInfo 0x3C/0x40: a death effect list, `[0]` spawned when the creature dies.
  * CarAAccel 0x08 (`coastRpmScale`, 1.5): driveMode 2 off the throttle. **No car in HW_Cars is
    driveMode 2** (all 33 are 1, on GameCube and PC) and no code writes driveMode, so the PC's third
    handling mode needs a CarAInfo from another pack (not checked) or a mod.
* **PS2-only**: CreatureInfo 0x34 is a SlaveList that PS2's `Creature_Init` starts.
* **Data nobody reads**: WorldGameData 0x40-0x48 (0.3, 80, 0.075 in every world) and four texture
  slots (0x80, 0x84, 0x94, 0x98); CarAInfo 0x28 (f32 1000), two sound sets (0x5C, 0x64), a
  {110², 30²} record (0xAC) and a texture slot (0x44); CarAEngine 0x60 (the same record as CarAInfo
  0x4C); CarACollider 0x3C/0x40 (a second list like `endFx`); CarAStuntInfo 0x10/0x20/0x24 (0.5, 1,
  5); ItemRecord 0x08 (800, 5000, 300: ms?).

## Not covered

* **WorldQuad**: skipped (its pads overlap `QuadPack`'s loader fields, which the xdl work is
  changing). Candidates: 0x04-0x0B, 0x14-0x1B and 0x28-0x2F (the quad pack header,
  `QuadPack.header`/`gxBase` at 0x28/0x2C), 0x36, 0x44-0x4F.
* DataPack, DataPackRef, DataPackFileHeader, QuadPack and the geometry/LOD/group structs, for the
  same reason.
* Wheels (`CarTrailBone`), AI scripts (`AICarScript`, `AIStateInfo`), `DestructibleInfo`,
  `FlashDef` and the world tile structs: reached through arrays the scan doesn't follow yet.

### CarAInfo

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `viewOffset` | f32[3] | likely | yes | yes | yes | shifts the car when an interface shows it (InterfaceManager_RenderObject) |
| 0x0C | `pad0C` | u8[4] | - | no | no | no | 0 in every car; never read (GC, PC, PS2) |
| 0x10 | `damagePercent` | u8 | likely | yes | yes | yes | health % below which parts break off |
| 0x11 | `driveMode` | u8 | likely | yes | yes | yes | 1: human-controlled |
| 0x12 | `pad12` | u8[2] | - | no | no | no | two u8; never read (GC, PC, PS2) |
| 0x14 | `skidLife` | u16 | certain | yes | yes | yes | skid mark life in 16 ms units (Trail_AddSegment life << 4): 600 (9.6 s) in every car |
| 0x16 | `pad16` | u8[2] | - | no | no | no | u16, 0 in every car; never read (GC, PC, PS2) |
| 0x18 | `damageLevel` | s32 | likely | yes | yes | yes | hits above it break parts off |
| 0x1C | `maxRpm` | f32 | likely | yes | yes | yes | max rpm (torque curve, engine pitch) |
| 0x20 | `upshiftRpm` | f32 | likely | yes | yes | yes | shift up above it; also caps reverse rpm |
| 0x24 | `downshiftRpm` | f32 | likely | yes | yes | yes | shift down below it |
| 0x28 | `pad28` | u8[8] | - | no | no | no | f32 1000, u32 18 (21 in car 23); never read (GC, PC, PS2) |
| 0x30 | `turboRpmScale` | f32 | likely | yes | yes | yes | 4WD (turbo): rpm divisor |
| 0x34 | `turboTorqueScale` | f32 | likely | yes | yes | yes | 4WD (turbo): torque factor |
| 0x38 | `pad38` | u8[0x10] | - | no | no | no | u32 1, u32 16 (1 in cars 0, 26), u32 0, texture slot 0x44 (relocated by the loader); never read (GC, PC, PS2) |
| 0x48 | `steerAnim` | ptr | certain | yes | yes | yes |  |
| 0x4C | `pad4C` | u8[4] | - | no | no | no | pointer to a 0x40-byte record; never read (GC, PC, PS2) |
| 0x50 | `skidMarkTexture` | u32 | certain | yes | yes | yes | texture slot of the skid marks (Trail_AddSegment), 0: none |
| 0x54 | `reflectionGeom` | ptr | certain | yes | yes | yes | drawn under human cars in the garage (state 5) |
| 0x58 | `pad58` | u8[8] | - | no | no | no | 0, CarASoundSet pointer; never read (GC, PC, PS2) |
| 0x60 | `rollSound` | ptr | certain | yes | yes | yes | CarA.soundIds[2] |
| 0x64 | `pad64` | u8[8] | - | no | no | no | CarASoundSet pointer (car 16 only), 0; never read (GC, PC, PS2) |
| 0x6C | `skidColor` | ptr | certain | yes | yes | yes |  |
| 0x70 | `pad70` | u8[8] | - | no | no | no | 0 in every car; never read (GC, PC, PS2) |
| 0x78 | `modelRef` | ptr | certain | yes | yes | yes |  |
| 0x7C | `gripScale` | ptr | certain | yes | yes | yes | initial CarA.gripScale |
| 0x80 | `engine` | ptr | certain | yes | yes | yes |  |
| 0x84 | `hud` | ptr | certain | yes | yes | yes |  |
| 0x88 | `accel` | ptr | certain | yes | yes | yes |  |
| 0x8C | `stunt` | ptr | certain | yes | yes | yes |  |
| 0x90 | `aiScripts` | u32[4] | certain | yes | yes | yes | AICarScript handles, per difficulty |
| 0xA0 | `collider` | ptr | certain | yes | yes | yes |  |
| 0xA4 | `forcePoints` | ptr | certain | yes | yes | yes | extra force points (CarA_ComputeForces) |
| 0xA8 | `padA8` | u8[8] | - | no | no | no | 0, pointer to {f32 12100 (110^2), f32 900 (30^2), 0...}; never read (GC, PC, PS2) |
| 0xB0 | `explosionThing` | u32 | likely | yes | yes | yes |  |
| 0xB4 | `padB4` | u8[0x1C] | - | no | no | no | 0 in every car; never read (GC, PC, PS2) |
| 0xD0 | `numWheels` | u32 | certain | yes | yes | yes |  |
| 0xD4 | `wheels` | ptr | certain | yes | yes | yes |  |
| 0xD8 | `numGears` | u32 | certain | yes | yes | yes |  |
| 0xDC | `gears` | ptr | certain | yes | yes | yes | torque curve table |
| 0xE0 | `numSlaves` | u32 | certain | yes | yes | yes |  |
| 0xE4 | `slaves` | ptr | certain | yes | yes | yes | [numSlaves] |
| 0xE8 | `numSlaveWeapons` | u32 | certain | yes | yes | yes |  |
| 0xEC | `slaveWeapons` | ptr | certain | yes | ? | yes |  |
| 0xF0 | `numPickupSlaves` | u32 | certain | yes | yes | yes |  |
| 0xF4 | `pickupSlaves` | ptr | certain | yes | yes | yes | indexed by item value |
| 0xF8 | `padF8` | u8[8] | - | no | no | no | u32 1, pointer to 12-byte {0, pointer, 0}; never read (GC, PC, PS2) |
| 0x100 | `numStartSlaves` | u32 | certain | yes | yes | yes |  |
| 0x104 | `startSlaves` | ptr | certain | yes | yes | yes |  |
| 0x108 | `numStartItems` | u32 | certain | yes | yes | yes |  |
| 0x10C | `startItems` | ptr | certain | yes | ? | yes | put into the wildcard rack |
| 0x110 | `numShiftSounds` | u32 | certain | yes | yes | yes |  |
| 0x114 | `shiftSounds` | ptr | certain | yes | yes | yes |  |
| 0x118 | `numSoundSets` | u32 | certain | yes | yes | yes |  |
| 0x11C | `soundSets` | ptr | certain | yes | yes | yes |  |

### CarAEngine

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `bodyOffset` | f32[3] | likely | yes | yes | yes | rest value of CarA.bodyOffset |
| 0x0C | `unused0C` | f32 | - | no | yes | yes | CarA_Init sets 1, never read |
| 0x10 | `selfRightDelay` | s32 | likely | yes | yes | yes | ms airborne before the self-righting torque |
| 0x14 | `steerRate` | f32 | likely | yes | yes | yes | rad/s the steering angle ramps to the input |
| 0x18 | `reverseDelay` | u32 | likely | yes | yes | yes | ms reverse before the car answers the throttle |
| 0x1C | `pad1C` | u8[4] | - | no | no | no | never read (GC, PC, PS2) |
| 0x20 | `selfRightArm` | f32 | likely | yes | yes | yes |  |
| 0x24 | `torqueCurve` | f32[4] | likely | yes | yes | yes | torque at 0 and max rpm, slopes there (CarA_FitCubic) |
| 0x34 | `animSpeed` | f32 | likely | yes | yes | yes | scales the steering animation time |
| 0x38 | `maxImpactVel` | f32 | certain | yes | yes | yes | car-car impacts cap the closing speed at it, scaled by CarA_GetImpactMultiplier (ThingManager_MoveThings);... |
| 0x3C | `maxAngularVel` | f32 | likely | yes | yes | yes |  |
| 0x40 | `itemDropImpact` | f32 | likely | yes | yes | yes |  |
| 0x44 | `maxGrade` | f32 | likely | yes | yes | yes | grade the car can hold, times turboRpmScale in turbo |
| 0x48 | `pad48` | u8[0x10] | - | no | no | no | never read (GC, PC, PS2) |
| 0x58 | `tiltForceScale` | f32 | likely | yes | yes | yes | force point scale by up.y |
| 0x5C | `damageScale` | f32 | certain | yes | yes | yes | car-car bumper damage per unit of closing speed (0.04-0.15 by car) |
| 0x60 | `pad60` | u8[4] | - | no | no | no | pointer to the record CarAInfo 0x4C points to; never read (GC, PC, PS2) |
| 0x64 | `minEnginePitch` | f32 | likely | yes | yes | yes | semitones added at 0 rpm |
| 0x68 | `soundRpmRange` | f32 | likely | yes | yes | yes | rpm per pitch step, CarA_UpdateSound gate |
| 0x6C | `offsetContactFlag` | f32 | likely | yes | yes | yes | 1: the spring scale keeps wheel->offset |
| 0x70 | `itemHeight` | f32 | likely | yes | yes | yes | picked-up items ride this high above the rack |
| 0x74 | `pad74` | u8[0xC] | - | no | no | no | never read (GC, PC, PS2) |
| 0x80 | `cameras` | CarACameras | likely | yes | yes | ? |  |
| 0x88 | `splitCameras` | CarACameras | likely | yes | ? | ? | used when the screen is split |

### CarAAccel

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `pad00` | u8[4] | - | no | no | no | 0 in every car; never read (GC, PC, PS2) |
| 0x04 | `driveRpmScale` | f32 | likely | yes | yes | yes | drive friction to rpm scale |
| 0x08 | `coastRpmScale` | f32 | likely | no | no | yes | PC only: driveMode 2 off the throttle uses it in place of driveRpmScale (ComputeSuspensionForces... |
| 0x0C | `pad0C` | u8[8] | - | no | no | no | 0 in every car; never read (GC, PC, PS2) |
| 0x14 | `tractionLift` | f32 | likely | yes | yes | yes | traction point lift: (this - 1) * up * contact height |
| 0x18 | `brakeDecel` | f32 | likely | yes | yes | yes | rpm/s at full brake |
| 0x1C | `engineBrake` | f32 | likely | yes | yes | yes | rpm/s off the throttle |
| 0x20 | `reverseThrottleDecel` | f32 | likely | yes | yes | yes | rpm/s at full reverse throttle |
| 0x24 | `airDownforceBase` | f32 | likely | yes | yes | yes |  |
| 0x28 | `airDownforceRate` | f32 | likely | yes | yes | yes |  |
| 0x2C | `airDownforceMax` | f32 | likely | yes | yes | yes |  |
| 0x30 | `airDownforceDelay` | s32 | likely | yes | yes | yes | ms |
| 0x34 | `spinImpulse` | f32 | likely | yes | yes | yes |  |
| 0x38 | `spinDecay` | f32 | likely | yes | yes | yes |  |

### CarACollider

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `inertiaScale` | f32[3] | likely | yes | yes | yes | per-axis factor of the inverse inertia tensor |
| 0x0C | `pad0C` | u8[4] | - | no | no | no | 0 in every car; never read (GC, PC, PS2) |
| 0x10 | `type` | u8 | likely | yes | yes | yes | 0: oriented box, 1: convex hull |
| 0x11 | `pad11` | u8[3] | - | no | no | no | u8s (1, 0, 0 in most cars); never read (GC, PC, PS2) |
| 0x14 | `maxHealth` | s32 | likely | yes | yes | yes |  |
| 0x18 | `damageThreshold` | s32 | likely | yes | yes | yes | impacts above it hurt |
| 0x1C | `pad1C` | u8[4] | - | no | no | no | s32 375 or -1; never read (GC, PC, PS2) |
| 0x20 | `mass` | f32 | likely | yes | yes | yes |  |
| 0x24 | `gravityScale` | f32 | likely | yes | yes | yes | forceAccum.y += gravityScale * mass |
| 0x28 | `pad28` | u8[4] | - | no | no | no | 0 in every car; never read (GC, PC, PS2) |
| 0x2C | `gravity` | f32 | likely | yes | yes | yes |  |
| 0x30 | `material` | ptr | certain | yes | yes | yes |  |
| 0x34 | `numEndFx` | u32 | certain | yes | ? | ? | debris spawned when destroyed, one at random |
| 0x38 | `endFx` | ptr | certain | yes | ? | ? | (Thing_ProcessEndEvent) |
| 0x3C | `pad3C` | u8[8] | - | no | no | no | a second list like endFx: u32 1, pointer; never read (GC, PC, PS2) |
| 0x44 | `box` | ptr | certain | yes | yes | yes | type 0: min/max corners |
| 0x48 | `spheres` | ptr | certain | yes | yes | yes | type 7 |
| 0x48 | `hull` | ptr | certain | yes | yes | yes | type 1 |

### CarAStuntInfo

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `pitchRate` | f32 | likely | yes | yes | yes |  |
| 0x04 | `rollRate` | f32 | likely | yes | yes | yes |  |
| 0x08 | `spinRate` | f32 | likely | yes | yes | yes |  |
| 0x0C | `maxTime` | f32 | likely | yes | yes | yes |  |
| 0x10 | `pad10` | u8[4] | - | no | no | no | f32 0.5 in every car; never read (GC, PC, PS2) |
| 0x14 | `turboPerPoint` | f32 | likely | yes | yes | yes |  |
| 0x18 | `pitchScore` | f32 | likely | yes | yes | yes |  |
| 0x1C | `rollScore` | f32 | likely | yes | yes | yes |  |
| 0x20 | `pad20` | u8[8] | - | no | no | no | f32 1, f32 5 in every car; never read (GC, PC, PS2) |
| 0x28 | `startDelay` | s32 | likely | yes | yes | yes | ms airborne before the stunt can start |
| 0x2C | `minFailTime` | f32 | likely | yes | yes | yes | also the stunt-time range landed stunts score in |
| 0x30 | `maxFailTime` | f32 | likely | yes | yes | yes |  |
| 0x34 | `numRewardThings` | u32 | certain | yes | yes | yes |  |
| 0x38 | `rewardThings` | ptr | certain | yes | yes | yes | picked by score |
| 0x3C | `numMessages` | u32 | certain | yes | yes | yes |  |
| 0x40 | `messages` | ptr | certain | yes | ? | yes | picked by score: string tables |
| 0x44 | `pad44` | u8[4] | - | no | no | no | 0 in every car; never read (GC, PC, PS2) |
| 0x48 | `failMessage` | ptr | certain | yes | yes | yes | NULL: g_stuntLossString |
| 0x4C | `numFailThings` | u32 | certain | yes | yes | yes |  |
| 0x50 | `failThings` | ptr | certain | yes | yes | yes | thing ids, picked by fail time |
| 0x54 | `spinScore` | f32 | likely | yes | yes | yes |  |

### CarAHud

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `pad00` | u8[0x10] | - | no | no | no | 0 (car 16: f32 -0.35, 15 at 0x04); never read (GC, PC, PS2) |
| 0x10 | `layout1` | u32 | likely | yes | yes | yes | one viewport |
| 0x14 | `pad14` | u8[4] | - | no | no | no | 0 in every car; never read (GC, PC, PS2) |
| 0x18 | `layout2` | u32 | likely | yes | yes | yes | two |
| 0x1C | `layout4` | u32 | likely | yes | yes | yes | three or four |
| 0x20 | `pad20` | u8[0x14] | - | no | no | no | 0, a pointer at 0x2C; never read (GC, PC, PS2) |
| 0x34 | `stats` | u8[4] | likely | yes | yes | yes | vehicle select bars (InterfaceHudInfo.carStats) |

### ProjectileInfo

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `draw` | u8 | likely | yes | yes | yes | 1: drawn |
| 0x01 | `drawMode` | u8 | likely | yes | yes | yes | 0: geometry, 1: billboard sprite |
| 0x02 | `pad02` | u8 | - | no | no | no | never read (GC, PC, PS2) |
| 0x03 | `collideMode` | u8 | likely | yes | yes | yes | on a hit (Projectile_DynamicCollisionCorrection): 0, 3 impact effect + release (3 ignores things), 1, 4... |
| 0x04 | `trailMode` | u8 | likely | yes | yes | yes | trail effect spacing: 0 none, 1 time, 2 distance |
| 0x05 | `guidance` | u8 | likely | yes | yes | yes | 1, 2: smart (homing), 3: aimed at a car ahead |
| 0x06 | `pad06` | u8 | - | no | no | no | never read (GC, PC, PS2) |
| 0x07 | `proximityAttach` | u8 | likely | yes | yes | yes | proximity effect sticks to the car hit |
| 0x08 | `impactOrient` | u8 | likely | yes | yes | yes | impact effect orientation: 0 projectile's, 1 surface, 2 reflected |
| 0x09 | `pad09` | u8[3] | - | no | no | no | u8s; never read (GC, PC, PS2) |
| 0x0C | `trailSpacing` | ptr | certain | yes | yes | yes | [base, range]: u32 ms (trailMode 1) or f32 distance (2) |
| 0x10 | `geometry` | ptr | certain | yes | yes | yes | drawMode 0 |
| 0x14 | `lifeBase` | s32 | likely | yes | yes | yes | ms |
| 0x18 | `lifeRange` | s32 | likely | yes | yes | yes |  |
| 0x1C | `damageBase` | s32 | likely | yes | yes | yes |  |
| 0x20 | `damageRange` | s32 | likely | yes | ? | ? |  |
| 0x24 | `pad24` | u8[8] | - | no | no | no | never read (GC, PC, PS2) |
| 0x2C | `mass` | f32 | likely | yes | yes | yes |  |
| 0x30 | `recoil` | f32 | likely | yes | yes | yes | impulse scale applied to the firing car |
| 0x34 | `proximityRadius` | f32 | likely | yes | yes | yes | 0: no proximity fuse |
| 0x38 | `proximityInterval` | s32 | likely | yes | yes | yes | ms between proximity checks |
| 0x3C | `pad3C` | u8[4] | - | no | no | no | never read (GC, PC, PS2) |
| 0x40 | `thrust` | f32 | likely | yes | yes | yes | acceleration along the nose |
| 0x44 | `maxSpeed` | f32 | likely | yes | yes | yes |  |
| 0x48 | `pad48` | u8[4] | - | no | no | no | never read (GC, PC, PS2) |
| 0x4C | `holeSizeBase` | f32 | likely | yes | ? | yes | bullet hole on terrain |
| 0x50 | `holeSizeRange` | f32 | likely | yes | yes | yes |  |
| 0x54 | `holeLifeBase` | s32 | likely | yes | yes | yes |  |
| 0x58 | `holeLifeRange` | s32 | likely | yes | yes | yes |  |
| 0x5C | `holeTexture` | u32 | likely | yes | yes | yes | 0: no hole |
| 0x60 | `sprite` | ptr | certain | yes | yes | yes | drawMode 1: first byte is the blend mode |
| 0x64 | `lensFlare` | ptr | certain | yes | yes | yes | LensFlare_Add |
| 0x68 | `light` | ptr | certain | yes | yes | yes | LightHolder_Add |
| 0x6C | `pad6C` | u8[4] | - | no | no | no | never read (GC, PC, PS2) |
| 0x70 | `trailFx` | u32 | likely | yes | yes | yes | DebrisParams.debris, 0: none |
| 0x74 | `impactFx` | u32 | likely | yes | yes | yes |  |
| 0x78 | `expireFx` | u32 | likely | yes | yes | yes |  |
| 0x7C | `pad7C` | u8[0x10] | - | no | no | no | never read (GC, PC, PS2) |
| 0x8C | `scaleKeys` | KeyTrack | likely | yes | ? | ? |  |
| 0x94 | `colorKeys` | KeyTrack | likely | yes | yes | yes |  |
| 0x9C | `slaves` | u8[8] | likely | yes | ? | ? | shared slaves (count, triggers) |
| 0xA4 | `spread` | ptr | certain | yes | yes | yes | [0..5] direction jitter, [6..11] spin range (degrees) |
| 0xA8 | `smart` | ptr | certain | yes | yes | yes | guidance 1, 2 |
| 0xAC | `holeColor` | ptr | certain | yes | yes | yes |  |

### SmartProjParams

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `pad00` | u8 | - | no | no | no | never read (GC, PC, PS2) |
| 0x01 | `invertZ` | u8 | likely | yes | yes | yes | look for targets behind |
| 0x02 | `delay` | s16 | likely | yes | yes | ? | ms before steering starts (Projectile.timer) |
| 0x04 | `turnMax` | f32 | likely | yes | yes | ? | rad/s |
| 0x08 | `turnOut` | f32 | likely | yes | yes | ? | steering gain turning towards the target |
| 0x0C | `turnIn` | f32 | likely | yes | yes | ? | steering gain when overshooting |
| 0x10 | `dropRange` | f32 | likely | yes | yes | yes | beyond it steer gradually, within it aim directly |
| 0x14 | `gain` | f32 | likely | yes | ? | yes | direct-aim gain |
| 0x18 | `pad18` | u8[4] | - | no | no | no | never read (GC, PC, PS2) |
| 0x1C | `fov` | f32 | likely | yes | yes | yes | targeting cone, degrees |
| 0x20 | `minRange` | f32 | likely | yes | yes | yes | ignore targets closer than this |

### CreatureInfo

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `pad00` | u8[4] | - | no | no | no | never read (GC, PC, PS2) |
| 0x04 | `health` | s32 | likely | yes | yes | yes |  |
| 0x08 | `pad08` | u8[0xC] | - | no | no | no | never read (GC, PC, PS2) |
| 0x14 | `numAnims` | u32 | certain | yes | yes | yes |  |
| 0x18 | `anims` | ptr | certain | yes | yes | yes | indexed by Creature_MotionStateInit |
| 0x1C | `pad1C` | u8[0x10] | - | no | no | no | never read (GC, PC, PS2) |
| 0x2C | `numDeathAnims` | u32 | certain | yes | yes | yes |  |
| 0x30 | `deathAnims` | ptr | certain | yes | yes | yes | indexed by Creature.deathAnim |
| 0x34 | `slaves` | u8[8] | certain | no | yes | no | SlaveList (count, triggers): PS2's Creature_Init starts them (MotionState_InitAllSlavesNow); GameCube and... |
| 0x3C | `numDeathFx` | u32 | likely | no | no | yes | PC only (creature hit handler 0x41F620): |
| 0x40 | `deathFx` | ptr | likely | no | no | yes | [0] is spawned as an EventSeq where the creature dies; GameCube and PS2 don't read it |
| 0x44 | `aiScripts` | u32[4] | likely | yes | ? | ? | CreatureAIScript per difficulty |
| 0x54 | `dropOff` | ptr | certain | yes | ? | yes | dropped at a pod: [0] health bonus, [3] points |
| 0x58 | `dropType` | u8 | likely | yes | ? | yes | Player.counters index |
| 0x59 | `pad59` | u8[0x17] | - | no | no | no | never read (GC, PC, PS2) |
| 0x70 | `damageParts` | ptr | certain | yes | yes | yes | NULL: can't be gibbed |

### ItemRecord

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `type` | u8 | likely | yes | yes | yes | ITEM_TYPE_* |
| 0x01 | `pad01` | u8 | - | no | no | no | 1 in most GameCube items; never read (GC, PC, PS2) |
| 0x02 | `value` | u16 | likely | yes | yes | yes | points, or the car's pickup slave set |
| 0x04 | `pad04` | u8[8] | - | no | no | no | u8 1 or 0, 3 zero bytes, s32 800 (5000, 300, 1000... ms?); never read (GC, PC, PS2) |
| 0x0C | `radius` | f32 | likely | yes | yes | yes | pickup radius |
| 0x10 | `height` | f32 | likely | yes | yes | yes | spawned this high above its point |
| 0x14 | `pad14` | u8[4] | - | no | no | no | 0 in every GameCube item; never read (GC, PC, PS2) |
| 0x18 | `spinMin` | f32 | likely | yes | ? | yes | random spin speed range, degrees |
| 0x1C | `spinMax` | f32 | likely | yes | yes | yes |  |
| 0x20 | `names` | ptr | certain | yes | yes | yes | name string table ([0]: the weapon name) |
| 0x24 | `pickupDebris` | u32 | likely | yes | yes | yes | effect spawned when picked up, 0: none |
| 0x28 | `spawnDebris` | u32 | likely | yes | ? | ? | effect spawned when it appears, 0: none |
| 0x2C | `pad2C` | u8[4] | - | no | no | no | pointer in item 51 only; never read (GC, PC, PS2) |
| 0x30 | `weapon` | ptr | certain | yes | yes | yes |  |
| 0x34 | `pad34` | u8[4] | - | no | no | no | pointer in some items; never read (GC, PC, PS2) |
| 0x38 | `sharedSlaves` | u8[4] | likely | yes | ? | ? | Thing_DrawSharedSlaves triggers |

### WeaponInfo

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `anim` | ptr | certain | yes | ? | ? | wildcard: the rack slot's model |
| 0x04 | `pad04` | u8[0xC] | - | no | no | ? | never read (GC, PS2) |
| 0x10 | `hudName` | u32 | likely | yes | ? | ? | InterfaceHudInfo.weaponName (handle) |
| 0x14 | `pad14` | u8[4] | - | no | no | ? | never read (GC, PS2) |
| 0x18 | `hudIcon` | u32 | likely | yes | ? | ? | InterfaceHudInfo.weaponIcon[0] |
| 0x1C | `pad1C` | u8[4] | - | no | no | ? | never read (GC, PS2) |
| 0x20 | `ammo` | s32 | likely | yes | ? | ? | picked up with the item |
| 0x24 | `ammoPerShot` | u32 | likely | yes | ? | ? | the AI doesn't use it with less |
| 0x28 | `maxAmmo` | s32 | likely | yes | ? | ? |  |
| 0x2C | `aimMode` | u8 | likely | yes | ? | ? | 1: by the stick, 2: auto-aims at targets, 3: faces the target |
| 0x2D | `aimLimit` | u8 | likely | yes | ? | ? | degrees |
| 0x2E | `aimBone` | u16 | likely | yes | ? | ? |  |
| 0x30 | `reloadTime` | u32 | likely | yes | ? | ? | ms |
| 0x34 | `priority` | u8 | likely | yes | ? | ? |  |
| 0x35 | `pad35` | u8[3] | - | no | no | ? | never read (GC, PS2) |
| 0x38 | `minRange` | f32 | likely | yes | ? | ? |  |
| 0x3C | `maxRange` | f32 | likely | yes | ? | ? |  |
| 0x40 | `aimAngle` | f32 | likely | yes | ? | ? | degrees |
| 0x44 | `farAngle` | f32 | likely | yes | ? | ? | aim tolerance at maxRange |
| 0x48 | `nearAngle` | f32 | likely | yes | ? | ? | aim tolerance at minRange |

### WorldGameData

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `fogType` | u8 | likely | yes | yes | yes |  |
| 0x01 | `pad1` | u8[3] | - | no | no | no | never read (GC, PC, PS2) |
| 0x04 | `mapScaleX` | f32 | likely | yes | yes | ? | radar map (InterfaceManager) |
| 0x08 | `mapScaleZ` | f32 | likely | yes | yes | ? |  |
| 0x0C | `envmapX` | f32 | likely | yes | ? | ? |  |
| 0x10 | `envmapY` | f32 | likely | yes | ? | ? |  |
| 0x14 | `fogNear` | f32 | likely | yes | yes | yes |  |
| 0x18 | `fogFar` | f32 | likely | yes | yes | yes |  |
| 0x1C | `pad1C` | u8[4] | - | no | no | no | 0 in every world; never read (GC, PC, PS2) |
| 0x20 | `farPlane` | f32 | likely | yes | ? | ? |  |
| 0x24 | `reverbLevel` | f32 | likely | yes | yes | yes | outside reverb volumes |
| 0x28 | `reverbDelay` | f32 | likely | yes | yes | yes |  |
| 0x2C | `reverbFeedback` | f32 | likely | yes | yes | yes |  |
| 0x30 | `musicVol` | f32 | likely | yes | yes | yes | scales of the music actions (Music_Change) |
| 0x34 | `musicPitch` | f32 | likely | yes | yes | yes |  |
| 0x38 | `musicTempo` | f32 | likely | yes | yes | yes |  |
| 0x3C | `cameraFxLead` | f32 | likely | yes | yes | yes | cameraFx is kept this many seconds of car travel ahead |
| 0x40 | `pad40` | u8[0x18] | - | no | no | no | f32 0.3, 80, 0.075 in every world, then 0; never read (GC, PC, PS2) |
| 0x58 | `mapOffsetX` | f32 | likely | yes | yes | ? | radar map: texture coordinates of the origin |
| 0x5C | `mapOffsetZ` | f32 | likely | yes | yes | ? |  |
| 0x60 | `mapHalfW` | f32 | likely | yes | yes | ? | half the area shown, in texture coordinates |
| 0x64 | `mapHalfH` | f32 | likely | yes | ? | ? |  |
| 0x68 | `impactFxMinVel` | f32 | likely | yes | yes | yes | car-car impact effects, by relative speed |
| 0x6C | `impactFxMaxVel` | f32 | likely | yes | yes | yes |  |
| 0x70 | `creatureFxMinVel` | f32 | likely | no | no | yes | PC only: effects of a car running a creature over |
| 0x74 | `creatureFxMaxVel` | f32 | likely | no | no | yes | (0x413060), picked by speed like impactFx; 5, 100 |
| 0x78 | `mapTexture` | u32 | likely | yes | yes | ? | radar map, 0: none (InterfaceManager_DrawMap) |
| 0x7C | `radarBlip` | u32 | likely | yes | ? | yes | InterfaceManager_SetRadarBlip |
| 0x80 | `pad80` | u8[8] | - | no | no | no | two texture slots; never read (GC, PC, PS2) |
| 0x88 | `lightTexture` | u32 | likely | yes | ? | yes | LightHolder_SetTexture |
| 0x8C | `envmapA` | u32 | likely | yes | ? | ? |  |
| 0x90 | `envmapB` | u32 | likely | yes | ? | ? |  |
| 0x94 | `pad94` | u8[0xC] | - | no | no | no | two texture slots, a pointer; never read (GC, PC, PS2) |
| 0xA0 | `vehiclePackList` | ptr | certain | yes | yes | yes |  |
| 0xA4 | `itemPack` | ptr | certain | yes | yes | yes | name of the world's item pack (type 2) |
| 0xA8 | `padA8` | u8[0x18] | - | no | no | no | 0 in every world; never read (GC, PC, PS2) |
| 0xC0 | `fogColor` | ptr | certain | yes | yes | yes |  |
| 0xC4 | `cameraFx` | ptr | certain | yes | yes | yes | emitter description kept ahead of each camera (rain...) |
| 0xC8 | `debugFont` | ptr | certain | yes | yes | ? | debug font (ShowStats_Init) |
| 0xCC | `pauseInterface` | ptr | certain | yes | yes | yes | interface script of the pause menu |
| 0xD0 | `padD0` | u8[0x10] | - | no | no | no | 0 in every world; never read (GC, PC, PS2) |
| 0xE0 | `padE0` | u8[8] | - | no | no | no | two pointers; never read (GC, PC, PS2) |
| 0xE8 | `numCreatureHitFx` | u32 | likely | no | no | yes | PC only (creature hit handler 0x41F620): |
| 0xEC | `creatureHitFx` | ptr | likely | no | no | yes | [2] under 60 damage, [3] from 60, at a random bone; 0 entries in every world |
| 0xF0 | `numImpactFx` | u32 | certain | yes | yes | yes |  |
| 0xF4 | `impactFx` | ptr | certain | yes | yes | yes | debris ids, by speed |
| 0xF8 | `numCreatureFx` | u32 | likely | no | no | yes | PC only, with creatureFxMinVel: debris ids, |
| 0xFC | `creatureFx` | ptr | likely | no | no | yes | by speed |
| 0x100 | `numCameras` | u32 | certain | yes | yes | yes | chase cameras of things without their own |
| 0x104 | `cameras` | ptr | certain | yes | ? | ? | 12 bytes each (CarACameras) |
| 0x108 | `numCamerasWide` | u32 | certain | yes | ? | ? | the same for view format 2 |
| 0x10C | `camerasWide` | ptr | certain | yes | ? | ? |  |

### IfaceObjectDef

| Offset | Field | Type | Confidence | GC | PS2 | PC | Notes |
|---|---|---|---|---|---|---|---|
| 0x00 | `role` | u8 | likely | yes | yes | yes | InterfaceObject.role |
| 0x01 | `type` | u8 | likely | yes | yes | yes | IFACE_DEF_* |
| 0x02 | `pad2` | u8[4] | - | no | no | no | u8s; never read (GC, PC, PS2) |
| 0x06 | `pressable` | u8 | likely | yes | yes | yes | see IfaceRecord.noPress |
| 0x07 | `pad7` | u8 | - | no | no | no | never read (GC, PC, PS2) |
| 0x08 | `active` | u8 | likely | yes | yes | yes | can be selected |
| 0x09 | `input` | u8 | likely | yes | yes | yes | role 3 child: controller 1-4 |
| 0x0A | `id` | u16 | likely | yes | yes | yes | role-specific parameter (HUD slot, goal id...) |
| 0x0C | `anchor` | s16 | likely | yes | yes | yes | relative index of the object it is positioned at, 0: none |
| 0x0E | `unusedE` | u16 | - | no | no | no | never read (GC, PC, PS2) |
| 0x10 | `linkId` | u32 | likely | yes | yes | yes | InterfaceManager_LinkThingToObject; debug vectors: chain |
| 0x14 | `sub` | ptr | certain | yes | yes | yes | type-specific data, see InterfaceObject.data |
| 0x18 | `stateIndex` | s32[6] | likely | yes | yes | yes | first IfaceStateDef per IFACE_STATE_*, -1: none (written by SetObjectState) |
| 0x30 | `anchorX` | f32 | likely | yes | yes | yes | -1..1: alignment of the object on its position |
| 0x34 | `anchorY` | f32 | likely | yes | yes | yes |  |
| 0x38 | `pad38` | u8[4] | - | no | no | no | never read (GC, PC, PS2) |
| 0x3C | `child` | ptr | certain | yes | ? | ? | slot holding the child interface |
| 0x40 | `query` | ptr | certain | yes | yes | yes | role 2: [0] != 0: use the callback's value |
| 0x44 | `record` | ptr | certain | yes | yes | yes |  |
| 0x48 | `conditions` | ptr | certain | yes | yes | yes |  |
| 0x4C | `pad4C` | u8[0x60 - 0x4C] | - | no | no | no | never read (GC, PC, PS2) |
| 0x60 | `anim` | ptr | certain | yes | yes | yes |  |
| 0x64 | `numStates` | u32 | certain | yes | yes | yes |  |
| 0x68 | `states` | ptr | certain | yes | yes | yes |  |

