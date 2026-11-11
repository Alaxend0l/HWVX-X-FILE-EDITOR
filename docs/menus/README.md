# The menus of the three releases

Each release's menus are data: interfaces (`InterfaceHeader`, objects of 0x6C) in its interface
pack, `HW_IPack.xgc` / `.xpc` / `.xps`. The main menu is the root of the menu pack (MCP
`menuFile`); every other menu screen is reached from it through go-to events (types 1, 0x20,
0x1F) and child slots. The pause menu is in each world pack (`gameData->pauseInterface`).

This inventory was taken from the loaded data: HWVX-SDL's `HWVX_MENUDUMP=1` (`Port_MenuDump.c`)
logs every screen reachable from each interface the game creates, with its texts, rows (record
id, up/down ring) and what each row's events do (`=2` adds every object). Data: GameCube
`orig/gc/files`, PC `orig/pc` with `orig/pc-backup`'s original `HW_IPack`/`HW_Cars`/world packs,
PS2 `orig/ps2`. Object counts are the screens' own.

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
| Pause (world packs) | Resume, Restart, Controller, Save, Quit | same | same |

Notes:

* PS2 is the GameCube's menus with PS2 wording and a two-player split car select (the
  GameCube code's id-8 start event, 0x1F, counts whatever id-2 children the screen has). Its screens
  sit at other offsets in the pack, so nothing found by GameCube offsets is there.
* PC removed everything multi-player from the menu pack but kept the MCP's multi-player
  progressions and the battle worlds; its car select exists only in the one-player form.
* The PC adds Exit to Desktop with its own confirm screen; the GameCube's quit screen exists in
  all three (made for the GameCube's exit to the system menu from the memory card screens).
* The memory-card screens are the same on GameCube and PS2 (slot picker, then a file list); the
  PC's are the file list alone. The progress, failure, naming and reminder screens are alike.
* Each release draws its rows the same way (text rows 26 apart from y 198 in a panel spanning
  about 170-360); the PC's Options rows are 31 apart from y 203.
