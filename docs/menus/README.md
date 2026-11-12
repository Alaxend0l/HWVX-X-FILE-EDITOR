# The menus of the three releases

Each release's menus are data: interfaces (`InterfaceHeader`, objects of 0x6C) in its interface
pack, `HW_IPack.xgc` / `.xpc` / `.xps`. The main menu is the root of the menu pack (MCP
`menuFile`); every other menu screen is reached from it through go-to events (types 1, 0x20,
0x1F) and child slots. The pause menu is in each world pack (`gameData->pauseInterface`).

This inventory was taken from the loaded data: HWVX-SDL's `HWVX_MENUDUMP=1` (`Port_MenuDump.c`)
logs every screen reachable from each interface the game creates, with its texts, rows (record
id, up/down ring) and what each row's events do (`=2` adds every object, with their positions and
conditions). Data: GameCube `orig/gc/files`, PC the CD as the HWVX-SDL launcher imports it (the
pristine data; `orig/pc-backup`'s packs have been modified), PS2 `orig/ps2`. Object counts are the
screens' own.

## Inventory

| screen | GameCube | PC | PS2 |
|---|---|---|---|
| Main menu | 26 objects: One Player, Multi-Player, Load Game, Save Game, Options, Extras | 21: One Player, Load Game, Save Game, Options, Extras, **Exit to Desktop**; no Multi-Player | as GameCube |
| Quit / exit | "Quit Game / Confirm Quit" (27), *Yes, Quit* = event 0x19 (exit to the system menu); only reached from Save Game → Manage Memory Card | "Exit to Desktop / Confirm Exit" (28): the main menu's last row, and B on the main menu; *Yes, Exit* = event 0x19 | as GameCube, but its warning text and the "Manage Memory Card" label are blank |
| One Player | Adventure, Challenge, Drag Race, Battle, Joyride (20) | same | same |
| Multi-Player | 16: Drag Race, Battle | **none** | as GameCube |
| MP options | Drag Race Options (26: Wins, Gadgets, Item Reset; sets code var 0x133 = 1), Battle Options (34, as One Player's) | **none** | as GameCube |
| MP world select | Drag Race / Battle Select: progression 5 / 7 (MP drag, MP battle), then the split car select | **none** (the MCP still has progressions 5 and 7) | as GameCube |
| Split car select | header id 8, 22 objects: four id-2 children (74 objects each), "First" .. "Fourth Player", join and Ready per player | **none**: one car select per pack | header id 8, 11 objects: **two** id-2 children (70 each), "First"/"Second Player", and a one-object screen before the start |
| One-player car select | id 8 wrapper (6) holding the id-2 car select (84); Joyride goes on to the item select (id 3, 78) | same | same |
| Options ("Game Settings") | 40: Controller, Cheats, Difficulty, Sound Mode, Music, SFX, hidden debug row | 33: **no Sound Mode** | as GameCube |
| Controller | 53: Player, Rumble Feature, Config # | 48: keyboard remap (13 actions, Done, Reset to Default) | 59: "Vibration Function" for Rumble |
| Load Game | id 5, 46: Memory Card Slot A/B picker, then 8 files; Format Memory Card, Delete Corrupt File | id 5, 21: the file list only | " Memory Card Slot 1/2", "Format Memory Card (PS2)" |
| Save Game | id 6 wrapper → 48: slot picker, files, Manage Memory Card, Format, Delete; naming (id 7) | id 6 → 21: file list; the same naming screen | as GameCube |
| Loading / saving | "LOAD GAME" (15), "SAVE GAME" (14): progress, then Retry / Continue without loading | same | same |
| Settings reminder | "Be sure to save your game..." (12), shown on leaving Options | same | same, "...Velocity X Maximum Justice*." |
| Extras | Credits, FMV (play all), Game Bootup, Adventure Start, Adventure End | same | same |
| Pause (world packs) | Resume, Mission Info, Restart, Controller, Save, Quit | same, and **Exit to Desktop** under Quit (39 objects) | same as GameCube |

Notes:

* PS2 is the GameCube's menus with PS2 wording and a two-player split car select (the
  GameCube code's id-8 start event, 0x1F, counts whatever id-2 children the screen has). Its screens
  sit at other offsets in the pack, so nothing found by GameCube offsets is there.
* PC removed everything multi-player from the menu pack but kept the MCP's multi-player
  progressions and the battle worlds; its car select exists only in the one-player form. The
  multi-player drag race worlds are gone too: progression 5 still lists `WORLDS_\MP_\DRAG_\race1l1`
  .. `race6l1`, but the CD has only the one-player `race1l2` .. `race6l2` (progression 2), and a
  one-player drag world spawns one human only (tried: two players in progression 2 race alone).
  The battle worlds (`bat1_l1` ..) serve both progression 3 and 7.
* The one-player car select (id 2, 84 objects) is laid out the same in all three: title
  "Vehicle / Select" at y 78, the car windows at 126, the car's name at 205, the stats from 295.
  The GameCube's split pane (74 objects) is that top strip with a join step (rows setting code var
  0xC9 = 2, a "Press A to join" child, "Ready"); the id-8 start (event 0x1F) counts the id-2
  children and starts once every one is at 0xC9 = 3 and at least two are, with more than one.
* Code variable 0x258, which the interfaces test as a platform id: GameCube 2, PS2 1, PC 0 (the PC
  exe's `GameSettings_GetCodeVariable`, `0x42A680`, sends it to its default case). The PC's One
  Player, Extras and options screens show some decoration only on 0.
* The PC adds Exit to Desktop with its own confirm screen; the GameCube's quit screen exists in
  all three (made for the GameCube's exit to the system menu from the memory card screens).
* The memory-card screens are the same on GameCube and PS2 (slot picker, then a file list); the
  PC's are the file list alone. The progress, failure, naming and reminder screens are alike.
* Each release draws its rows the same way (text rows 26 apart from y 198 in a panel spanning
  about 170-360); the PC's Options rows are 31 apart from y 203.

## In HWVX-SDL: one set of menus

HWVX-SDL's `src/game/Port_Menu.c` patches the menus as each release's pack has them when they are
created, finding screens by content (texts, record ids, go-to targets) and building anything new
from the loaded release's own objects; nothing is copied into the executable or taken from another
release's pack. The result, with default settings:

| | GameCube | PC | PS2 |
|---|---|---|---|
| Main menu | One Player, Multi-Player, Load, Save, Options, Extras, **Quit Game** (the pack's Confirm Quit screen) | One Player, **Multi-Player**, Load, Save, Options, Extras, Exit to Desktop (its own) | as GameCube |
| Multi-Player | port screen: Online, Local (the pack's Multi-Player) | the same, Local opening a Multi-Player screen **built from One Player's**: Battle only (no drag worlds) | as GameCube |
| MP options, world selects | the pack's | built from One Player's: Battle Options going to a copy of Battle Select picking progression 7 (and, where the drag worlds exist, Drag Race Options without Opponents, 0x133 = 1, and Drag Race Select picking 5) | the pack's |
| Split car select | the pack's, four players | built: the car select wrapper with **two** copies of the one-player car select showing only their top strip, "First / Player" and "Second / Player", one controller each; A picks (no join step) | the pack's, two players |
| Options | Graphics row and screen | the same (built from its five-row Options) | the same |
| Load / Save | slot A's file list straight away | the PC's own file lists | as GameCube |
| Pause, multi-player | no Save, rows under it moved up | the same, Quit and Exit to Desktop moved | as GameCube |
| Battle Options, online | no Opponents row | the same | the same |

`mainmenu=original` leaves each release's main menu as its pack has it. What is not there: PC
Drag Race in Multi-Player (no worlds for it on the CD), and PC split screen for three or four
players (the car select strip is about 160 units tall; two fit the 480-unit screen, four would need
the panes scaled, and the interface code only offsets children, never scales them).

## Widescreen HUD (HWVX-SDL)

On a screen wider than 4:3 the port keeps the 640x480 layout in the middle and moves the in-game HUD's
side elements out to the screen's edges (`Port_HudShift`, src/game/InterfaceManager.c). An element's
side comes from its layout x (its default animation's first key, plus its parent's or anchor's): left
of 192 moves left by the width of the side area, right of 448 moves right, the middle stays. Gameplay
only; menus keep their 4:3 layout (letterbox bars and backdrops are stretched or zoomed instead).

* **A player's interfaces** (the HUD): each element by its own side. The target arrow (role 0x1F) is
  a camera-space point, not a screen position, and is left alone.
* **Child interfaces** (drawn by a kind-6 placeholder object of the HUD): the placeholder never moves
  (it sits at x = 0, which would read as "left"), and each of the child's elements moves by its own
  side: the battle's child has "Round" on the left (x 140) and the kill tally on the right (480-587),
  so moving it as one group would leave it all in the middle.
* **Interfaces of no player** (the battle's target car with its X, a mission's "Wins" beside the
  timer): the whole interface moves when all of it lies on one side; ones spread across the middle
  (the pause menu) stay. Found by attaching a debugger to a running game: "Wins" is a two-object text
  interface at x 585-587, beside the count at 628.
  The joyride gear icon is covered by the same rule.
* Not covered: 3D objects drawn in the HUD (a car or thing placed in front of the camera): their
  position is in world units, not pixels. None is known to be misplaced.
* Tools: `tools/run_test.sh` `WINDOW=WxH`; `tools/aspect_shots.sh` runs the same moment at several
  window sizes and puts the frames side by side.
