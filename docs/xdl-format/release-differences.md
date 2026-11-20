# What differs between the retail releases (for mods and the editor)

Checked with the three retail releases' own data on one machine (November 2026): the GameCube's
extracted files, the PC CD (`ims-hwvx.cue` / `.bin`) and the PS2 DVD (`SLUS-20412 (1.00).iso`),
ripped with `xdlrs disc extract` (HWVX-SDL `tools/xdlrs/src/disc`). Each point says what was
checked, how, and whether it was **verified** (the same everywhere, or handled), **fixed** (wrong
before, with the fix and its test) or **open**. The tests read the data when it is there and skip
without it: `HWVX_TEST_DATA` / `HWVX_TEST_GC_DATA` (GameCube), `HWVX_TEST_PC_DATA`,
`HWVX_TEST_PS2_DATA`, `HWVX_DATA` (the editor's integration tests); the PC's and the PS2's are
the ripper's folders (`orig/pc`, `orig/ps2`).

## 0. The PC disc's .cue (fixed)

The retail PC CD's `ims-hwvx.cue` names `IMS-HWVX.BIN`; the file beside it is `ims-hwvx.bin`.
Windows and macOS find it either way, Linux didn't: `xdlrs disc info` and the launcher's import
(`pcdisc.c` `CueTrack`) both failed with "could not open". Both now look the name up without
case when the exact spelling isn't there (the launcher through dvd.c's `Port_ResolveCase`).
Tests: `xdlrs` `disc::tests::cue_track_nocase`; `tools/test_pcdisc.c` (a crafted .cue and track:
found without case, `Sub\TRACK.BIN` too, a missing one still fails).

## 1. The ripper on the PC and PS2 discs (verified)

| | PC CD | PS2 DVD |
|---|---|---|
| `xdlrs disc info` | release pc, ISO 9660 in raw 2352-byte sectors, 110 files, 738.6 MiB (image 663.1 MiB: HWVX.cab's MSZIP) | release ps2, archive version 183, 110 files, 643.6 MiB (image 1.6 GiB) |
| `xdlrs disc extract` | 2.7 s wall (page cache warm), 6.6 MB of memory | 1.1 s, 7.0 MB |
| layout | HWVX.cab's files flat, the movies in `FMV_/` | `XPS/ENGLISH/` |
| the launcher's import of the same image | 4.1 s | 0.2 s |
| same files as the launcher's import | 110 of 110, byte for byte | 110 of 110, byte for byte |

The comparison is `tools/test_pcdisc.c` with the real discs (`HWVX_TEST_PC_DISC`/`_DATA`,
`HWVX_TEST_PS2_DISC`/`_DATA`): the launcher's `Port_ImportPCDisc` / `Port_ImportPS2Disc` into a
folder, every file compared with the ripper's. The game runs on both folders (`tools/run_test.sh`
with `--data`): the attract demo plays its levels (bat2_l9, race3l2, bat4_l1, ...) with the same
screens as the GameCube's, and the log has no load errors (`DVDOpen: bsfile not found` is the
GameCube's disc-check file, which the other releases don't have; `g_pFree NULL` is in the
GameCube's log as often).

## 2. Points for the Content tab

### 2.1 New track: names and world select pictures (fixed: the PC)

* **Names** (verified): the world selects' 57-entry track list (`HWVX_MENUDUMP`, every mode's
  select) is the same text on all three releases.
* **Pictures**: the world select pictures are HW_IPack interface pack 6's textures, texture =
  key − 3 on every release, and drawn bottom row first everywhere (checked against the screen of
  `HWVX_START=worldselect:mode=challenge` on the GameCube and the PC). The PS2 lists the same
  keys as the GameCube. **The PC's keys differ**: missions +6 (25–38), challenges +4 (49–66),
  drag and battle pictures +3, +4 or +6 (its pack has 135 textures in another order; the
  GameCube's 138, the PS2's 139). A thumbnail link (`base://HW_IPack/interface_6/texture/N`) was
  the running release's texture N, so a GameCube-made track showed one of the select's small
  status icons on the PC.
* **Fix**: a link's N is the GameCube's number of the picture on every release; the game finds
  the same picture in the PC's pack (`Port_ContentData.c` `WorldPictureOn`, the two releases'
  key tables), and `texture/<release>:N` (gc, pc, ps2) names one release's texture only (the
  alias's picture elsewhere), as the schema already allowed. The editor previews from the PC's
  own texture (`content_tracks.rs` `release_texture`) and finds HW_IPack in the PS2's layout.
* **Tests**: `test_content` scenarios `thumb`, `thumb-pc`, `thumb-ps2`, `thumbonly`,
  `thumbonly-pc` (an extra challenge track with challenge 3's picture: key 47 on the GameCube and
  the PS2, 51 on the PC); the editor's `world_select_pictures_per_release` (every picture it
  decodes from the PC's and the PS2's data is the GameCube's most alike, 64 × 64 grey, ≥ 0.93;
  with the GameCube's numbers on the PC it fails at the first picture).

### 2.2 The multi-player drag worlds the PC hasn't (fixed)

The PC's data has no `race1l1`–`race6l1` (multi-player drag); its MCP still lists them
(progression 5). Its menus already leave Drag Race out of Multi-Player (the port's "no
multi-player drag race worlds"). But `HWVX_START=race:mode=mpdrag,world=race1l1` on the PC sat on
the loading screen for ever, and so would a mod track over one of those worlds.

* **Game**: `Port_DataHasPack` (dvd.c) says whether a world is in the data. HWVX_START on a world
  the data hasn't logs `start: race1l1: not on this release (the data has no
  WORLDS_\MP_\DRAG_\race1l1); HWVX_START ignored` and stays on the title screen; a registered
  track whose world isn't there is left out (`content: ...: its world ... is not on this
  release; left out`; a mod's own `@mod\`/`@world\` worlds aren't looked for). A mod naming one
  of those songs is harmless: the song is never asked for on the PC.
* **Editor**: it reads the data's release and archives (`content.rs` `DataInfo`, through
  `xdlrs::disc`: a folder or an image). The Soundtrack list shows the six songs as "not on this
  release (PC)" and doesn't offer them (54 of 60 songs on the PC, 60 on the GameCube and the
  PS2); a mod's song for one is listed with that note; its game test is refused. A track whose
  level is over such a world says so in its inspector. New track's base list is the single-player
  families only (drag is race1l2–race6l2, which the PC has): nothing to change.
* **Tests**: `test_content` `mpdrag` (the GameCube races on race1l1), `mpdrag-pc`,
  `mpdragtrack-pc`; the editor's `songs_per_release` (0, 6, 0 missing) and its
  `songs_match_the_viewer` integration test, now per release (it assumed the GameCube's layout).

### 2.3 Car sounds by index (verified)

`sound/N` keys are the GameCube's indices and apply on every release. Checked with the data: all
33 car packs have as many sounds on each release (97 in all), and each index has the same role
from the car's CarAInfo (wind, engine, rolling, gear shifts, unused sets; "used by the car" at a
data offset that differs by release, as the data sections do). HW_share's pack (15 sounds) and
HW_Items' 15 packs with sounds have the same counts. No release keys are needed. Test: the
editor's `car_sound_roles_per_release`.

### 2.4 Paint on textures the releases number otherwise (fixed)

A recipe's `texture/N` is texture N of the release the game runs (`Port_Mods.c` `ApplyRecipe`).
From the pairing table (`pairing/HW_Cars.jsonl`, `HW_Items.jsonl`), 86 car and weapon textures
don't have the same number everywhere:

* the PS2 has extra car textures before the shared ones (car_1, 3, 7, 20: `ps2:0` or `ps2:3`)
  and its own order in car_15, 20, 24 (two swapped); its share_2 textures 21–22 are its own;
* the PC lacks car_3's texture 3 and car_13's texture 2, its later numbers one less;
* most weapons (item_0–7, 10, 11) have a second texture only the GameCube has; the PC's numbers
  after it are one less, the PS2 has another of its own there; item_4 has a PC/PS2 one of its own
  (`pc:2`), item_51, 68 and 69 a PS2 extra, and item_77's fourth is the GameCube's only.

So a recipe made on one release didn't do nothing elsewhere: it painted another texture. The
paint now writes `texture/N` where every release numbers the texture N, and otherwise a recipe
per release that has it under its number there (`texture/gc:4`, `texture/pc:3`,
`texture/ps2:5`); the inspector names the other releases' numbers, or that one hasn't it. The
numbers are the editor's `TEXTURE_ROWS` (content_retail.rs). Tests: `texture_numbers_by_release`
and `texture_numbers_against_the_data` (each car and weapon pack's textures by the GameCube's
numbers found where the table says on the PC's and the PS2's data: 194 and 196 textures).

Open: car_26's PC geometry differs from the GameCube's (pairing.md §5); nothing in the editor
names geometry, so nothing to warn about yet. The PS2 prototype (`ps2beta`) isn't in the table.

### 2.5 Custom worlds' textures (verified for world1; open for other bases)

A custom world's materials are indices into its base level's world texture pack (the sample
ramp: Joyride world1, `w1_tex` 4, 9 and 32). The pairing table has `w1_tex`'s 124 textures at the
same index on all three releases, so the sample names the same textures everywhere. Built with
`hwvx-x world build tools/world-samples/ramp/demo.jsonc` and driven with
`tools/world_drive.sh` on each release's data: it composes over world1, the car starts at the
world's start and climbs the ramp (highest y 106 on each), and the ground and walls look the same
in the three releases' frames.

Other bases are not all alike (world texture packs, GameCube number ≠ the other's):

| pack | PC differs | PS2 differs | of |
|---|---|---|---|
| w1_tex, w1r1_tex, w2_tex, w2r1_tex, bat1–5, 7, 8 | 0 | 0 | |
| bat6_l6 | 1 | 1 | 74 |
| w2r2_tex | 4 | 0 | 79 |
| w3_tex | 21 | 2 | 117 |
| w3r1_tex | 43 | 1 | 103 |
| w4_tex, w4r1_tex | 1 | 1 | 132, 42 |
| w5_tex | 37 | 0 | 75 |
| w5r1_tex | 39 | 0 | 52 |

Open (for the user to decide): a world over a world-3 or world-5 base would show other textures
on the PC. The game could map the GameCube's numbers per release as it does the world select
pictures (a table per pack from the pairing table), or the builder could refuse those bases.

### 2.6 The editor's game runs on the PC's and the PS2's data

The editor's integration tests, on each release's folder (`HWVX_DATA`, `HWVX_TEST_DATA`):

| test | PC | PS2 |
|---|---|---|
| `testdrive` (the "Drive it" route the Level and Content tabs share: compile, launch, in the race and driving, leave by the pause menu; from here; Stop; Save as mod; both routes) | 6 of 6 | 6 of 6 |
| `content_music` (`songs_match_the_viewer`, `replacement_plays_in_the_game`: a mod's song plays in the race, its loop and never past it) | 2 of 2 | 2 of 2 |
| `track_picture_game` (a track's own PNG in the world select, right way up) | passes | passes (failed 1 of 6 runs: a frame without the picture, not looked into) |
| `frames`, `level_edit` | pass | pass |
| the editor's library tests, with the three releases' data together | 37 of 37 | 37 of 37 |

What they found: `songs_match_the_viewer` assumed the GameCube's layout and every world (now per
release); the PS2's disc spells every archive in upper case (`HW_SHARE.XPS`), so the Sounds
section's general list and its keys were `base://HW_SHARE/...` on the PS2's data (now the
GameCube's spelling, `content.rs` `archive_name`; the game matches either). `protocol` and
`world` fail on the GameCube's data here as well (the viewer's HTTP check, a blank world frame on
this machine), and `media`'s failing test is `media_gamecube`, the GameCube's formats only: not
release differences.

## 3. Seen on the way (open)

* The challenge select's second picture list (object 56: keys 35–39 on the GameCube, the
  select's small pictures) gets an extra track's thumbnail too (`pictures 19 (obj 56)`, entry 18:
  its world picture), on every release (not checked on screen). Port_ContentData.c extends both
  lists as pictures.
* `test_content` finds the GameCube's data only at `orig/gc/files`; this machine has
  `orig/gc/HWVX GC RIP/files`, so `ctest` skips it unless `HWVX_TEST_GC_DATA` is set.
