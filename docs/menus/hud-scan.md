# Widescreen HUD: the interface scan

## Tools

* `tools/hud_scan.py --data DIR [--data DIR...] [--json out.json] [--only mixed] [--owner none]`
  loads every pack that can hold interfaces (interface, world, share, item, car, link) of each
  release in viewer mode and prints every interface: release, pack, header offset and id, owner,
  class, and each object as `index:kKIND/rROLE@x,y` plus side (L/C/R), `[lo..hi]` the x range of
  all its position keys when it differs. About 30 s for the three releases.
  `--from-json out.json` reclassifies a saved scan without running the game;
  `--textures` prints, per pack, the pack's own textures its interfaces show.
* The viewer's `interfaces` command (`{"cmd":"interfaces","pack":"world2l2/world_0"}`, a loaded
  pack): `interfaceTextures` (texture indices) and `interfaces`: per header `off`, `id`, `layer`,
  `mode`, `roots`, `refs` (the slots pointing at it, with their graph node), `events` (input
  events whose target is a header: `from` object or -1, `type`, `target`) and `objects` (`type`,
  `kind`, `role`, `anchor`, `x`, `y`, `kxmin`/`kxmax`, `w`/`h` first scale key, `text`, `child`).
* `HWVX_HUDLOG=1` (InterfaceManager.c): in gameplay, each no-owner interface as it is created,
  with its class as `Port_HudShift` sees it. The static scan cannot see every creator (the
  mission 5 map icon is named only by an EndInterface action in its world pack), so this is how
  to confirm one while playing.

## How the scan works

* Headers: a pack does not list its interfaces, so every target of a pointer slot is tried as an
  `InterfaceHeader` and kept when it fits (1..200 object definitions at a pointer slot, each of a
  type AddObject knows, sub-data, default animation and states null or at pointer slots, a child's
  slot a pointer slot). `src/game/Port_IfaceScan.c`. Rare false finds (a single object of no kind
  AddObject makes, `id600` in the PC/PS2 worlds) are classed `junk`.
* Layout x: `Port_HudLayoutX`'s maths: the default animation's first position key, or the
  anchor's; a child interface adds its placeholder's x (the tool, per parent).
* Owner: "player" for the car packs' HUDs (CarAInfo 0x84: 0x10, 0x18, 0x1C); "none" for level
  scripts' InterfacePlay actions (TAC action 4), the world's pause menu, interface packs' roots
  and input events of type 0x1F; both passed on to children and go-to targets. "?" when nothing
  in the pack creates it (`roots=end`: only an EndInterface action names it).
* Classes (kind-6 placeholders left out): left (all < 192), right (all > 448), centre, mixed.

## Results (GameCube, PC, PS2; retail)

Known-fixed ones come out as moving: mission "Wins" (587/585,73 right, worlds 1l1..5l1), the
top-right icon pairs at 567/565,74 and 587/585,73 (mission 5's map icon, mission 12's power
boxes: `right`, created from outside the pack's own scripts in some worlds, owner "?"), the
joyride and challenge counters at 538,0 (gear, crates: `right`). The battle target car and the
battle Round/kill tally are not in the scanned packs' no-owner headers (the GameCube's
HW_SHARE share_1 and MCP link_0 do not load in the viewer; quadrant packs are not scanned).

* **Real candidate: the drag races' "CHECKPOINT"** (race1l1, race1l2, race2l1, race2l2, all three
  releases; a level script's InterfacePlay, no owner). Every object's default animation starts at
  x = 0, so the interface reads as "left" and moves left as a whole; but the text's state
  animations hold it at x = 320, so on a wide screen "CHECKPOINT" sits left of the centre
  (confirmed: HWVX_HUDLOG logs `-> left`, a 1280x720 drag race shows it off-centre).
  Proposed change (not made): for an interface of no player, judge its side by each object's whole
  x range over all its position keys (default and state animations), not the first key only:
  it moves left only if every key is < 192, right only if every key is > 448. Over the whole scan
  this changes only the CHECKPOINT interfaces (left -> mixed, so they stay); every other no-owner
  left/right interface keeps its class.
* Mixed no-owner interfaces in gameplay are all screens or panels that should stay 4:3: pause,
  restart/quit, save, controller and memory card screens, briefings ("Destroy 10 CRATES",
  "Find Hidden KEY"), "GO"/"BEGIN!", time bonus texts, "NEW CAR UNLOCKED!!!", results tables
  (multi-player drag and battle rounds), "Player N WINS!!"/"Out of Time" panels. The challenge and
  joyride briefing (39 objects) holds five "00:00" timer texts at 630,16 that would stay at the
  4:3 edge, but they are not shown while it is up (checked at 1280x720 on ch1_l8); the HUD's own
  timer moves.
* Player HUDs (car packs): every object's layout x is computable (no missing key, no anchor out of
  range). One oddity to eyeball: the two player-message lines (role 61, objects 36-37) are laid out
  at x 320 (stay) but have keys down to x 16, so a message sliding to the top-left corner would
  stop at the 4:3 edge.
* world3l3 (all releases) creates a one-line interface (a kind-9 line at 0,0) that reads "left";
  lines are debug drawing, so nothing visible moves.

## Interface textures (mixed builds)

`Port_IfaceTextures(const PortIfaceScan *s, const u32 *texRelocs, u32 numTexRelocs, u8 *used,
u32 numTex)` (`src/game/Port_IfaceScan.c`, header `Port_IfaceScan.h`) marks the pack's own
textures its interfaces show: images, texture key animations, fonts, briefing characters' fonts
and animations, through the pack's texture relocations. A world pack's list includes its pause
and save screens and its level HUD textures (ch1_l3: 16 of 206). Some are shared with world
geometry (ch1_l3's texture 52 is also geometry/55's), so keeping them from the base also keeps
that geometry's copy. Indices are the release's own (they differ between releases).
