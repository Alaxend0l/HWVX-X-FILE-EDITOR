# Plan: new pickups and new briefing screens in mods

Status: research and plan only (2026-11-20); nothing built. Paths are relative to `HWVX-SDL/`
unless they start with `docs/`. Ground rules as everywhere: a mod never holds game data
(`docs/xdl-format/README.md` §8), new tools are Rust (`tools/xdlrs`, `tools/browser`), never Python.

| | data-only for the modder? | port code needed | minimum viable feature | stages |
|---|---|---|---|---|
| A. pickups | yes, for variants of the existing behaviours | yes, once: a base pickup copied under its own pack id | a mod `pickups` entry: a copy of a base pickup with patched fields and recoloured textures, placed by a level | 5 (+1 optional) |
| B. briefings | yes (text, a PNG or a recipe) | yes: a runtime override of the briefing interface | a mod track's own briefing lines and title over its base mission's briefing layout; then a picture | 6 (+1 optional) |

## A. New pickups

### What exists

* **One pickup = one `HW_Items` pack** holding an `ItemRecord` (`src/game/include/ItemThing.h:40-58`,
  `schemas/ItemRecord.schema.json`, fields in `docs/fields/README.md` §ItemRecord). The GameCube, PC and PS2
  have 48 item packs each, the PS2 prototype 44 (`docs/xdl-format/pairing/HW_Items.jsonl`: packs 0-11, 16-17,
  19-24, 50-65, 68-79; 76-79 not in the prototype). Packs 0-11 are the 12 weapons of the item select
  (the mod `items`, not this task); 16-79 are the AI's and the worlds' own (`src/game/Port_ContentData.c:30-43`).
* **What a pickup does is chosen by `ItemRecord.type` alone**, in `CarA_ProcessItemPickup`
  (`src/game/CarA.c:2295-2482`):

  | type | effect when a car drives through it |
  |---|---|
  | 1, 2, 3, 6 | a weapon (wildcard) into the car's rack with `WeaponInfo.ammo` (`WildCardRack_Add`) |
  | 7 | carried: rides on the car (a flag to bring somewhere), dropped by scripts |
  | 8 | the car's pickup slave set number `value` (`CarAInfo.pickupSlaves`, per car) |
  | 9 | counted (`car->pickupCount++`) |
  | 10 | humans only, no effect of its own (for scripts) |
  | 11 | stunt points `value`, turbo time from them (capped 30 s), refills every rack slot's ammo |

  Creatures take only wildcards (`src/game/Creature.c:676`). Every pickup also fires the `item_pickup`
  triggers watching its spawn point (`src/game/Trigger.c:807`), which is how levels react to one. The AI
  chooses targets by type (`src/game/AI_CarA.c:1352-1355`).
* **Other fields:** `value`, `radius` (pickup reach), `height` (above its point), `spinMin/Max`,
  `pickupDebris`/`spawnDebris` (effects: links into the pack, spawned by `src/game/Thing.c:1231` and
  `src/game/FileStreaming.c:430`), `weapon` (`WeaponInfo`: ammo, HUD icon and name), `names`. Its model
  is the pack's slave set (`Item_Init`, `src/game/ItemThing.c:45`; drawn at `:162`); its textures are the pack's
  (paired per release in the pairing file); sounds come with its effects.
* **Placement:** a quadrant's `QuadItem` (`src/game/include/World.h:348-367`): position, `respawn` ms,
  `id` = the `HW_Items` pack id (0xFFFF: random in `lo..hi`), `record` null in the pack. As the world loads
  (`src/game/State_ProgressionManager.c:568-627`) each point's pack is loaded by id and `record` set from
  it; code variable 0x134 decides: 0 no pickups, 1 each point's own id, 2 random. Spawning and respawning:
  `FileStreaming_HeartBeatItems` (`src/game/FileStreaming.c:378`), every 1.5 s.
* **The editor:** a level's `items` (`{pos, quad, id, respawn}`) are compiled into the quadrant's
  table by `tools/xdlrs/src/level_pack.rs:434-470` (a copy of the quadrant's first point; the quadrant must
  already have points: `docs/level-editor.md` "The quadrant rule"). **+ Add > Item, here**
  (`tools/browser/src/level_edit.rs:1082`, record at `:815`) places pack 0; the inspector's `id`
  (`:1443`) says "the item in the world's item pack (0..)" but is a pack id: only the 48 ids above load.
  Sample: `tools/level-samples/gear-grab` (two points with id 0).
* **Mods:** `items` entries alias a weapon (`base://mcp/item/N`) and their `patch` applies to the pack
  `Port_ContentItemPack(n)` gives (`src/game/Port_Mods.c:1470-1490`). An aliasing item's pack *is* its
  base pack (`Port_ContentData.c:293-302`), and `Port_ContentPackSource` skips item packs
  (`Port_ContentData.c:316`), so today a patched copy of a pickup would change the original too.

### Data-only or code?

* **A variant of an existing behaviour is data**: other points or ammo, reach, height, spin, another
  base effect, a recolour (recipe), another weapon from a base `WeaponInfo`. That is a patch on a *copy*
  of a base pack, which needs one port change (below), after which modders need no code.
* **A new behaviour is code** (an instant repair, a timed shield, a turbo refill without points):
  `CarA_ProcessItemPickup` knows only the types above. Without code a level can already script one
  with an `item_pickup` trigger and its actions (`vehicle_give_item`, variables, timers). A port-side
  `effect` member (A6) is the clean way; it waits for the user's answer.
* **A new model** needs geometry in mods (glTF, not loaded by the port yet): out of scope. A pickup
  takes the look of the base pickup it extends, recoloured.

### Minimum viable feature

A mod `pickups` entry, a copy of a base pickup under its own pack id, placed by levels by its uid:

```jsonc
"pickups": {
  "uid://bigpoints": {
    "extends": "base://HW_Items/item_50",          // its model, effects and sounds: the player's own
    "name": "Big Points",
    "patch": { "ItemRecord/0": { "value": 500, "radius": 12 } },
    "recipes": { "texture/0": [ { "op": "hue_shift", "degrees": 200 } ] }
  }
}
```

The registry numbers it past the archive's last pack (80, 81, ... on retail; one allocator shared with
`Port_ContentItemPack`'s own-data items), kept in `ids.lock` under `pickups`. `DataPack_LoadFile`
loads the alias's pack under that id and renumbers its script id (`DataPack_GetScript` matches it,
`src/game/DataGamePackLoad.c:1087`), as an extra car already does; the patches and recipes then apply
to that pack only. A level point says `"id": "uid://bigpoints"`; the level file keeps the uid and
`Port_Level.c` writes the machine's number into `QuadItem.id` as it applies the level (numbers differ
between machines). A level whose pickup's mod is missing is refused with a message naming it.

### Stages (each one agent-day or less)

1. **Inventory (Rust).** `xdlrs items inventory DATA...` (like `xdlrs cars stats`): per release and
   `HW_Items` pack: type in words, value, radius, height, spin, name, weapon and ammo, effects, texture
   count, and which worlds place it (from `QuadItem.id`). Writes `docs/fields/pickups.md`. Test: counts
   48/48/48/44; every type of the table above found; ids placed by worlds all exist.
2. **Port: pickup copies.** Registry kind (or item flag) for pickups, `ids.lock` `pickups`,
   `Port_ContentPackSource` for type-2 `HW_Items` ids past the archive, script renumbering, patches and
   recipes on that pack only, `HWVX_TEST_EXTRA_PICKUPS=N` (copies of 50, 51...). Test: `test_content`
   scenario (ctest, headless) greps "pickup uid://...: pack 80 = item_50"; `HWVX_MODS_LOG=1` shows the
   patched fields on pack 80 and none on pack 50; the attract demo frame-identical.
3. **Levels place them.** `Level.schema.json` items `id`: number or uid; `xdlrs level compile/check`
   (check: a uid the project's mod or an installed mod brings); `Port_Level.c` resolves it. Test:
   gear-grab with one point on the copy; `tools/run_test.sh BUILD DATA OUT gg 40
   HWVX_START=race:mode=joyride,world=uid://gearGrab` with the sample's autopilot route: the log shows
   the pickup on pack 80 and the `item_pickup` trigger firing; `HWVX_DUMP` frames show the recoloured
   pickup next to a retail one.
4. **Schema and packer.** `Mod.schema.json` `pickups`; `xdlrs mod check/pack` (modcheck): known fields
   only, a patch restating the base refused, `type` changes checked (1-3 and 6 need `weapon`; 8 needs a
   `value` every car's slave sets have) or refused; a sample in `tools/mod-samples`. Test: `cargo test`
   in `tools/xdlrs`; the sample packs with no game data found.
5. **Editor: "New pickup".** See below. Test: the browser's own tests (model round trip: a pickup made,
   saved, reopened; the level's point refers to it), then a test drive from the editor.
6. *(Optional, after question 2)* **Port effects.** `"effect": {"kind": "repair", "amount": 0.25}` etc.,
   a hook before the type switch in `CarA_ProcessItemPickup`; online sends the effect with the pickup.

### Editor side (hwvx-browser)

* A **Pickups** panel (next to the Level tab's lists, saved in the level's mod project): the base pickups
  from stage 1 (name, type in words, a texture thumbnail) and the project's own; **New pickup from this**
  makes an entry and opens a form with only the fields its type reads (points for 11, ammo for weapons;
  reach, height, spin always), effects as a dropdown of base effects, recipe sliders (hue, tint,
  brightness) with a live preview in the viewer.
* The Level tab's item inspector: `id` becomes a picker of base pickups and the project's own (fixing the
  "(0..)" tooltip); **+ Add > Item, here** offers the last one picked.
* Checks shown live: a pickup no level uses, a level point on a pickup that doesn't exist.

### Risks

* **Online:** both machines need the mod; pickups are decided by their owner (`Port_OnlinePickup`) and
  content choices go by id (`Port_ContentWriteChoice`): pickups need the same, or online refuses levels
  with mod pickups at first.
* **Pack ids past 79** collide with own-data items unless one allocator numbers both.
* **Game options:** code variable 0x134 = 0 turns every pickup off; random points (`lo..hi`) never pick a
  copy.
* **Per release:** `extends` a pack the player's release lacks (76-79 on the prototype) must fall back
  or be refused; `pad2C` (a pointer in item 51 only) and `pad34` are not understood.
* **Type changes** can crash (`WildCardRack_Add` reads `weapon`), hence stage 4's checks.

### Questions for the user

1. Which pickups do you mean in-game (what does the player see: the wildcards, the points pickups, the
   collectibles)? Stage 1's table will name them; is a turbo or repair pickup expected to exist already?
2. Are new behaviours wanted (repair, shield, turbo), or are variants of the existing ones enough?
3. Pickups in mods shared by many levels (this plan), or private to one level?

## B. New briefing screens

### What exists

* **A level's opening** is its progression level's `steps` (`ProgLevel`, `src/game/include/common.h:663-686`),
  played by `State_ProgressionManager.c:184-245`: type 0 a movie, type 1 an interface pack `{name, id}`
  created at `:227`. Mission (adventure) levels show their briefing this way from `HW_brief`: 14
  interface packs (`interface_0`-`13`, one per mission, 133-200 textures and 1-2 sounds each; pairing in
  `docs/xdl-format/pairing/HW_brief.jsonl`), voices in the `HWB_M01`-`14` audio packs. Then phase 1 shows
  the loading screen (`level->pack`, `:254`).
* **A mod track** gets its progression level as a copy of its base level's (`Port_ContentData.c:1086-1095`):
  same steps, so the base mission's briefing; only `world` is overridden. The level file is applied
  when the world loads (`Port_LevelWorldLoaded`, `State_ProgressionManager.c:412`), after the briefing, so
  it can't carry the briefing as it is.
* **A briefing interface** (`InterfaceHeader`, `src/game/include/Interface.h:470`) has images, texts,
  boxes, music, and one dialogue object (`IFACE_DEF_DIALOG`, `Interface.h:278`): a list of entries
  (`Interface.h:426-462`): speaker portrait id, `time` ms, a voice (`soundName`/`soundID`), `text` (a
  string table), the character (`font`, text `color`, idle and talk texture animations: the portraits),
  an action list. `InterfaceManager_PreprocessInterface` (`src/game/InterfaceManager.c:778`) loads the
  voices and fonts; `InterfaceManager_MoveDialogSystem` (`:2712`) steps the entries: role 0x21 text
  objects get the entry's text (`SetObjectText`, `:897`), role 0x22 images the speaker's animations,
  0x23/0x24 the character's values. The dialogue object's `data` is per interface instance, so it can
  point at a list of the port's own without touching the pack.
* **Challenge and Joyride** have no `HW_brief`: their intro screen is an interface the world's script
  plays (`interface_play`; `tools/level-samples/checkpoint-run/triggers/intro.jsonc`), whose box shows an
  objective's text through role 0x34 (`InterfaceManager.c:843-858`, `SetMissionObjectiveText` `:5359`):
  the first `WorldObjective` whose `info->id` is the object's id. A level's own objectives are appended
  after the base's (`Port_Level.c:1552`, `:1978`), so the base's matches first: the limit in
  `docs/level-editor.md` §10.
* **Testing:** `HWVX_START=race` skips the opening (`Port_StartSkipsOpening`, `src/game/Port_Start.c:533`);
  `HWVX_MENUDUMP=1` logs interfaces and the progression scripts' worlds (`Port_MenuDump.c:215-232`), not
  their steps.
* **Pictures:** the port has no PNG decoder (`src/third_party`: miniaudio, stb_vorbis) but does make
  RGBA8 textures at run time in slots past `PORT_MAX_TEXTURES`, swapped in at draw time
  (`src/game/Port_Prompts.c:240-275`). Mod recipes recolour a base texture everywhere (`gx_texmod.c`), so
  a recipe on a briefing texture would change the base mission's briefing too.

### Smallest change

Override the briefing at run time, per interface instance, only while that track plays; no pack bytes
change, nothing of the base is copied into the mod.

* `level.jsonc` gets `briefing`; `xdlrs level mod` copies it into the mod's `tracks` entry
  (`Mod.schema.json`), which the port reads before the world loads (beside `Port_ModsTrackLevel`,
  `Port_Mods.c:1505`):

  ```jsonc
  "briefing": {
    "title": "Two Gates",
    "lines": [ { "who": 0, "text": "Two gates, one clock." }, { "who": 1, "text": "Gate 1 first!" } ],
    "picture": "briefing/gates.png"    // or { "texture": "base://HW_brief/interface_0/texture/12", "recipe": [ ... ] }
  }
  ```
* A new `src/game/Port_Brief.c`, called after the opening's `InterfaceManager_Create` when
  `Port_ContentTrackOfLevel` is a registered track with a briefing: the dialogue object's `data` becomes
  the port's list, each line a copy of the base entry of speaker `who` (portrait, font, colour kept),
  its own text, no voice, a `time` from its length; the title through `SetObjectText` on the title
  object (found in B1); freed when the interface ends.
* Challenge and Joyride: role 0x34 prefers the level's own objective with that id (a few lines), so a
  level's objective with the intro box's id gives the box its text; the editor writes that objective.
* The picture: an RGBA8 slot drawn in place of one image object's texture, made from the mod's PNG
  (stb_image, PNG only, sizes the game uses: powers of two up to 256) or from a private copy of a base
  texture with the recipe applied.

### Stages (each one agent-day or less)

1. **Inventory (Rust + log).** `xdlrs brief inventory DATA...`: each `HW_brief` interface's objects
   (role, type, id, place, font, text), dialogue entries (speaker, time, voice, text), the longest line
   per text box; and `HWVX_MENUDUMP` logs each progression level's steps ("level 3: HW_brief 3").
   Writes `docs/menus/briefings.md`. Test: `tools/run_test.sh BUILD DATA OUT md 15
   HWVX_START=menu:mode=mission HWVX_MENUDUMP=1`; grep the 14 missions' steps.
2. **Keep the opening in tests.** `HWVX_START=race:...,opening=1` plays phase 0. Test: run_test on
   mission 1; `HWVX_DUMP` frames show the retail briefing; without `opening=1` runs are unchanged.
3. **Port: own lines and title.** `Port_Brief.c`, the `tracks` entry's `briefing`, the schema; voices
   off for own lines. Test: two-gates with a briefing, `opening=1`: the log says "brief: uid://twogates:
   2 lines over HW_brief/interface_0"; frames at 3 s and 8 s show the lines; retail mission 1's
   briefing frame-identical to before; the menudump shows the texts.
4. **Challenge and Joyride intro text.** The role 0x34 preference; `xdlrs level` writes an objective
   with the intro's id from `briefing.lines[0]`. Test: checkpoint-run's intro frame shows its own
   text; a retail challenge unchanged; `tools/level_test.sh` passes.
5. **Picture.** stb_image in `src/third_party` (provenance in its README), the RGBA8 slot and draw-time
   swap, the recipe variant on a private copy; `xdlrs mod check` already refuses a PNG that looks like
   a base texture. Test: frames show the picture; a bad PNG (too big, not PNG) is logged and the base
   picture stays.
6. **Editor: "Briefing".** See below. Test: browser tests (round trip of `briefing`), then the
   editor's test drive with `opening=1`.
7. *(Optional)* **Voices:** a mod WAV per line, through the dialogue's sound pack path.

### Editor side (hwvx-browser)

* A **Briefing** section in the Level tab: the base level's briefing (stage 1's inventory, or "none:
  intro box text" for Challenge and Joyride), then the level's own: title, lines with a speaker picker
  (the base characters, with portrait thumbnails), a counter per line against the box's limit, and a
  picture (a PNG chosen from disk and copied into the project, or a base texture plus recipe sliders).
* **Preview** runs the test drive with `opening=1`, so the briefing plays first.
* Checks: a line too long for the box, a speaker the base briefing lacks, a PNG of the wrong size.

### Risks

* **Unknown layout:** which object is the title, how long a line fits, whether talk animations wait for
  the voice (`dialogSpoken`): stage 1 settles this before stage 3.
* **Entry actions** (the list at entry 0x18) may drive the interface (go-to, end): copying a base
  entry's actions onto every line can end the screen early; keep the base's last entry's actions on the
  last line only.
* **Releases differ** in interface packs (wording, object counts): find objects by role, not index;
  mixed builds (`docs/xdl-format/ultimate-interface.md`) too.
* **Memory:** the port's list, strings and texture slot must be freed when the interface ends or the
  level restarts.
* **Voice mismatch** if the base voice is kept with new text.

### Questions for the user

1. Own lines silent, or keep the base voice (which won't match)?
2. Speakers: the base characters only (this plan), or own portraits later?
3. Should Challenge and Joyride levels get a full mission-style briefing (a new opening step using a
   mission's layout), or is the intro box's own text enough?
4. Where should the title show: the briefing's own heading object (if one exists), or the first line?
