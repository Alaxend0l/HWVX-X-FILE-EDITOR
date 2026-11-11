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
| `orig/pc-backup/Hot Wheels.exe` | `91af8432…` | **retail**. Match and disassemble against this one |
| `orig/pc/Hot Wheels.exe` | `5485ea3f…` | car-limit patch from `pc_exe.md` (35 bytes in two caves) |

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
items and music. Still to do: Bink movies (skipped), a pass over the PC interface screens with the
port's menu patches, PS2 data, vertex caching for the indexed path.

### Geometry plan: the PC layout as the port's own

Pack geometry reaches GX in one place, `Render_DrawGroupPrims` (`Render_Geometry_.c`): a vertex
setup, the LOD's arrays (`LOD+0xC`), the group's display list (`+0x1C`, size `+0x10`). Everything
else that touches geometry is layout-independent: LOD choice and culling (stride 0x18, distances,
bounding boxes), the draw lists (`+0x10` only tested for non-zero), radius from the bounding box.
Shadow volumes have their own format, the same on both, read by `Render_DrawShadows`. World
quadrants draw through the same path (`World.c` → `Gfx_DrawGeometry`).

PC geometry is drawn in that layout directly: `Port_Data.c` finds a pack's (and a quadrant pack's)
geometries through the pointer list and local relocations (header `0x0002`), marks their drawn
levels `PORT_LAYOUT_INDEXED` in the level's `u16` at 0 (never read by the game), and flags their
groups' texture relocations so the id lands at `+0x1A`. `Render_DrawGroupPrims` sends marked
levels to `GX_PortDrawIndexed` (`gx_mesh.c`), which builds vertices the way the display-list
decoder does. Skinned levels take each vertex's bone from the per-bone count table at `LOD+0x10`
(bone k is matrix slot k). The next step is converting GameCube geometry to the same layout on
load (the `gc2pc_geo.py` rules; bone from `pnmtx/3`).

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
