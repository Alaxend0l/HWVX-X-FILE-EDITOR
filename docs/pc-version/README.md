# The PC release next to the GameCube one

What the 2002 PC release (`Hot Wheels.exe`, `.xpc` packs) does differently from the GameCube game,
in code and in data, and what that means for tools that read or write either. Collected while
comparing `HWVX-PC-Decomp` with `HWVX-GC-Decomp` and teaching HWVX-SDL to load `.xpc` packs.

* Car-pack formats (geometry, textures, sounds) are in
  [`docs/beta-cars/research/pc_formats.md`](../beta-cars/research/pc_formats.md); this file adds what
  that one doesn't cover and doesn't repeat it.
* The 33-car limit and its exe patch are in
  [`docs/beta-cars/research/pc_exe.md`](../beta-cars/research/pc_exe.md).

## Files

| | sha1 | note |
|---|---|---|
| the CD's `HWVX.cab` → `Hot_Wheels.exe` | `4aa790d0…` | **the original**, as installed from the disc |
| `orig/pc-backup/Hot Wheels.exe` | `91af8432…` | the original with a **no-CD patch**: 3 bytes at `0x455E90`, the CD check (`GetVolumeInformationA`/`GetDriveTypeA`, "Please insert the CD-ROM…"), turned into `mov al, 1; ret`. Otherwise identical; `HWVX-PC-Decomp` currently targets this one |
| `orig/pc/Hot Wheels.exe` | `5485ea3f…` | the no-CD exe plus the car-limit patch from `pc_exe.md` (35 bytes in two caves) |

PE32, image base 0x400000, no relocations needed: file offset = VA − 0x400000 in `.text`. No symbols.
The exe still names its sources in assert paths (`C:\Projects\Game\Trigger.c`,
`C:\Projects\BSLibs\Audio\Voice.c`, `…\Win32\Audio_.cpp`, `…\Render\OpenGL\Render_.c`); a function
that pushes one of those strings belongs to that file, which is the quickest way to pair it with
its GameCube namesake. Link order follows the GameCube file order closely.

The renderer is OpenGL 1.x (`glDrawElements`, `glCompressedTexImage2DARB`, `gluBuild2DMipmaps`),
sound is DirectSound, video is Bink (`binkw32.dll`, `FMV_\*.bik`).

### Folder layout

Every pack has the same name as on the GameCube with `.xpc` for `.xgc`, in the same folders
(`HW_*.xpc`, `mcp.xpc`, `worlds_/{geo_,tex_,col_,mp_,sp_}/…`, `audio_/HWB_Mxx.xpc`). The retail
PS2 release (`orig/ps2`) uses `XPS/ENGLISH/<name>.xps` instead. Its TOC header is byte-identical to
the PC's (little-endian, version 0xB7):

```
PC/PS2  a000 0102 ae00 0104 b700 0000 …   (LE 0x020100A0, 0x040100AE, 0xB7)
GC      0201 00a0 0401 00ae 0000 00b7 …
```

## PC data the car-pack notes don't cover

* **Pack loading** (exe `0x420C80`, the PC `DataPack_LoadDataPack`): the pack-level alignment is
  the GameCube's: tables padded to 32 (`and 0x1f`), the sound region and blob 64- and 32-aligned
  (`& -0x40`, `& -0x20`). Only objects *inside* the blob are 16-aligned. A loader written for
  `.xgc` reads `.xpc` tables, offsets and pointer lists unchanged, minus the byte swap.
* **Texture table:** headers are packed at 0x10 (`u16 format, width, height, mips; ptr; 0`, LE).
  GameCube headers are the same 0x10 bytes **padded to 0x20**, and their u16s are little-endian
  even in the big-endian file (the PPC code stores them byte-reversed). So "walk the table" differs
  only in stride.
* **Music** (`HW_music.xpc`): the songs stay `RIFX` 'MIDI' (big-endian container, as on the
  GameCube), but the instrument banks are `RIFF` 'sndb', **little-endian**: header words, the
  0x20-byte instrument entries and the PCM16 samples. Unused header words are `0xCCCCCCCC`
  (uninitialised MSVC memory). Bank sizes are equal to the GameCube's.
* **Sounds:** in every pack, SFX samples are PCM16 LE (GC: BE). The descriptor's length is in bytes.
* **Interfaces** (`HW_IPack.xpc`): the same screens, with PC wording ("PRESS ENTER"), one
  car-select header per pack where the GameCube has two (if6/if7), and a keyboard-remap screen
  whose strings come from `InterfaceManager_SetControllerConfigInputString` (`0x434310`).
* **Geometry group texture slot:** after relocation the PC keeps the texture id in the u16 at
  `+0x18` (its draw code binds that). GameCube code reads the id at `+0x1A` and treats `+0x18` as
  an override that `ShaderHolder` writes for animated textures. A tool converting between the two
  has to move the id.

## Save files

One file holds every slot: **`My Documents\VelocityX Saved Games\velocityX`** (the folder name and
`SavesFolder`/`saveFile` are strings in the exe; under CrossOver, the bottle's Documents is the
Mac's `~/Documents`). Nothing goes to the registry, which holds only the graphics options. A save
from a session that beat Challenges 1 and 2 (`HWVX-SDL/build-saves/pc/velocityX.sav`, 2356 bytes):

| offset | size | contents |
|---|---|---|
| 0x000 | 4 | `SAVE_FILE_MAGIC` 4 |
| 0x004 | 4 | 0x928: the size of what follows the header (GameCube: `g_gameInfo[].version`) |
| 0x008 | 4 | checksum: `FileManager_GetChecksum` (sum of byte + 1) over the 0x928 bytes from 0x00C; verified |
| 0x00C | 0x188 | general settings, the GameCube's `GeneralGameSettings` (0x150) plus 0x38 bytes: s32 difficulty, audio (`64 64 01 00`), **4 × 0x20 controller settings**, then at +0x88 the 0x20-byte slot entries (`01`, name "HW VEL X 1", u32 year/month/day/hour/minute) |
| 0x194 | 8 × 0xF4 | the slots' `HotWheelsGameData`, **the GameCube's struct, little-endian**: cars, tracks, items, cheats, extras, script values, vehicle, item |

Decoded: both slots have cars 0–5 (the PC's default unlocks) and items 0–3; the later save has
tracks 33 and 41 more.

The 0x38 extra bytes are 4 × 14: each player's controller settings are 0x20 bytes where the
GameCube's are 0x12. The first 0x12 are the GameCube's (`data[0]` vibration, `data[1]` control
scheme, then the 16 bytes of the MCP's binding, `00 00 41 11 26 28 00 10 57 53 27 25 44 20 c0 00`,
the same on all three releases: its key codes are Windows virtual keys, arrows, W/A/S/D, space);
then a 13-byte gamepad button map (`0a 0b 09 07 08 06 04 05 03 02 0c 0d 01`, the defaults
`func_00429F10` writes at +0x12) and one byte of padding. The settings block is
`D_0064D160` (0x188 bytes, `func_0042C1A0` writes it), the slot entries `D_0064D1E8` = +0x88.
HWVX-SDL imports these saves (`Port_SaveImport.c`, docs/xdl-format §11.5).

## Game logic: PC vs GameCube

Compared subsystem by subsystem: the PC function was found through strings, constants and call
graphs, then both were read. "Confirmed" means both binaries were read; the GameCube side was
checked in `main.dol` where its `.c` is NonMatching.

### Only on PC

| what | PC | notes |
|---|---|---|
| creatures take damage and die | hit handler `0x41F620`; callers: car runs over (`0x413060`, from `CarA_PreMove` `0x40F261`), blasts (`0x4231A0`, in `Blast_Action`), projectile sweep (`0x441360`, from `Projectile_PreMove` and twice from SmartProjectiles) | GC creatures die only by script or their own logic |
| a third car handling mode (`CarAInfo+0x11 == 2`) | controller `0x413540`; suspension extras in `0x416A40`; engine sound `0x40EF94` | GC has an empty `case 2`. Full gearbox, turbo refilled to 1000 every frame, backfire effect on upshift, steering (180 − forward speed)/150 · stick · −π/4. **Not yet checked whether any shipped car uses it** |
| keyboard: menus read `WM_KEYDOWN` | `InterfaceManager_ProcessInput` `0x431300`, `MoveInterface` `0x42FA50` | a held arrow repeats at the Windows key repeat (≈500 ms, then ≈30/s); GC repeats the stick every 250 ms |
| Esc pauses, menu always on input 0 | `StateGamePlay_Heartbeat` `0x444A40` | GC: Start on any human's pad, menu to that pad |
| keyboard remap screen | `0x434310` | GC: empty stub |
| a `"%d/%d"` HUD counter of things attached to the car | in `CarA_GetInterfaceInfo` (`0x4105E0`, ~`0x410646`) | purpose unknown |
| debug combo `[0x800, 4, 0x10]` records poses to `HW\NPP.DC` | `0x43F690` / `0x43F530` | GC branch empty |
| registry options | read at `0x454E70`, stored `0x68DEE0`–`0x68DF06` | see below |

* **The trigger orbit camera** (`camera_set_targets` with an orbit, camera mode 28 of
  `Camera_UpdatePosition`: GC `0x8001208C`, PC `0x40C520`, PS2 `0x00258940`). After
  `Camera_Orbit`, the GameCube (`0x80012DBC`–`0x80012E0C`) and the PS2 (`0x0025907C`–`0x002590DC`)
  check the player's car: if it is a `CarA`, tipped past 60° (`worldMatrix.v[1].y < 0.5`, at
  `+0x24`) and either stuck (`stuckCounter` `+0x464` > 0) or with its wheels down (`airTime`
  `+0x460` ≤ 0), the camera's mode goes back to `g_triggerAction_params_default` and the camera
  goes back to the car, ending the event's orbit. The PC's mode-28 code (`0x40C93B`–`0x40C9CD`)
  calls the orbit (`0x40BD20`) and jumps straight to the end of the function. It has no check,
  so its orbit keeps going until the trigger script changes the camera. The PC exe writes the
  default mode (`0x47A2E4`) only in `CameraManager_GetNewCamera` (`0x40A5A2`) and
  `CameraManager_ReturnAllToPlayer` (`0x40CA81`). Its `Camera_TriggerActionGetLocation`
  (`0x40C350`) also leaves out the GameCube/PS2 hand-back when a car location's car has been
  freed or released (`flags & 0x201000`).
  HWVX-SDL plays the PC's orbit by default. `--gc-air-camera` / `HWVX_GC_AIR_CAMERA=1` turns
  the GameCube/PS2 check back on.
  Players see the GameCube hand-back mostly after jumps and crashes. `stuckCounter` goes up
  after the tipped body hits the ground, and it is cleared only once the car is upright again
  (`CarA.c`, `tiltState`). So when an event starts its orbit while the car is coming down or
  rolling, the orbit ends at once.

### Only on GameCube

* Player-steered weapons and turrets: WildCard spin type 1 (`WildCardRack_Move`, PC `0x451360`),
  turret rotation in `Destructible_Move` (PC `0x4223E0`).
* GBA link: cars 0–2 unlock only through it. PC unlocks cars 0–5 by default (`0x429C30`) and its
  save summary's "Completed %" is `(sum-50)/72` for GC's `(sum-47)/75`.
* Code variable 0x19C (a 30-entry array; PC's CodeVariables struct is 0x78 bytes shorter);
  `Get(0x258)` returns 2 on GC, 0 on PC (looks like a platform id scripts can read).
* "Lid Handler" process.

### Changed rules and tuning

| | GC | PC |
|---|---|---|
| wins-required modes (`GameRules_HeartBeat`, PC `0x4293C0`) | reads var 0x131 | also rewrites 0x131 while nobody has the wins |
| winner when time runs out, modes 1/2 | highest score | **first** player with score > 0 (looks like a bug) |
| item cheat 8 (item 3 → 0x4D) | pickups and the chosen item | pickups only (`0x446840`) |
| failed device check in `StateManager_QueueChange` | change dropped | forces Shutdown (`0x4480E0`) |
| emitter catch-up (`Emitter_Move`, PC `0x422DF0`) | capped at 1000 ms | uncapped |
| tyre particles | humans in 1–2 player views | a quality setting (`0x68DF05`), AI too |
| lens flare (`0x438BD0`) | intensity from 128 | from 64, so half as bright; fade assumes a 480-line screen |
| one-shot SFX priority (`0x409E60`) | `rand()%10 + 5` | always 5 (first to be cut) |
| voices | 32, the DSP steals | 64 (32 SFX, 32 music), stolen in software |
| `DestroySFX` (`0x459630`) | frees the sample | stops its voices first (`0x45D110`) |
| LOD in split screen (`Gfx_GetCullLODGX` ~`0x45E3xx`) | one level coarser whenever split | only when Geometry Quality is low |

### Registry options and what reads them

| option | global | effect |
|---|---|---|
| Geometry Quality | `0x68DEEC` | renderer flag 2: coarser LOD |
| Texture Quality | `0x68DEF0` | **dead**: saved, never read |
| Particle Quality | `0x68DEF4` | `CreateParticle` (`0x43BE70`): low keeps 1 in 17 GX particles |
| Draw Distance | `0x68DF00` | renderer flag 4: far plane 500 (else 15000; GC 20000), forced grey fog 100–500 |
| Filter / AA | `0x68DEFC` / `0x68DEF8` | texture filter 0/1/2, AA passed to the GL setup |
| Surface Particles | `0x68DF05` | tyre sparks/dust/smoke |
| Disable Movies | `0x68DF06` | Bink play returns at once (`0x43B096`) |

### Stereo pan (the speaker law)

A player's report: the PC release has wider stereo than the GameCube. It does, and the cause is
only the last step, the speaker law. All three releases compute the same pan:

- SFX_ProcessSource (GC Voice/AudioManager; PC 0x409830..0x409A01; PS2 EE 0x257534): pan =
  26213.6 (0x46CCCB33) x the source's x in the listener's frame (normalized), so +-0x6666 at the
  side; split screen (more than one listener) centres everything. The IT player: channel pan
  (p - 32) << 10, +-0x7FFF (GC IT.c, PC 0x45B86A). SetVoicePos stores it at Voice+0x40 (PC 0x459A70).
- GameCube SetVoiceVol_ (0x800BD1D4) and PS2 AUDIO.IRX SetVoiceVol_ (IOP 0x5458): 7-bit index
  ((pan >> 9) & 0x7F) ^ 0x40 into g_panTab = 32767 sqrt(i/127) (the same table on both):
  equal power, -3 dB per speaker at centre, 9.8 dB apart at the side; music at full pan is hard.
  No AXSetMode in the GC binary: plain stereo AX, no Dolby Pro Logic II; the port's ax.c mixes
  vL/vR linearly, as AX does.
- PC SetVoiceVol_ (0x45DC20): SetVolume(600 log2(vol) - 9000) (0x45D5D0, an fyl2x; the decomp's
  C for it lacks the log) and SetPan(pan / 4) (0x45D610), clamped +-10000: the far speaker
  -|pan|/400 dB, the near one at full. 30 deg off centre: 32.8 dB apart (GC 3.7); at the side
  65.5 dB (GC 9.8); a music channel at 75 % pan 41 dB (GC 4.9). No speaker-mode check, no reverb.

`xdlrs audio pan` prints the table; `xdlrs audio level DUMP --lr` a dump's L/R per window.
Measured (HWVX_START=race:mode=battle, world 0, A presses, 0.25 s windows, from 5 s, music on): mean |R-L|
1.6 dB (max 4.0) with the GameCube law, 2.7 dB (max 6.5) with --pc-pan; the song and the
player's own (centred) engine dominate the mix, single sounds differ by the amounts above.

Majority rule: GC and PS2 agree, so the port keeps their law by default. `--pc-pan` /
`HWVX_PC_PAN=1` (Port_PCPan, game/Voice_.c PCPan) uses the PC law for every voice (effects and
music), with the centre kept at the GameCube level (near speaker 0x5A82), so centred sounds and
the attract demo are unchanged; a sound at the side is 3 dB lower in total power, as on the PC.

### The same on both

The trigger and action systems and their table sizes; thing pools
(140/192/160/500/96/192/128/1/16/8/100/1/32/1/1/56/128/64); normal (mode 1) handling; stunt scoring
(Ceiling/Wall Plant, 99999 points, 30000 turbo); combo cheats; AI rubber-banding, stuck detection,
weapons and car picks; cheats and unlocks; save-data defaults; the state table; world loading; the
3D-sound maths; particle, trail, tracer, bullet-hole and light limits; the per-frame update order.

### Bugs both share

* `Projectile_PreMove` (GC `Projectile.c:463`, PC `0x441510`): over the speed cap the velocity is
  scaled by `1/sqrt(speed²)`, i.e. to speed 1, not to the cap.

## PC function map

PC addresses paired with GameCube names so far (confirmed by reading both). Units are
`HWVX-PC-Decomp/src/auto/text_<first function>.cpp`.

| GC | PC | | GC | PC |
|---|---|---|---|---|
| `GameSettings_SetCodeVariable` | `0x429FA0` | | `InterfaceManager_KeyPress` | `0x430CA0` |
| `GameSettings_GetCodeVariable` | `0x42A680` | | `InterfaceManager_ProcessInput` | `0x431300` |
| `IsCheatEnabled` | `0x42AFB0` | | `InterfaceManager_MoveInterface` | `0x42FA50` |
| `GameSetting_Init` (defaults) | `0x429C30` | | `StateManager_Create` | `0x447DD0` |
| `GameRules_HeartBeat` | `0x4293C0` | | `StateManager_GotoState` | `0x447FC0` |
| `LoadWorldProcess` | `0x446840` | | `StateManager_QueueChange` | `0x4480E0` |
| `DataPack_LoadDataPack` | `0x420C80` | | `StateGamePlay_Init / Heartbeat` | `0x443DC0` / `0x444A40` |
| `CarA_PreMove` | `0x40E8D0` | | `Input_Heartbeat` | `0x4568C0` |
| `CarA_Move` | `0x40F270` | | `Input_KeyPress` / `Input_GetAnalog` | `0x4590E0` / `0x458F20` |
| `CarA_ExecuteCombo` | `0x40DCD0` | | `SoundManager_Init` | `0x409220` |
| `CarA_ComputeSuspensionForces` | `0x416A40` | | `SFX_ProcessSource` | `0x4096C0` |
| `…_Aux1` (stunt landing) | `0x4160C0` | | `SoundAudioSample_Play` | `0x409E60` |
| `CarA_PreMove_Control_HW` | `0x414820` | | `GetVoice` / `DestroySFX` | `0x459880` / `0x459630` |
| `AI_Car_Cheat` (catch-up) | `0x402C00` | | `ParticleManager_Init` | `0x43BDB0` |
| `CheckIfStuck` | `0x4038D0` | | `CreateParticle` | `0x43BE70` |
| `Projectile_PreMove` | `0x441510` | | `Emitter_Move` | `0x422DF0` |
| `Blast_Action` | `0x423550` | | `LensFlare_Render` | `0x438BD0` |
| `WildCardRack_Move` | `0x451360` | | `Trigger` table / Action table | `0x47B9C0` / `0x47A040` |
| `Destructible_Move` | `0x4223E0` | | `ThingManager_MoveThings_DARYL_SUBTICK` | `0x44BA60` |

`HotWheelsGameData` is at `0x64D300` (0xF4 bytes, the GameCube layout); `__ftol` is the CRT copy at
`0x46A7CC`; `g_iDeltaTimeMS` is `0x692370`.

## Decompiling the PC exe

`HWVX-PC-Decomp` builds on macOS with the repo's `MSVC600/` (Visual C++ 6.0) under CrossOver's
Wine, in a bottle of its own:

```sh
python3 tools/configure.py hotwheels --msvc ~/.local/share/decompnet/toolchains/msvc \
    --wibo ~/.local/bin/wine-msvc6      # wrapper: wine --bottle msvc6, WINEPATH with MSPDB60.DLL
ninja
python3 tools/instr_diff.py [unit|func] [--show]
```

`CL.EXE` fails silently without `MSVC600/Common/MSDev98/Bin` (MSPDB60.DLL) on the Wine path.
`tools/instr_diff.py` compares instructions, not bytes (relocations by symbol, branch targets by
index, `__ftol` and D_ offsets normalised); the target is an instruction match.

Some units look built with **`/G6`**: `text_00413540` goes from 73.8% to 92.2% with it and two of
its functions match exactly; the remaining `andb`/`orb` where the original has `andl`/`orl` in
`text_00412DB0` look the same. 430 functions match without it, so it would be per unit
(`config/hotwheels/file_flags.json`).

## HWVX-SDL and other releases' packs

The layout the port converts every release into, and the rules, are specified in
[`../port-format/README.md`](../port-format/README.md).

HWVX-SDL (branch `pc-data`) loads `.xpc` packs file by file next to `.xgc` ones:

* `dvd.c` opens a pack under its GameCube name, then the other releases' (`s_formats`: extension
  and folder per release), and tags the file with `PORT_DATA_GC/PC/PS2`.
* `game/Port_Data.c` runs every load step by that tag: GameCube packs go to the byte swapper, PC
  packs need no swap. PC texture headers become port-only formats (`PORT_TEX_PC | format`) that
  `gx_tex.c` decodes (DXT1 as RGB, DXT5, raw); a setting draws them with the GameCube's colours
  (squared) or the PC's own. PC geometry is marked and drawn by index (below). PC song banks get
  the GameCube's `RIFX` header form in place.
* Sound samples are kept in host order whichever release they come from: GameCube PCM16 samples
  are swapped as their pack loads, PC ones need nothing, and the mixer reads plain `s16`.
* PS2 support needs a `s_formats` entry (`.xps`, `XPS/ENGLISH/`) and its `PORT_DATA_PS2` steps.

The whole PC install runs as `--data`: PC menus (with the PC's key hints), worlds, cars, HUD,
items and music.

* **Movies:** the PC's are Bink (`FMV_\*.bik`, not in `orig/pc`). The port plays THP only; a
  movie it can't find is logged and skipped, so a PC folder runs without movies and a mixed folder
  plays the GameCube's THP files. Decoding Bink would need a third-party decoder.
* **`bsfile`:** the GameCube's Lid Handler polls this disc file; a PC folder lacks it, the reads
  fail harmlessly (the PC release has no Lid Handler).
* **Menus:** PC's main menu has no Multi-Player row and its own "Exit to Desktop" (with a
  confirm screen; it exits the port cleanly), so the port adds no "Quit Game" there. PC's Game
  Settings has no Sound Mode row, which the port's Graphics screen was built from; adapting
  `Port_Menu.c` to the PC screens is in progress.

### Geometry: the PC layout as the port's own

Pack geometry is drawn in one place, `Render_DrawGroupPrims` (`Render_Geometry_.c`), and only in
the PC's indexed layout. PC geometry is used as it is: `Port_Data.c` finds a pack's (and a
quadrant pack's) geometries by their headers (`0x0002`, levels at header + 0x40), marks their drawn
levels `PORT_LAYOUT_INDEXED` in the level's `u16` at 0 (never read by the game), and flags their
groups' texture relocations so the id lands at `+0x1A`. GameCube geometry is converted to the same
layout as its pack loads (`Port_Geometry.c`), skinned geometry included. `GX_PortDrawIndexed`
(`gx_mesh.c`) builds vertices the way the display-list decoder did; skinned levels take each
vertex's bone from the per-bone counts at `LOD+0x10` (bone k is matrix slot k). The GameCube's
display-list path for pack geometry is gone: the converted frames were byte-identical to it.
Everything else that touches geometry is layout-independent: LOD choice and culling (stride 0x18,
distances, bounding boxes), the draw lists (`+0x10` only tested for non-zero), radius from the
bounding box. Shadow volumes keep their own format, read by `Render_DrawShadows`.

The layout and the conversion rules are specified in
[`../port-format/README.md`](../port-format/README.md).

`VertexColorSeqManager_AddVertices` registers a quadrant's vertex-colour sequences, but nothing in
the GameCube code reads that list, so it doesn't constrain the vertex layout.

## Open questions

* Does any shipped car set drive mode 2?
* Whether PC's extra shadow boundary edges (`n1 = 0`) change `CreateSilhouette`'s result.
* PC collision packs (`col_`) haven't been compared with the GameCube's.
* PC `Projectile_Draw` lacks GC's 360.0 constant; not looked into.
* Creature AI on PC dispatches to five sub-states (`0x406880`…`0x406FF0`) with other random ranges;
  only partly mapped.
* Not compared yet: camera tuning, quadrant streaming, the AI car state machine, NavHolder, the
  MIDI/IT players, collision code beyond its constants.

## The third handling mode (driveMode 2) in HWVX-SDL

As built on branch drivemode2 (merged). Notes for docs/xdl-format §11 and docs/fields are included below.

Branch `drivemode2`. Three pieces of text for three documents.

### For docs/pc-version/README.md, "Only on PC" (replaces the row, answers the open question)

| a third car handling mode (`CarAInfo+0x11 == 2`) | `CarA_PreMove` `0x40E956` (sets turbo time to 1000, then the controller `0x413540`); rescue loading `0x412DB0`/`0x412FD0`; suspension `0x416CD3`..`0x417CA8`; wind sound `0x40EF94`; `CarA_Init` `0x40D545` (flat torque curve, also on GC) | GC and PS2 (`CarA_PreMove` `0x25F91C`) have an empty `case 2`: no driver. **No shipped car uses it**: all 33 are mode 1 on GC, PC and PS2, nothing writes the byte, and the PC has no menu or registry option for it. HWVX-SDL offers it (Options, Handling) |

What mode 2 does (all verified against the PC asm; HWVX-SDL `src/game/CarA_HW.c`):

* **Controller** `0x413540`: `back` (Player 0x14, action 4) reverses with half the torque; `brake`
  (0x13) slows the wheel speed to 0 at `brakeDecel` (from reversing at `reverseThrottleDecel`)
  and never reverses; `gas` (0x12) adds the **full** torque (mode 1 takes half), shifts up past
  `upshiftRpm` with `CarA_ProcessImpulse(car, dir, pos, -10·dir, 1.0)` from 8 behind and 3 below
  the car (the "backfire": a kick forward, built like the turbo's); off the gas or at the rev limit
  on the ground, engine braking of `engineBrake·255·brakeDecel` plus `160·forward.y` uphill, with
  downshifts. Handbrake (0x15) sets flag 0x8000. Steering is immediate:
  `clamp((180 − forward speed)/150, accel+0x4C, 1) · stick/128 · −π/4`; flag 0x2000 holds it at 0
  for 100 ms after a collision. No freestyle stunts.
* **Rescue loading** `0x412DB0`: under speed 30 with button 0 pressed, inside an enabled creature
  volume (type 5) whose data's first byte is set: flag 0x200, the creatures riding in the car's
  pods (type 13 slaves) tagged with the volume; always flag 0x80 (interface condition 3) until the
  car moves off (`0x412FD0`). From Armageddon's engine; no HWVX car carries pods.
* **Suspension** `0x416A40`: below `accel+0x40` speed the ride height eases to `axis[1].y`, below
  `accel+0x44` the dampers scale to `accel+0x48`; off the gas the wheels' drag winds the rpm down
  by `coastRpmScale` (on the gas not at all); grip × |ground normal.y|³; skid marks from a slip of
  3 (not 110) while braking or on the handbrake.
* **Wind sound** `0x40EF94`: pitch `|rpm/maxRpm · gears[1].soundSet.pitchScale| + 0.01` (mode 1:
  `·3 + 0.5`).
* `accel+0x40..0x4C` are past `CarAAccel`'s 0x3C bytes: the PC reads whatever follows it in the
  pack, `0, 0, 1, 0` after every car on GC and PC alike, so the ride-height and damper paths never
  apply and steering fades to nothing at 180. HWVX-SDL takes these values as constants
  (`g_carAMode2Tuning`) instead of reading out of bounds.
* No controller layout (GC/PC) binds action 4, and the PC keyboard has no key for it: on the PC a
  human in a mode 2 car could not reverse. The AI does (`AI_CarA`: button 14 when stuck).

### For docs/xdl-format/README.md §11.1 (or §11.4: the save's settings)

The byte after the audio settings (`GeneralGameSettings.audio`, +3; every release writes 0 and
none reads it) holds HWVX-SDL's **handling** choice: 0 the cars' own (`CarAInfo.driveMode`), 2 the
PC's third mode. In a JSON save it is `"settings": {"handling": 2}`, written only when not 0;
JSON saves from before it named the byte `audio_unused`, which is still read. A binary save with
it set still loads on the releases (they ignore the byte).

### For docs/fields/README.md, "PC-only data uses" (extends the CarAAccel bullet)

* CarAInfo 0x11 `driveMode`: 1 in every car (GC, PC, PS2). A mod may set 2 (the PC's third
  mode, on every release in HWVX-SDL) or 0 (no driver). HWVX-SDL also turns every mode 1 car to 2
  when the player picks Options > Handling > PC Mode 2 (`CarA.driveMode`, decided in `CarA_Init`;
  never online, in the attract demo or in a recorded demo's playback).
* The PC's mode 2 reads CarAAccel **0x40-0x4C**, past the struct: `0, 0, 1, 0` in every car's
  pack (the bytes after the accel record), on GC and PC alike.

### Release differences and the choice made

GC and PS2 agree (no mode 2 driver); the PC has one. No data uses it on any release, so the
default behaviour is the same everywhere and stays retail: the attract demos are frame-identical
to pc-data on GC, PC and PS2 data, also with `HWVX_HANDLING=2` (demos keep the cars' own
handling). The option is the port's, on all three releases' Options screens, since the code is
data-independent and the data it reads is the same on all three.

## Mouse steering (as in the PC release), in HWVX-SDL


| what | PC | notes |
|---|---|---|
| mouse steering and buttons, always on (no option) | WndProc `0x455A9B`: WM_MOUSEMOVE stores the client x/y (`0x68DDC0/C4`), WM_LBUTTONDOWN/UP and WM_RBUTTONDOWN/UP set bytes `0x68DDCC`/`0x68DDCD`; input poll `0x4568C0` (from `0x456B3B`) | The poll fills controller 1's slots after the DirectInput wheel/joystick (`c_dfDIJoystick2`; the PC opens no DirectInput mouse or keyboard). Left button: slot 9 (A) held, value 0xFF; right: slot 7 (X). Then, once a mouse move has been seen, `GetCursorPos` minus the last position: x > 4 sets the stick X (`keyValue[0x10]`) to 0x65, x < -4 to -0x65; last y minus y (up) likewise for stick Y (`keyValue[0x11]`); otherwise the stick keeps the joystick's value. The cursor goes back to (320, 240) (`SetCursorPos`) every poll, menus included, and is hidden (`ShowCursor(0)` at start, `0x455901`). So the mouse is a digital stick while it moves, with no sensitivity or dead-zone option; with the default controller layout A is gas and X brake/reverse. HWVX-SDL: `Port_Keyboard.c` / `pad.c`, in races only, off by default (launcher Settings, `HWVX_MOUSESTEER`). |
