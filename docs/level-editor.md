# The HWVX level editor: user guide

The level editor is part of the **data browser** (`hwvx-browser`). It makes new levels for Hot
Wheels: Velocity X in the game's own worlds: a Challenge, a Mission, a Battle arena round, a Drag
Race or a Joyride. You place checkpoints, starts and pickups in the 3D view, write the level's
rules as triggers, and press **F5** to drive it in the game straight away.

A level is a small folder of text files (`level.jsonc` and `triggers/*.jsonc`). It holds only
what you added and changed. It never contains the game's data: it links to records in the
player's own copy of the world, and the game applies it when the level loads.

## Contents

1. Getting started
2. The screen
3. Editing
4. Adding and deleting things
5. Starts and the spawn box
6. Each mode
7. Test drive
8. Saving, undo and redo
9. Sharing your level (packaging)
9b. The Content tab (new cars, weapons, tracks, sounds and music)
9c. The Media tab (movies and the Audio browser)
10. Limits and known issues
11. Troubleshooting

---

## 1. Getting started

### What you need

* **Your own copy of the game.** The editor reads the game's data from your disc's files, for
  example the GameCube disc's extracted `files/` folder (HWVX-SDL's README, "Game data", says how
  to get it). No game data comes with the editor, and none goes into your levels.
* **HWVX Editor** from the [releases page](https://github.com/Alaxend0l/HWVX-X-FILE-EDITOR/releases) (tags
  `editor-v*`). It includes the game, so nothing else is needed:
  * **Windows:** unzip `HWVX-Editor-VER-windows-x86_64.zip` anywhere and run `hwvx-browser.exe`.
  * **macOS:** open the `.dmg` and drag **HWVX Editor** to Applications.
  * **Linux:** make the `.AppImage` executable and run it.

  The packages also hold FFmpeg, which the Media tab uses to play the PC's and the PS2's movies
  (section 9c).

The game's launcher has an **Editor** button that starts the editor when it is installed beside
the game.

### Running it

Start the editor. The first time, it asks **where your copy of the game is**: the GameCube
disc's extracted files (the folder with `HW_share.xgc`, or the `files/` folder Dolphin's
**Extract Entire Disc** makes), a disc image (`.iso`, `.gcm`, `.gcz`), or the PC or PS2
release's data. If the game's own launcher already plays one, the editor offers it: press
**Use it**. Otherwise type the path or press **Folder…** or **Disc image…**, then **Use this**.
The editor remembers the choice (`editor-data.txt` in its scratch folder); **File > Game data…**
changes it, and it asks again if the folder is moved away.

**A disc image** (`.iso`, `.gcm`, `.gcz`, the PC CD's `.cue`/`.bin`, or a folder holding the PC
CD's `HWVX.cab`): the screen shows what the disc is and offers **Extract it…**. A GameCube
`.iso`/`.gcm` can also be used as it is (**Use it directly**); the PC and PS2 discs, and `.gcz`, must
be extracted. **File > Extract a disc…** does the same at any time. The **Extract a disc** window
shows the release, its files and size, and a folder to extract into (by default `game-data/<release>`
beside the editor's scratch folder; an empty folder, or an earlier extraction of the same disc, which
it finishes). It checks the free space first, extracts with a progress bar (**Cancel** stops after
the file in progress), then **Use this as the game data**. Your PC and PS2 discs take a few seconds.
From a terminal: `xdlrs disc extract DISC OUT` (docs/xdl-format/xdlrs.md, "The disc ripper"). The editor works with
every release's data. Where the releases differ, it says so (section 10).

From a terminal (macOS: the program is in `HWVX Editor.app/Contents/MacOS/`), `--data` uses a
folder for one run without asking:

```sh
hwvx-browser --data /path/to/your/game/files
```

To open one of the sample levels at once, and its world with it:

```sh
hwvx-browser --data /path/to/files --level level-samples/checkpoint-run --world ch1_l2
```

(The samples come with the editor: `level-samples/` beside the program, or
`HWVX Editor.app/Contents/Resources/level-samples` on macOS. Copy one before changing it.)

The browser starts the game in a hidden "viewer" mode and shows its picture in the middle of
its window. The first time you open a level in a world, the editor extracts that world once into
its scratch folder (a few seconds; "checked in ... ms" by **Live preview** shows when it's done).
After that, checks take well under a second.

### Building from source (developers)

From the HWVX-SDL checkout: `cmake -S . -B build -G Ninja && ninja -C build`, then
`cargo build --release --manifest-path tools/browser/Cargo.toml`. In a checkout the browser finds
the game, `schemas/` and `hwvx-x` by looking in the folders above itself (`--game PATH` to pick a
game); packaging is in HWVX-SDL's `packaging/README.md` ("The editor"). No Python is needed.

### The samples

`level-samples/` (in a checkout: `tools/level-samples/`) has one finished level per mode. Open them to see how a level is made,
or copy one as a starting point.

| sample | mode | world | won by | lost by |
|---|---|---|---|---|
| checkpoint-run | Challenge | ch1_l2 | three checkpoints, in order | the 40 s clock |
| gear-grab | Joyride | world1 | grabbing both gears | the 30 s clock |
| two-gates | Mission | world1l1 | gate 1, then gate 2 (objectives) | the 35 s clock |
| flag-run | Battle | bat1_l1 | reaching the flag (wins the round) | the time-out (the round goes to the AI) |
| short-sprint | Drag Race | race1l2 | reaching the moved finish first | an opponent getting there first |

### Opening a level

**File > Open level…** chooses a level's folder (the one with `level.jsonc`). Or in the left
panel, pick the **Level** tab, type the level's folder (the one with `level.jsonc`)
under **Level project (its folder)** and press **Open** (or Enter), or press **…** to choose it
with your system's folder chooser. The line below shows its name, mode and world, for example
`Checkpoint Run · challenge · ch1_l2`.

If no world is loaded yet, press **Load its world (...)**. (You can also load it from the
**Worlds** tab.) Placing things, picking in the view and the base world's lists all need the
level's own world on show.

### Starting a new level

Open **File > New level…** (or **New level…** in the Level tab):

1. **mode**: challenge, battle, drag, joyride or mission.
2. **on**: one of that mode's retail worlds.
3. **name**: your level's name.
4. **in**: the folder your level's folder goes in (**…** to choose).
5. **Make it**.

The editor writes a new folder (named after the level) with a `level.jsonc` and an empty
`triggers/` folder, and opens it. The new level is the retail level of that world with all its
own triggers kept (`base triggers: all`), so it plays like the original until you change it. A
new Battle level starts with round rules (5 kills, 3 rounds, 2 minutes).

### Joyride worlds as templates (a level of any mode on any world)

A Joyride level is an open-ended version of its
world: the whole world, its own intro, music and effects, and ten gears and a hidden key to pick
up. So it is the best stage for a level of another mode, and the only one on some worlds: retail
has no challenge on world 4. Every mode can now be made on every Joyride world.


**File > New level…** (or **New level…** in the Level tab): **on** lists the mode's retail
worlds, then, under *a Joyride world as the template*, the five Joyride worlds
(`world1 (Joyride world 1)` … `world5 (Joyride world 5)`) for every mode but Joyride. A Joyride
world picked shows:

* **Template: remove the gears and the key** (on by default): the level takes out the template's
  ten gears and its key (their pickup points), the triggers that count them and unlock cars, and
  the two objectives that show them on the intro box ("Collect 10 GEARS to UNLOCK CAR", "Find
  Hidden KEY to UNLOCK CAR"). The world is a clean stage; its intro, music and effects stay.
* a line on what the template gives the mode and what is left to you (below).

**Make it** extracts the world the first time (a few seconds, a spinner) and writes the project
with `xdlrs level new` (the same code: `xdlrs::level_template`). **Level settings** then says
"11 base pickups and 2 objectives removed (the template's gears and key)", with **keep them**
(the gears, key and objectives back, base triggers back to all).

The **Content tab**'s **+ New track**: *made from* lists the mode's retail tracks, then
"Joyride world 1 … 5 as the template" (not for Joyride). Such a track extends the mode's first
track, names the Joyride world (`world`) and starts with that world's select picture. Link a level
project made on the same world to give it a race.

#### What each mode gets on a Joyride world, and what it needs

A level of another mode on a Joyride world looks and plays like that mode: the game loads the
mode's own screens beside the world (`modeWorld`: the world pack of the mode's first retail level,
Challenge 1, Mission 1, Battle 1 or Drag Race 1, its data only, its script never run), and New
level writes the mode's flow as the level's own triggers, the way the mode's retail levels play
it. The Joyride world keeps its effects and music; its free-roam intro, its gears and key go.

| mode | the template plays (the mode's own screens) | you add | the checks warn when |
|---|---|---|---|
| Challenge | `start`: the challenge's intro box with your briefing line (`brief`), A to begin; `go`: its start (the lights, the HUD), the 60 s clock (`clock`, the HUD's); `won`: when `goal` is 1, **Objective Complete** (Continue, Restart, Save, Main Menu); `timeout`: at 0, **Objective Failed** (Retry, Main Menu) | what sets `level://variables/goal` to 1 (a volume to reach) | nothing sets `goal` |
| Mission | the same with the mission's screens (its intro box, Objective Complete, Objective Failed); no briefing of another mission (the game leaves the alias's out) | the objectives and what sets `goal` | nothing sets `goal` |
| Battle | `start`: the battle's countdown, the round clock (`clock`, `rules.roundTimer`); round rules (5 kills, 3 rounds, 2 minutes); `round-over`, `series-over`: the battle's **results** (standings: opponents disabled, rounds won; after the last round **Summary: Match Complete!** with Restart and Main Menu); `next-round`: when they're closed, the next round and its countdown | opponents: a start each (port 0, the AI's), added to the cars of `start`, `round-over`, `series-over` and `next-round` | under 2 starts; an AI start (it roams) |
| Drag race | the drag race's countdown, the same results (times, rounds won, Summary), `timeout`: **Out of Time** and the round to player 2 | 3 more starts, one per lane (**Lanes: Check the lanes**), a finish volume and a trigger per car: `round_complete` N and the mode's **Player N WINS!!** (`base://race1l2/world_0/trigger/1N/actions/1/data/p0`) | not 4 starts; an AI start |

The screens behave as on the mode's retail levels: Continue after Objective Complete goes on as a
won challenge or mission does; Retry plays the level again (its opening first); Main Menu (and the
battle's and drag race's Summary) goes back to the menus.

* **The screens are the same in every level of a mode.** `xdlrs level screens WORLD` fingerprints
  the screens a world's script plays (every record they reach, slots zeroed, their textures' and
  sounds' own bytes): across all 18 challenges, 14 missions, 8 battle arenas and 12 drag strips
  (GameCube) each role has one shape per mode; the only differences are in world 5's levels (ch5,
  world5l1-2, race5, bat5/bat8: a texture of their own where the others share one). Challenge
  and mission even share their Objective Complete and Objective Failed screens, and the Joyride
  intro box is the challenge's.
  So the first retail level of each mode serves every level of it on another world.
* **One start.** A Joyride world has the player's start only (a battle world has 7, a drag
  world 4). Battles and drag races add theirs: + Add > Start, here.
* **AI on an open world.** The world has no AI scripts of the mode (a battle world's cars attack,
  a drag world's keep to their lanes): an AI car of the level's own drives its vehicle's own AI on
  the world's roads, wherever they lead. It roams: it won't chase a flag or keep to a lane. Use AI
  cars as traffic, rivals in a battle, or leave them out and race the clock (the joyride-drag
  sample). An AI start is port 0 in the editor; the game gets port 6 (the AI's) and spawns it
  with the level.
* **The intro box.** The mode's intro shows the objectives by their HUD ids. The removed Joyride
  objectives are hidden and lose their ids, so the level's own objective with id 1 (the template's
  `brief`) is the intro's text. Keep it short (the box holds about two lines).
* **Test drive.** A level with the mode's screens is applied at run time (its level file, as a
  shared mod's is), not built into a world pack of its own.
* **Lanes and the spawn box.** Open worlds are wide; the start checks (the cars' box against the
  world, starts overlapping) and **Check the lanes** work as on any world.

---

## 2. The screen

Along the top, the **menu bar**:

| Menu | What's in it |
|---|---|
| **File** | New level…, Open level…, Save level (Ctrl+S; ⌘S on macOS), Save as mod…, Game data…, Quit |
| **Edit** | Undo (Ctrl+Z), Redo (Ctrl+Shift+Z), Delete (the selected record or trigger) |
| **View** | the left panel's tab (Packs, Worlds, Level, Content, Media), Raw nodes (in the Outline), Spin the camera |
| **Tools** | Test drive (F5), Stop the test drive, Reload world (or pack), Screenshot of the 3D view, Restart the game |
| **Help** | Editor guide (this page), About (the editor's version, the game's data, renderer and data folder) |

Below it, from left to right:

* **The left panel**, with five tabs:
  * **Packs**: the game's data packs (for browsing; not needed for levels).
  * **Worlds**: every level of every mode. Double-click one to load its world. Below the list:
    **Orbit** / **Fly** (the camera), **Start** (go to player 1's start, also **G**), **Focus**
    (the camera on the selected object, also **F**), the **Overlays** and **Unload world**.
  * **Level**: your level: open, new, the test drive controls, **Live preview**, **+ Add**, a
    **Filter**, and the lists (below).
  * **Content**: a mod of new cars, weapons and tracks, changes to the game's own cars and
    weapons, sounds and music (section 9b).
  * **Media**: the game's movies, and every sound and song in the data (section 9c).
* **Outline**: the loaded pack's contents (Geometry, Triggers, Textures...). You don't need it
  for level editing.
* **The 3D view** in the middle: the game itself drawing the world. On the Media tab, tabs above
  it switch between **3D view**, the open movie (**Movie: NAME**) and the **Audio browser**.
* **The inspector** on the right: the selected thing's fields, with **↶ Undo**, **↷ Redo**,
  **💾 Save** and **all fields** along its top. On the Content tab it shows the mod's selected
  car, weapon, track, sound or song instead.
* **The top bar**: the viewer's status, **Reconnect**, **▶ Test drive (F5)**, the loaded world,
  **Reload world**.
* **The bottom bar**: camera buttons and **Screenshot**.

### The Level tab's lists

* **Level settings**: the level's name, description, author, version, whether it starts
  unlocked, and which of the base world's triggers stay. For Battle and Drag Race also the round
  rules (section 6).
* **Your level's own things**: **Triggers**, **Volumes**, **Starts**, **Items**, **Effect points**,
  **Timers**, **Variables**, **Texts**, **Objectives**, each with how many there are.
* **The base world**: the world's own records you can change: **Starts (tile cars)**,
  **Volumes** (spheres and boxes), **Items**, **Effect points**, **Timers**, **Variables**,
  **Objectives**. Ones you changed say "(patched)".

**Filter** narrows every list. Anything with a problem is marked ⚠ in red.

### Moving around in the 3D view

* **Orbit** (the default): drag to orbit, right-drag, middle-drag or Shift-drag to pan, the
  wheel zooms. Over the picture: arrow keys orbit, **+**/**-** zoom, **Home** resets, **Space**
  spins.
* **Fly** (Worlds tab): **W A S D** to fly, **Q**/**E** down and up, **Shift** faster, the wheel
  sets the speed, drag to look around.
* **G** goes to player 1's start; **F** puts the camera on the selected object.

The world's quadrants stream in and out around the camera as in the game. The line under the
picture says how many are loaded and which one the camera is in.

### Overlays

In the Worlds tab: **regions** (each quadrant's box), **loaded** (loaded quadrants: green in
view, blue loaded, yellow the camera's), **collision** (the collision's edges), **volumes**
(fog, reverb and trigger volumes), **spawns** (starts as the spawn box, green for players,
orange for AI; pickups, teleporters, lights), **paths** (the AI's route), **objects** (placed
objects and effect points).

On top of these the editor draws your level: the selection in yellow with a red/green/blue x/y/z
gizmo, your own volumes in magenta, your own starts as cyan boxes, and starts with a problem in
orange.

### View switches: clipping, all quadrants, fog

The Worlds tab's **View** row has three checkboxes for the 3D view only (the game and test drives are unaffected):

* **Geometry clipping** off: nothing disappears with distance (no far plane, no distance or off-screen culling; far geometry stays on its last level of detail).
* **Load all quadrants**: the whole world is drawn, not just the part around the camera.
* **Fog** off: no distance fog (the sky keeps its haze).

From the command line: `hwvx-browser --world NAME --view noclip,allquads,nofog`.

### Picking

Click in the 3D view to select what's under the pointer: the base world's starts, airbox
volumes, items, effect points and objects, and your own volumes, starts, items and effect
points. The nearer one wins. Or select from the lists.

---

## 3. Editing

### Selecting

Select something in a list or click it in the view. The inspector shows its fields. With
nothing selected it says "Select something in the Level tab's lists, or click it in the view."

### Moving and turning

* **Drag an axis of the gizmo** (the x/y/z lines at the selection: x red, y green, z blue)
  to move the selection along that axis only. The axis under the pointer turns yellow; a drag
  that starts anywhere else turns the camera as usual. An axis that points straight at the
  camera can't be taken: turn the view a little. The gizmo keeps about the same size on screen
  however far away the camera is.
* **Ctrl + drag** (Cmd + drag on macOS) in the view moves the selection over the ground under
  the pointer. A volume keeps its height above the ground. Items and effect points find their
  new quadrant by themselves.
* A drag, of either kind, is one step of **Undo**, however long it lasts.
* **Ctrl/Cmd + wheel** turns a start, 15° a notch.
* **To the ground** (inspector) drops the selection straight down onto the collision. For a
  start, it seats the car on the ground (section 5).
* Or type the numbers in the inspector (`pos/x`, `center x`...).

**Base box volumes.** A box (BSP) volume of the base world is drawn as its box when selected
and moves like a sphere. The game never reads such a volume's `center`, so a level moves the
box by patching it: the patched center is how far the box moves (retail's box volumes have a
center of 0, 0, 0). The inspector's `center` shows that offset, not the box's middle. A few
retail box volumes have no closed box (14 of 717 in the GameCube worlds); they aren't listed.

### The inspector

The inspector edits what's selected by its type:

* **Numbers**: whole numbers are kept within the field's range.
* **Enums** are menus of names (a volume's type, a keyword's mode...).
* **Flags** are tick boxes; **vectors** are x / y / z.
* **Links** (a reference to another record: "which volume", "which variable") are a menu of what
  can be named there: your level's own records and triggers, and the base world's. **🎯** next to
  a link: click the target in the view instead. **Esc** cancels.
* Fields the file leaves out show greyed, with the value they get. **all fields** also shows the
  fields the game never reads (editor ids, unknown bytes).
* Problems with the selection list in red at its top.

### Changing the base world: patches

When you change one of the base world's own records (move a retail start, change the round
timer's time), the editor doesn't copy the record. It keeps only the fields you changed, as
**patches** in `level.jsonc`. The inspector says "edits are patches: only what differs from the
base is kept". **⟲** next to a field takes that patch out, so the field is back to the
original. Patched records say "(patched)" in the lists, and **Level settings** counts them
("1 base record patched").

Patches set every kind of value a base record holds: numbers, enums (a pickup's or a start's
mode, as a menu), lists of numbers (each its own box; the list is one patch), and the members
of a position or a turn (`pos/x`, an effect point's `quat/y`). Two kinds can't be patched, and
show greyed with the base world's value (the reason when you hover it):

* **Links** (what a record points to: a pickup's item kind, a start's AI scripts). Shown with
  **all fields**.
* **Numbers the game indexes with**: a volume's `shape` and `type`, a start's `dynamic`, `port`
  and `maxVehicle`, a timer's `active`, `type` and `countUp`, a variable's `active` and `id`.
  The game reads them as table positions or record kinds (a timer's three share a word with
  its on-screen binding), so it never lets a level change them.

A patch the game still refuses when the live preview applies the level (one over what a kept
base trigger reads) shows under **Live preview** and in red at the field.

### Texts

A Text is one or more paragraphs (at most 255 characters each; the inspector counts them).
**+ paragraph** adds one. Triggers show texts with `print_debug_string` (what 4: the message line
under the challenge and joyride briefing and on the win and lose screens). Keep them short, and print a
blank text as the level starts, as retail does: the briefing's own text is an objective's.

### Triggers

A trigger is "when this happens, if these hold, do these". Select one to see:

* **type**: what it waits for (`level_start`, `airbox_entered`, `timer_value_check`,
  `item_pickup`, `interface_end`...). Changing the type gives it that type's parameters.
* **conditions are**: how its conditions combine (`all`, ...).
* **when**: the type's parameters, for example `volumes` (which volume) and `cars` (whose car)
  for `airbox_entered`. Lists have **+ add** and **−**.
* **if (n)**: the conditions, numbered. Each has a kind (`variable_value_check`...) and its
  parameters. **+ condition** adds one of a kind from its menu, **−** takes one out.
* **do (n)**: the actions, in order. Each has a kind (`variable_change_value`, `end_level`,
  `round_complete`...), **after ... ms** (a delay), **every time** (it runs each time the trigger
  fires, not just once) and its parameters. **+ action** adds one.
* `player_vehicle_in_air` triggers also have **else (landing)**: actions for when the car lands.

The sample triggers are a good reference: checkpoint-run's `cp2` only counts after `cp1`
(a condition on the `progress` variable), and `cp3` ends the level won.

**Ending a level** (section 6 has more):

* Challenge, Mission, Joyride: an `end_level` action. Result 4 (arg 0) wins; any other result
  loses.
* Battle, Drag Race: a `round_complete` action ends the round for the player named.

**Base triggers.** In **Level settings**, **base triggers** says which of the world's own
triggers stay: **all**, **none**, or some (by number, in **kept (numbers)**). checkpoint-run drops
them all and has its own script; short-sprint keeps all but the retail finish line's.

### Live checks and the live preview

A quarter of a second after each edit, the editor checks the level as it is (your files aren't
written for that): links that lead nowhere, unknown fields and kinds, the game's limits on
triggers, the mode's rules. Problems list in the inspector and are marked ⚠ in the lists. Next to
**Live preview**, ✓ means it's clean; otherwise it shows how many problems there are.

With **Live preview** on, the checked level is applied to the world on show: moved and patched
base records move at once, timers take their new times, your volumes show. Triggers don't run
in the editor; their effect shows in a Test drive.

---

## 4. Adding and deleting things

### + Add

**+ Add** in the Level tab opens a menu.

Placed things go on the ground under the camera: in Fly, below the camera; in Orbit, where it
looks. They need the level's world loaded ("(load the level's world to place things)" otherwise).

| + Add | what you get |
|---|---|
| **Sphere volume, here** | a 30-unit airbox sphere a little above the ground |
| **Box volume (BSP), here** | a 40 × 25 × 40 box volume on the ground (size it in the inspector) |
| **Start, here** | a car start facing away from the camera; port 0 (an AI car). Set **port** 1–4 for a player's start |
| **Item, here** | a pickup (item 0 of the world's item pack) in the quadrant it's in (a copy of that quadrant's first pickup, or another quadrant's if it has none) |
| **Effect point, here (a copy of the selected one)** | a copy of the base effect point you selected first (its emitter, its glow) |

Not placed: **Timer** (60 s, counting down), **Variable** (0), **Text**, **Objective**, and
**Trigger** (a submenu of every trigger type; its parameters start empty, numbers 0, links
unset).

If there is no ground under the camera, the editor says "no ground under the camera". Move the
camera over the track and try again.

A volume's inspector also has its **type**'s settings: force box (force, one shot, cars only),
reverb, fog (near, far, colour), light, creature (rescue). The game finds force boxes, fog,
reverb, light and creature volumes through the world's volume grid; your volumes are added to
it when the level loads, so they work like the base world's.

### The quadrant rule (items and effect points)

The world is split into quadrants that stream in and out. Items and effect points belong to a
quadrant, which the editor finds for you as you place and move them. It must be in a quadrant:
"that place is in no quadrant: move it onto the track". Any quadrant will do: one without items
or effect points of its own gets a list of them when the level loads.

### Delete

**🗑 Delete** in the inspector takes one of your own records or triggers out. If something uses
it, the inspector says so ("used by trigger cp1"); those links then show as problems until you
fix them. A deleted trigger's file is deleted when you save. You can't delete the base world's
records; use **base triggers** to drop the world's triggers, or patch a record to move it away.

Everything is undoable, adding and deleting too.

---

## 5. Starts and the spawn box

Every start (the world's own tile cars and yours) is drawn as one box: **every car's collision
box at once**, per axis the largest of all the cars the game has (including cars a mod adds). On
the retail cars it is 10 wide, 27 long, and reaches 1.4 below the car's origin and 5.25 above
it. The inspector shows its size ("every car's box at once: 10.00 wide, 6.65 high, 27.00 long
(33 cars)"). So a start that's clear for this box is clear for any car the player picks.

**The ground rule.** A car standing on the ground reaches a little below its origin (the box's
below-origin depth, 1.4 on retail). So a start whose box goes into the ground **up to that depth
is fine**: that's a car standing there. **To the ground** puts the origin on the ground, and new
starts are placed that way.

The editor checks every start against the world and warns (orange, "the cars' box here: ...") when:

* its box is deeper in the ground than that ("its box 2.10 into the ground (deeper than the
  cars' 1.40 below their origin)"),
* it floats more than 3 units above the ground,
* it's over a ledge or slope, or there's no ground under it,
* a wall crosses the box, or a ceiling or overhang is inside it,
* its box overlaps another start's (the check uses each start's turned footprint and height).

These are warnings: the level still builds and runs. Fix them with **To the ground**, by moving
or turning the start, or by moving it apart from the others.

Every level needs a **player start** (port 1); without one it's an error.

---

## 6. Each mode

The mode is fixed when the level is made (it's in **Level settings**, read only). Each mode has
its own checks; they're warnings, and the game still runs the level.

### Challenge

A course against the clock. End it with `end_level` (result 4 wins). checkpoint-run is the
example: sphere volumes as checkpoints, a `progress` variable so they count in order, the round
timer patched to 40 s, `timeout` losing when the clock reaches 0.

### Mission

An objective chain. Add **Objectives** (a text, a "done" variable, and optionally a counter or
timer shown on the HUD), and triggers that set each objective's done variable and finally
`end_level` (result 4: won, 0: lost). The world's retail AI stays as its level has it. Example:
two-gates.

### Battle

Arena rounds. In **Level settings → Round rules**, tick **the level sets them (else the options'
own)**, then:

* **win by**: kills, points, or none (only your triggers end the round),
* **kills** / **points** to win a round,
* **rounds**,
* **time**: a round's time in ms (0: no limit),
* **jettison**: wildcards are dropped on a reset.

A `round_complete` action ends a round for the player it names. Battle needs at least 2 starts.

**Items in a battle.** Since the game's 0.4.2, Battle Options starts with Gadgets on **Random**:
every item point then rolls a weapon from its range (the retail points' 0-11, the item
select's weapons) instead of the item you set, and only among the weapons the player left on
under Battle Options > **Weapons**. Your items show as placed when the player picks Gadgets
**On**. In the Level tab, **Random items** (Battle levels only) test-drives it the players' way;
off, your items as placed.
Example: flag-run (1 round; the flag volume gives it to player 1, the time-out to player 2).

### Drag Race

Four lanes: exactly **4 starts**, one per lane. **Round rules** as in Battle. Under **Lanes**,
**Check the lanes** (with the level's world loaded) sweeps each start's box straight ahead for
400 units: "✓ every lane clear for 400", or "lane 2 blocked after 130" when something (a gantry
leg, a wall) is in the way. It also warns when there aren't 4 starts. Example: short-sprint (the
finish moved up the road, one `round_complete` trigger per car, a handicap holding the AI back
2.5 s).

### Joyride

Free roam, no AI (retail Joyride has none; adding an AI start is warned). Collectibles are items
with an `item_pickup` trigger: count them in a variable, and `end_level` result 4 wins. Keys that
unlock cars use `code_variable_modify_value` 100 (arg: the car). Example: gear-grab.

### AI in every mode

AI cars are used the way retail uses them in that mode and world. The checks warn when you put
an AI car in Joyride, in a world whose retail level has none, or one of your own AI starts off
the retail AI routes.

---

## 7. Test drive

Press **F5** (or **▶ Test drive (F5)** in the top bar or the Level tab). The editor builds your
level as it is now, saved or not, and starts the game straight into it in a window of its own.
Your project's files aren't written.

Options, above the button:

* **Car**: the car to drive, by number (0, 1, 2...).
* **Build world pack** (off by default): build a world pack of the level's own from your data and drive that, the older route. Off, the test drive runs your level exactly as players who install it will get it: the level file applied to the world as it loads.
* **Quick start**: skip the intro screen and the start's delays: the camera, countdown, control
  and the clock all at once, so you're driving immediately.
* **From here**: start player 1 on the ground under the 3D view's camera (in Fly: under it; in
  Orbit: where it looks), facing the camera's way. Good for trying one part of a long level.

**Timings.** The line under the button says how long the build took ("compiled in 55 ms (unchanged)"), which route it used ("as shared: the level file applied at load", or "its own world pack, built") and the game's PID. Then it says when the level was applied, the race began and you could drive ("level applied 444 ms · in the race 530 ms · driving 561 ms"). As shared, a test drive starts in about half a second. With **Build world pack** ticked, the first test drive of a world takes a few seconds longer (the world is extracted and its pack built once); after an edit about 2 s; unchanged, under a second.

**Errors** stop the test drive and list under the button by file and place, for example
`triggers/cp1.jsonc param: level://volumes/cp9: the level has nothing of that name`. Fix them and
press F5 again.

**Back to the editor.** Quit the race from the pause menu (Quit), or choose Main menu / Continue
at its end, or close the game's window: the test drive ends and the editor is back. While it
runs, the button reads **↻ Test drive again (F5)** and restarts it; **■ Stop** ends it. The
editor only ever stops the game it started.

### Is my level beatable?

**You decide that, in Test drive.** Drive it the way a player would: is the clock fair, can every
checkpoint be reached, do the objectives chain? The editor's own checks are structural (links
lead somewhere, triggers can fire, objectives chain, starts are clear, limits are kept); they
can't tell if a level is fun or possible.

The game also has an autopilot (`HWVX_AUTOPILOT=1`, a test switch; docs/testing/autopilot.md)
that lets the game's AI drive player 1. It is **only a smoke test**: it shows that the level
loads, starts and doesn't break in the parts the AI reaches. The game's AI isn't good enough to
judge beatability: a pass shows one route worked for the AI, and a fail doesn't mean a person
can't win.

---

## 8. Saving, undo and redo

* **Save**: **Ctrl/Cmd+S** or **💾 Save** (it reads **Save\*** when there are unsaved changes).
  Only the changed files are written, edited in place: your comments, blank lines, key order and
  formatting stay. A note under the buttons says what was saved.
* **Undo**: **Ctrl/Cmd+Z** or **↶ Undo**. **Redo**: **Ctrl/Cmd+Shift+Z** or **↷ Redo**. A whole
  drag or a burst of typing is one step. Adding, deleting and changing a kind are undoable.
* Test drive and the live checks use your edits as they are; you don't have to save first.

The level is plain text, so you can also edit `level.jsonc` and the trigger files in a text
editor and open the folder again. From the command line,
`hwvx-browser --level LEVEL_FOLDER --check --data /path/to/files` runs the same checks without a
window (`--drive SECS` test-drives it, `--save-mod OUT` packs it).

---

## 9. Sharing your level (packaging)

A level is shared as an installable mod, a `.xmod` file. It holds your level only: its compiled
level file and your project's sources. It never holds game data, and saving refuses if it would.

1. In the Level tab, open the level and save it (⌘S / Ctrl+S).
2. Click **Save as mod…** and choose where the `.xmod` goes. The first time takes about 30 s
   while it checks your copy of the game, so that nothing from the game ends up in the mod; that
   check is cached afterwards.
3. Give the `.xmod` file to the other player.
4. The player opens the launcher's **Mods** page, clicks **Install a mod...** and picks the file.
   It is listed with its name and version.
5. The level appears at the end of its mode's world select. Pick it and play.

Nothing is built on the player's machine. When the level's world loads, the game applies the
level to the player's own copy of that world: it changes values, swaps triggers, and adds starts,
items, effect points and objectives in memory. All of it is undone when the world unloads.

A test drive (F5) already runs your level exactly as players will get it: the level file applied as the world loads. Ticking **Build world pack** in the Level tab tests through a world pack built from your data instead. The result is the same in most cases; the default is what you share.

---

## 9b. The Content tab (new cars, weapons, tracks, sounds and music)

The **Content** tab (or **View > Content**) makes a **mod**: new cars, weapons and tracks built
from the game's own, changes to the game's own cars and weapons, and sounds and music of your
own. Everything in it builds on the player's copy of the game. A mod holds no game data: only
your numbers, your own files (sounds, music, pictures) and your levels.

### Opening a mod

* **Open** a mod's folder (the one with `mod.jsonc`), or **New mod…**: a name, an author and the
  folder it goes in. The `night-shift` sample (`tools/mod-samples/night-shift`) is one to look at.
* The left panel then lists the mod's parts, in this order: **Cars**, **Weapons**, **Retail
  changes**, **Sounds**, **Soundtrack** and **Tracks**. Select one to edit it in the inspector.
* **💾 Save** (Ctrl+S on this tab) writes `mod.jsonc`, keeping its comments. **● unsaved** by the
  mod's name means there are changes to save.

### New cars and weapons

* **+ New car** / **+ New weapon**: pick what it's made from (cars by name; weapons as the item
  select numbers them, 0-11) and a name. It gets an id of its own (`uid://…`); the game gives it
  the next free number when the mod is on (the first new car is 33, the first weapon 12).
* **The inspector**: the name, what it's made from, **unlocked** (from the start), and for a car
  its **Paint** (a hue shift and brightness on one of its textures, applied to the player's own
  texture as it loads). Below, the **numbers** by part (a car's engine and gears, handling,
  acceleration and brakes, stunts; a weapon's weapon and pickup values): tick a field to set it,
  untick it to go back to the base's value. **🗑 Delete** takes the entry out.
* A new car's model, sounds and scripts are its base car's. A new weapon is a copy of its own:
  changing it leaves the weapon it was made from as it was.

**A new car's sounds.** The car inspector's **Sounds** (under Paint) lists the sounds of the
car's own pack, by what plays them: **wind (loop)**, **engine (loop, pitched by rpm)**, **rolling
(loop, on a surface)**, **gear shift**, or **not played** (a sound set the game never reads).
Each row has **▶** (the game's sound), its length, rate and **↻** if it loops. **Replace…** picks
a WAV, FLAC, Ogg Vorbis or MP3 file and copies it into the mod's `sounds/` folder; the row then
plays your file as the game will decode it, and **✕** drops it. A replacement here plays in this
car only, never in the car it is made from. Horns, crashes, skids and pickups aren't in a car's
pack: they are the shared sounds (**Sounds** below).

### Retail changes

A balance or repaint mod: the game's own cars and weapons, changed wherever the game uses them,
with no new entries. **Retail changes** lists the 33 retail cars by name and the 12 item-select
weapons by number. A changed one is marked `● … (N changed)`, and the header counts them.

Select one: the inspector is the same as a new car's or weapon's (paint for a car, the numbers
by part, tick to change). **↺ Reset** drops every change the mod makes to it.

* A change to a retail weapon doesn't reach a new weapon made from it, and the other way round.
* A few car and weapon textures have other numbers on the PC or the PS2 (or aren't there). For
  those, the paint is written for each release under its own number, and the inspector says how
  the other releases number the texture, or that one hasn't it.

### Sounds

**Sounds** replaces any other sound of the game (the shared horns, crashes and skids, a
weapon's, a world's...) for every car and level.

* **+ Replace a sound**: pick an archive, a pack with sounds and one of its sounds (**▶** to hear
  it, its name to pick it). It opens in the inspector.
* **Replace…** picks your file (WAV, FLAC, Ogg Vorbis or MP3) and copies it into `sounds/`.
  Every pack that loads the sound plays your file instead. The game mixes it to mono and plays it
  at the sound's rate.
* **▶ Drive with it**: a Challenge race with the mod on (in that car for a car's sound, else in
  the mod's first new car, else car 0).
* Files in `sounds/` that nothing uses any more are listed, with **remove**. Remove them, or they
  go into the package.

### Soundtrack

**Soundtrack** replaces any of the game's 60 songs with music of your own. One file loops; several
make a playlist.

* **+ Replace a song** lists every song, grouped: Menus, Adventure, Challenge, Joyride, Drag race,
  Battle and Multi-player drag (a filter field narrows it; **✔** marks the ones the mod has).
  Pick one, then **Choose files…** (OGG, MP3, FLAC or WAV, several at once). The files are copied
  into the mod's `music/` folder. You can also drop files on the window, type a path, or pick a
  file already in the mod (**+ A file of the mod**).
* **The inspector** shows the song's tracks in order: **⏶** **⏷** to reorder, **✖** to take one
  off (the file stays in the folder). Each track shows its rate, channels and length, and has:
  * **▶ Play**: the file as the game decodes it. **▶ Loop seam**: 3 s before the loop's end,
    then 3 s from its start, twice, as the game loops it. **■ Stop**.
  * **loops**: unticked, the track plays once, then silence.
  * **loop start** / **loop end**: ticked, your own (in the file's frames); unticked, the file's
    own loop tags (`LOOPSTART`, `LOOPEND`, `LOOPLENGTH`), else a `.loop.txt` beside it, else the
    whole file. In a playlist each track plays to its **ends at**, then the next one follows.
  * **intro**: a file of the mod played once before the track. **volume** 0-4 (1 is right for
    music mastered at about -17 dB RMS).
* **▶ Test in game** saves the mod and starts the game where the song plays: a level's song in
  its race (the intro box is passed for you), the title screen's song (`music_1`) at the title,
  the menus' song (`music_2`) in the main menu.
* **▶ Base song** plays the game's own song, to compare.
* **🗑 Keep the game's song**: the mod no longer replaces it (its files stay in `music/`).
* Problems (a missing file, a loop that ends before it starts) list under the tracks. A file over
  ten minutes is flagged: the game plays it, but **Save as mod** can't check it.

### Tracks

A mod can bring new tracks: new entries in a mode's world select, each a race of its own. One mod
can hold several, a small championship.

* **+ New track**: the mode (Adventure, Challenge, Joyride, Battle, Drag race), the retail track
  it is made from (by its world select name and world, for example `40  Challenge 2 (ch1_l3)`),
  a name, and **locked until the game unlocks it** or not. The game puts it after the mode's own
  tracks in its world select.
* **The inspector**: the name, what it's made from (its loading screen, win and lose sequences,
  and its world), **unlocked**.
* **Level**: none (the base track's own race), or a level project of yours (the Level tab's
  folders): **Link a level project…**. **Compile** compiles it into the mod folder (**▶ Drive
  it** and **Save as mod** do that by themselves); **Open in Level tab** edits it; **Unlink**
  goes back to the base track's race. The link to your project's folder stays on your machine:
  a shared mod never carries your folders.
* **Picture in the world select**: **the game's** (a click picks one of the select's own
  pictures, each named after the tracks that use it) or **your own**: **Choose image…** takes a
  PNG of at most 1024 x 1024 and 4 MB and copies it into the mod's `images/`. The game draws it
  at the base picture's size (256 x 256, shown stretched to about 5:3: a 5:3 PNG looks as
  drawn). A new track starts with its base track's picture.
* **▶ Drive it**: saves, compiles the track's level and starts its race.

### Drive it, Try it

Every part has a button that saves the mod and starts the game with it on, at the place to
check it. **■ Stop** in the top bar ends it.

| what | button | the game starts |
|---|---|---|
| a new or retail car | **▶ Drive it** (or **F5**) | a Challenge race in that car |
| a new or retail weapon | **▶ Try it** (or **F5**) | a Battle with that weapon |
| a sound | **▶ Drive with it** | a Challenge race in a car that plays it |
| a song | **▶ Test in game** | where the song plays |
| a track | **▶ Drive it** | the track's race |

The game's log is `testdrive.log` in the editor's scratch folder.

### Save as mod

**Save as mod…** packs the mod into an `.xmod`, as for levels (section 9): it compiles the tracks'
levels first, then checks that nothing in it is the game's (its sounds, music and pictures
included: your own files pass, a copy of the game's is refused). Mods made here install like any
other (the game's launcher, Mods page).

### Custom worlds (experimental)

A world of your own, made from your own model (glTF) and driven over a retail level's rules, is
being built. It is not in the editor yet and can't go into a mod yet: today it is a command-line
test. To try one with the programs in the editor's package (or a checkout):

```sh
hwvx-x world demo my-world        # a ground, a ramp and a wall: my-world/world.xgc and world.jsonc
HWVX_WORLD=my-world/world.jsonc HWVX_START=race:mode=joyride,world=uid://hwvx-world-demo \
  hwvx --data /path/to/your/game/files
```

`hwvx-x world demo my-world --gltf` also writes the demo as `demo.gltf` with its project file
`demo.jsonc`: change them, or point one at your own model, and `hwvx-x world build
my-world/demo.jsonc my-world` builds it (start it with `world=` the `"id"` in that file,
`uid://hwvx-world-sample-ramp` for the demo's). Nothing of the base level shows (its objects, movers, AI
cars, lights, weather); its rules, music and fog stay. The plan, and what works so far, is
docs/plans/custom-worlds.md.

---

## 9c. The Media tab (movies and the Audio browser)

The **Media** tab (or **View > Media**) plays the game's movies, and every sound and song in the
data. It has two parts: **Movies** and **Audio**.

### Movies

The list shows the data's movies, from any release:

| release | movies | played by |
|---|---|---|
| GameCube | THP (`fmv_/bglogo`, `hw_intro`...) | the editor itself |
| PC | Bink (`FMV_/*.bik`) | FFmpeg |
| PS2 (retail and prototype) | PSS (`*.PSS`) | FFmpeg for the picture; the editor for the sound |

Each row shows the format, picture size, length and sound. Click a movie to open it in the 3D
view's place (the **Movie: NAME** tab; **3D view** switches back).

* **▶ Play** / **⏸ Pause**, **■ Stop** (back to the start), **⏮ Restart**, and the scrubber: you
  can seek to any frame.
* The sound plays through the system's player, from wherever the picture is.
* **Export audio WAV** saves the soundtrack.
* **Open in your video player** hands the file to your system's video player. Use it for a Bink
  or PSS movie when FFmpeg isn't found, and for any other video.
* **Open a video…** picks any file: the game's kinds play in the editor, others go to your video
  player. **Rescan** reads the list again; it also lists the video files in an open mod's folder
  (as "mod: ...").

FFmpeg (versions 6 to 9) comes with the editor's packages. The editor looks for it in
`HWVX_FFMPEG_DIR` if that is set (and only there), else beside the program, in the package, and
in the system's libraries. Without it the PC's and the PS2's movies say **FFmpeg not found**
(section 11).

### The Audio browser

The **Audio** side counts everything playable in the data. **Open the Audio browser** shows the
list in the 3D view's place (the **Audio browser** tab). It holds:

* **sound**: every pack's sound effects, in every archive (cars, weapons, the shared sounds, the
  menus', the worlds', the briefings');
* **instrument**: the samples of the songs' instrument banks;
* **song**: every MIDI song (the menus' and the worlds');
* **movie**: every movie's soundtrack;
* **mod file**: an open mod's own audio files (its sounds and soundtrack).

Search by archive, pack and name (for example `hw_cars car_5`, `sound/3`, `music`), tick the
kinds to show, or pick one archive. Each row has **▶** / **■**, its length, rate and loop start,
and **Open in pack** (loads the pack and opens the item in its own view). A sound's or sample's
▶ plays it through the system's player. A song's ▶ plays it in the game (its sequencer), not
through the system's player. A movie's ▶ opens the movie.

Select a row for its waveform with the loop marker. **Export WAV** saves a sample; for a song,
**Export MIDI** saves it as a standard MIDI file.

From the command line, `--movie NAME[@SECONDS][!]` opens a movie (paused at SECONDS; `!` plays
it), and `--audio SEARCH` opens the Audio browser with that search.

---

## 10. Limits and known issues

* **The intro screen's text** is the base level's; your level's text shows as the objective
  line (pause, win and lose screens).
* **The live preview** shows your volumes, timers, variables, texts and patches; your own starts,
  items, effect points and objectives are drawn as marks but only take effect in a test drive (and in the shared level).
* **Base volumes**: box volumes without a closed box (a few) aren't listed; a box volume
  another one shares its shape with can't be moved (the test drive's build says so; none in the
  GameCube data).
* **Shared levels: light and creature volumes of your own** work through the volume grid (a
  thing inside one is lit, a car inside a rescue zone counts), but a light volume's world-wide
  part (directional and ambient lights) and creature spawning read the world's own volume list,
  which only the test drive's built world has your volumes in.
* **Patches** don't change links (which item a pickup gives, which model an instance shows) or
  the numbers the game indexes with (a volume's type, a start's port, a timer's count-up): see
  section 3. Make a record of your own instead where you can (a volume, an item, a start).
* **The gizmo while the camera spins** (Space) picks from the camera of up to a second ago;
  stop the spin to drag an axis.
* **Trigger limits**: the game allows only so many active triggers of each type (your level's
  plus the base ones kept). Going over is a check error; drop base triggers you don't need.
* **macOS: the game stalls while its window is covered.** With the Metal renderer, a test drive
  window hidden behind another window stops drawing. The editor brings it to the front; if it
  still stalls, start the browser with `--testdrive-renderer opengl`.

**The Content tab:**

* **No undo yet.** Untick a number to go back to the base's value, or **↺ Reset** a retail car
  or weapon. (Ctrl/Cmd+Z does nothing there.)
* **F5** on the Content tab drives the selected car or weapon. For a track, a sound or a song use
  its own button.
* **A car's own model** isn't possible yet: a new car is a base car's, repainted. Weapons are
  picked by number, not name. New pickups aren't possible yet
  (docs/plans/pickups-and-briefings.md).
* **Paint** shifts the hue and brightness of a texture the player already has: a mod can't bring
  a car texture of its own. In the editor, only a track's world select picture can be your own
  (a PNG).
* **Music files over ten minutes** play in the game, but **Save as mod** can't check them.
* **Custom worlds** are experimental: a command-line test only, not in the editor or in mods
  yet, a ground of one surface, textures from the base world only (section 9b).

**The releases (GameCube, PC, PS2):**

* The PC has no multi-player drag worlds, so six of the 60 songs aren't on the PC. The
  Soundtrack list shows them as "not on this release (PC)" and doesn't offer them; a mod's song
  for one is kept for the other releases, but can't be tested on the PC. A track over one of
  those worlds is left out of the PC's world select.
* A few car and weapon textures have other numbers on the PC or the PS2 (section 9b, Retail
  changes); the editor writes the paint for each release.
* A custom world over a world 3 or world 5 base shows some other textures on the PC (its texture
  packs are in another order there).
* PC and PS2 movies need FFmpeg (it comes with the editor's packages; section 9c).

**The Media tab:**

* A song (MIDI) plays in the game behind the 3D view, through its sequencer, not through the
  system's player: it can't be heard while the game isn't running, and **Export MIDI** is the way
  to hear it elsewhere.
* Sounds, samples and movie sound play through the system's player (`aplay`, `afplay` or
  PowerShell).

---

## 11. Troubleshooting

**"The viewer could not be started"** (instead of the editor). The browser couldn't start or
reach the game. Check:

* the game is built (`build/hwvx` in the same checkout as the browser), or pass `--game PATH`;
* you run the browser from that checkout (it finds the game and the tools from there; another
  checkout's browser uses another build), or pass `--tools DIR` for the tools folder;
* the game data is right: **Choose the game data…** (or **File > Game data…**). When the game
  finds no data in the folder chosen, the editor goes back to **Where is your copy of the
  game?** by itself.

The message under the heading says what went wrong; **Try again** retries.

**A level opened over another world.** Opening a level while a different world is loaded is
safe: the level is never applied to the wrong world (the game refuses it), and the base world's
lists stay empty with "load the level's world (Worlds tab) to list and pick its records". Load
the level's own world.

**+ Add does nothing / "load the level's world to place things in it".** Load the level's world
(**Load its world** or the Worlds tab).

**"no ground under the camera"**: move the camera over the track (in Orbit, look at the road).

**"select a base effect point first"**: an effect point is a copy of an existing one. Select one
of the world's effect points (list or click), then **+ Add → Effect point, here**.

**Test drive "N errors: not started"**: the errors list under the button by file and place.

**The game window doesn't appear or doesn't move (macOS)**: see the covered-window note in
Limits (`--testdrive-renderer opengl`).

**A PC or PS2 movie says "FFmpeg not found"**: the editor found no FFmpeg (6 to 9:
libavformat, libavcodec, libavutil). The packages bring it; a build from a checkout uses the
system's (install FFmpeg with your package manager), or set `HWVX_FFMPEG_DIR` to the folder that
holds its libraries (only that folder is then looked in). Meanwhile **Open in your video player**
plays the file.

**Logs**: the test drive's game log is `testdrive.log` in the editor's scratch folder
(`viewer-pref/testdrive/` next to the browser program, or under `--pref`); the viewer's is
`viewer-pref/viewer.log`. Lines starting `level:` say whether the level was applied (`level: not applied:
...` says why); `testdrive:` lines give the timings.

### Command line

```
hwvx-browser [--data DIR] [--game PATH] [--tools DIR] [--pref DIR]
             [--level FOLDER] [--world NAME] [--car N] [--quick-start] [--test-drive]
             [--testdrive-renderer opengl]
```

`--level` opens a level, `--world` loads a world, `--test-drive` starts a test drive as soon as
the browser is up (with `--car` and `--quick-start`).
