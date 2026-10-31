# Adding cars 33-35 (Fandango, Sweet 16 II, Jester) to HWVX GameCube

Short version: the shipped interface data was already built for 36 cars. Every
car-select string list and thumbnail texture-animation has 36 entries, and
thumbnails 33-35 are real pictures of the three cut cars. The only things
holding the roster at 33 are:

1. a `max = 32` on 7 interface variables per car-select screen,
2. the save/unlock array `cars[0x21]` in the code,
3. the car packs themselves, which are not in HW_Cars.xgc.

**Verified at runtime:** patched HW_IPack + the SDL build with HWVX_CHEATS=1.
Scrolling left from Twin Mill 2 now shows Jester -> Sweet 16 II -> Fandango with
correct thumbnails and names. See `carselect_new_cars.png` (frames from `run9/`).
The 3D preview is empty because packs 33-35 are missing from HW_Cars.xgc. A
failed load does not crash.

## 1. Car-select screens: where the list lives and how it works

The "list" in `GSI_VehicleItemSelection` (`msg->list[msg->selection]`) is not
a car-id table. It is the interface's variable array (`iface->values`), and
`selection` is a variable index taken from the event's `bytes[0]`.

### Screens (IMHeader id 2 = vehicle select; InterfaceManager_Create sets the GSI callback)

| HW_IPack pack | header @data | objs | use |
|---|---|---|---|
| 05_interface_6 (pack id 6, front end, verified) | 0x72d7a0 | 84 | 1-player car select (reached via One Player > Drag Race) |
| 05_interface_6 | 0x7c9a00 | 74 | multiplayer per-player select (child of id-8 header 0x7c6540, 22 objs) |
| 06_interface_7 (near-copy of 6, offsets +0xC0) | 0x72d860 / 0x7c9ac0 | 84 / 74 | same screens in the other front-end pack |
| 02_interface_3 | 0x3b7780 | 84 | car select under id-8 header 0x39eba0 (with weapon select id 3). Flow not confirmed |

No other interface lists cars. Packs 1, 2, 4, 5, 8 and 9 have no 33/36-entry
car tables. The other 0x64 conditions are a fixed "car 30" unlock check in
pack 6 @0x880620, pack 7 and pack 8.

### Binary representation (per screen)

- **Variables** (`hdr+0x1C -> {count, IfaceVar[0x34]}`):
  - var1..var7: type 0, min 0, **max 32**, offset 0..6. These are the 7 carousel slots; var4 is the centre (selected car id).
  - var0 is the scroll direction (-1..1). var8 is info on/off. var9 is code var 0xC9 (vote).
  - `max` is the s32 at `IfaceVar+8`. Header 0x72d7a0 in pack 6: vars at 0x72d9f4 + 0x34*k (k = 0..6), max at +8.
- **Scrolling:** the object data "steps" on the selectable objects (obj 71/72) add -1/+1 to var1..7 on left/right. The engine wraps them to min..max (`InterfaceManager.c` ~2165).
- **Selection event:** type 0xD, `bytes = 04 32 33` (var 4, 3D objects 0x32/0x33). GSI loads `HW_Cars` pack `id = vars[4]`.
- **Confirm:** type 0x11, arg isVar=1 value 4. Then `GetCodeVariable(100 = 0x64, vars[4])` and set 0xCB.
- **Thumbnails:** objects of type 0 with ObjSub type 3. These are texture key animations: `{u32 numKeys = 36, keys*, frameTime, u8 0xC = 1 (frame from variable), u16 0xE = var index}`. The frame index is the car id (`InterfaceManager.c:4035`). Keys are texture slots 60..95 of pack 6 (texture_relocs). 60-92 are RGBA8 128x128; **93-95 are CMPR thumbnails of Fandango (red), Sweet 16 II (green) and Jester (purple)** (`carthumbs_if6.png`).
- **Names:** text objects (type 1) whose `sub+0x10 == 1` hold a string list. List at `sub+8`: `{u32 count = 36, texts*, u8, u16 var = 4}`. The name lists are objs 75/76/77 (multiplayer: 61/62). All of them point to the same string slots, one set per pack. Entries 33-35 are placeholders ("HW Prototype 12*", "Power Rocket*", "Jet Threat 3*"). Each string has a 32-byte slot. `*` renders as the TM glyph.
- **Unlock hints:** obj 82 (multiplayer: obj 67), a 36-entry list on var 4. Entries 33-35 are " " (32-byte slots).
- **Locked state:** object conditions type 2 = code var **0x64 with arg = variable (isVar=1, var N)**. The index is therefore the **car id**, not a list position. This covers padlocks on slots (objs 15-19, 32-37, 50-55) and the hint panel (81/82) when var4 is locked.
- **Stats bars:** object kinds 70-73 read `CarAInfo->vehicleIface->hud->stats[0..3]` from the loaded car pack (GSI_VehicleSelectionInfo). They come from the new car packs, not from interface data.

## 2. Names and images

- Names exist only in the HW_IPack packs above (02, 05, 06). They are not in HW_Cars, mcp or HW_brief.
- Thumbnails are in the same packs. They already exist for 33-35, so no texture work is needed. Swapping them is optional.
- The preview model comes from the car pack.

## 3. Hard-coded car counts in code

| place | what | change |
|---|---|---|
| `include/common.h:506` HotWheelsGameData | `s8 cars[0x21]` @0x00, `tracks[0x39]` @0x21, **`u8 pad5A[0x39]` @0x5A is unused** (no access in the SDL code or the GC decomp) | use pad5A[0..2] for cars 33-35 |
| `GameSettings.c:272` Set 0x64 | `if (arg < 0x21)` | map 0x21..0x23 -> pad5A |
| `GameSettings.c:571` Get 0x64 | `if (arg < 0x21)` (cheat 1 already returns 1 for any arg) | same |
| `GameSettings.c:104-109` GameSetting_Init | default cars 3,4,5 | optional |
| `GameSettings.c:1738` level details | `i < 33` car count, `/75.0f` percentage | cosmetic: count 36, 75->78 |
| `State_ProgressionManager.c:671-706` | AI random car: `randOffset = rand(0, 0x24)`, `id % (TileCar.maxVehicle+1)` | data: world TileCar+0x37 |
| `DataGamePackLoad.c` / `common.h:314` | TOC cache `FileTOCEntry entries[50]`; header read 0x800 bytes | HW_Cars goes from 34 to 37 packs, under the limit |

Nothing else is per-car. PlayerManager, World and GSI use the id directly. Code var 0xCB is s32.

### Save format (backward compatible)

Keep `sizeof(HotWheelsGameData) == 0xF4` and store cars 33-35 in pad5A[0..2] (save offsets 0x5A-0x5C).
- Old saves have zeros there. If 0 means locked, the new cars are locked in old saves.
- For "unlocked by default, even on old saves", store the inverse: `carsExtLocked[3]`, where 0 = unlocked.
- Do not grow `cars[]`. That shifts `tracks` and every later field and breaks existing saves and the 0xF4 asserts.

## 4. Plan

### Data
1. **HW_IPack.xgc, in place, sizes unchanged.** Script: `patch_ipack.py`. Run it as `cp -r ipack X && python3 patch_ipack.py X && hwvx-x pack X out/HW_IPack.xgc`.
   - In every id-2 header of packs 02, 05 and 06: var1..7 `max` 32 -> 35 (7 s32 per header, 5 headers, 35 words).
   - Name strings 33/34/35 -> "Fandango*", "Sweet 16 II*", "Jester*" (one shared set per pack).
   - Optional: hint strings 33-35 (32-byte slots, <=31 chars, `|` = newline).
   - Result: 146 bytes changed in the .xgc. No pointer, pointers.bin or reloc changes, because nothing grows.
2. **HW_Cars.xgc:** append car packs with ids 33, 34, 35 that reference the shared pack id 2, like the existing ones. The TOC becomes 37 entries (limit 50, and 37*0x18+0x10 < 0x800).
3. **Optional AI:** in the 14 race/battle worlds (`tilecars.txt`: bat1-8, race1-6 l1/l2), set TileCar+0x37 `maxVehicle` 32 -> 35 so random AI can use the new cars. Single byte edits. Challenge, mission and joyride worlds use fixed cars (maxVehicle 1).

### Code (SDL; the same logic is needed in the DOL)
See `code_changes.diff` (proposal, not applied).
- Get/Set 0x64: `arg < 0x21` -> `cars[arg]`; `0x21 <= arg < 0x24` -> `pad5A[arg-0x21]` (or the inverted-lock variant).
- Optional: GetActiveMemoryCardLevelDetails counts 36 cars.
- **Real GameCube DOL:** GameSettings_SetCodeVariable @0x8002F28C and GameSettings_GetCodeVariable @0x8002FAC8 each have a `cmplwi rX,0x21` for the type-0 0x64 case. These need a small hook/code cave for the remap.
  - Changing only the bound to 0x24, without the cave, would alias cars 33-35 onto tracks[0..2]. tracks[0] is set by default, so that is not acceptable.
  - Alternative DOL hack: return 1 for arg 0x21..0x23 (always unlocked), with no save change.
  - Everything else (interface, AI, GSI) needs no DOL change.

## 5. Uncertainties
- Which game flow uses pack 3's car select (0x3b7780). It is patched the same way regardless.
- Packs 6 and 7 are near-identical front-end variants. The selection rule is in the MCP steps and was not checked; patch both.
- No in-game trigger unlocks 33-35. Without default-unlock or cheats they stay locked unless a briefing/world action sets 0x64 for them.
- The new car packs must provide `vehicleIface->hud->stats` and AI scripts like the other cars do; this is not covered here.
- The DOL hook was not written or tested.
