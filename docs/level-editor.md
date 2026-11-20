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
9b. New cars and weapons (the Content tab)
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

---

## 2. The screen

Along the top, the **menu bar**:

| Menu | What's in it |
|---|---|
| **File** | New level…, Open level…, Save level (Ctrl+S; ⌘S on macOS), Save as mod…, Game data…, Quit |
| **Edit** | Undo (Ctrl+Z), Redo (Ctrl+Shift+Z), Delete (the selected record or trigger) |
| **View** | the left panel's tab (Packs, Worlds, Level), Raw nodes (in the Outline), Spin the camera |
| **Tools** | Test drive (F5), Stop the test drive, Reload world (or pack), Screenshot of the 3D view, Restart the game |
| **Help** | Editor guide (this page), About (the editor's version, the game's data, renderer and data folder) |

Below it, from left to right:

* **The left panel**, with three tabs:
  * **Packs**: the game's data packs (for browsing; not needed for levels).
  * **Worlds**: every level of every mode. Double-click one to load its world. Below the list:
    **Orbit** / **Fly** (the camera), **Start** (go to player 1's start, also **G**), **Focus**
    (the camera on the selected object, also **F**), the **Overlays** and **Unload world**.
  * **Level**: your level: open, new, the test drive controls, **Live preview**, **+ Add**, a
    **Filter**, and the lists (below).
* **Outline**: the loaded pack's contents (Geometry, Triggers, Textures...). You don't need it
  for level editing.
* **The 3D view** in the middle: the game itself drawing the world.
* **The inspector** on the right: the selected thing's fields, with **↶ Undo**, **↷ Redo**,
  **💾 Save** and **all fields** along its top.
* **The top bar**: the viewer's status, **Reconnect**, **▶ Test drive (F5)**, the loaded world,
  **Reload world**.
* **The bottom bar**: camera buttons and **Screenshot**.

### The Level tab's lists

* **Level settings**: the level's name, description, author, version, whether it starts
  unlocked, and which of the base world's triggers stay. For Battle and Drag Race also the round
  rules (section 6).
* **Your level's own things**: **Triggers**, **Volumes**, **Starts**, **Items**, **Effect points**,
  **Timers**, **Variables**, **Texts**, **Objectives**, each with how many there are.
* **The base world**: the world's own records you can change: **Starts (tile cars)**, **Airbox
  volumes** (the sphere ones), **Items**, **Effect points**, **Timers**, **Variables**,
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
| **Item, here** | a pickup (item 0 of the world's item pack) in the quadrant it's in |
| **Effect point, here (a copy of the selected one)** | a copy of the base effect point you selected first (its emitter, its glow) |

Not placed: **Timer** (60 s, counting down), **Variable** (0), **Text**, **Objective**, and
**Trigger** (a submenu of every trigger type; its parameters start empty, numbers 0, links
unset).

If there is no ground under the camera, the editor says "no ground under the camera". Move the
camera over the track and try again.

A volume's inspector also has its **type**'s settings: force box (force, one shot, cars only),
reverb, fog (near, far, colour), light, creature (rescue).

### The quadrant rule (items and effect points)

The world is split into quadrants that stream in and out. Items and effect points belong to a
quadrant, which the editor finds for you as you place and move them. Two rules:

* It must be in a quadrant: "that place is in no quadrant: move it onto the track".
* That quadrant must already have some items (for an item) or effect points (for an effect
  point). A quadrant with none is refused when the level is built (the error names it). Turn on
  the **objects** and **spawns** overlays to see where the world's own are.

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

## 9b. New cars and weapons (the Content tab)

The **Content** tab (or **View > Content**) makes a **mod** of new cars and weapons built from the
game's own: each one takes a base car or weapon (its model, sounds and scripts come from the
player's copy) and changes its numbers and paint. A mod holds no game data.

* **Open** a mod's folder (the one with `mod.jsonc`), or **New mod…**: a name, an author and the
  folder it goes in. The `night-shift` sample (`tools/mod-samples/night-shift`) is one to look at.
* **+ New car** / **+ New weapon**: pick what it's made from (cars by name; weapons as the item
  select numbers them, 0-11) and a name. It gets an id of its own (`uid://…`); the game gives it
  the next free number when the mod is on (the first new car is 33, the first weapon 12).
* **The inspector** on the right: the name, what it's made from, **unlocked** (from the start),
  and for a car its **Paint** (a hue shift and brightness on one of its textures, applied to the
  player's own texture as it loads). Below, the **numbers** by part (a car's engine and gears,
  handling, acceleration and brakes, stunts; a weapon's weapon and pickup values): tick a field
  to set it for this car, untick it to go back to the base's value.
* **▶ Drive it** (cars) / **▶ Try it** (weapons), or **F5** on this tab: saves, then starts the
  game with the mod on, in a Challenge race in the car or a Battle with the weapon. **■ Stop** in
  the top bar ends it.
* **💾 Save** (Ctrl+S on this tab) writes `mod.jsonc`, keeping its comments. **Save as mod…**
  packs it into an `.xmod` after the check for game data, as for levels (section 9).

Mods made here install like any other (the game's launcher, Mods tab). New weapons are copies of
their own: changing one leaves the weapon it was made from as it was.

Not yet: a car's own model (only a base car's, repainted), weapon names in the picker (numbers),
new pickups (docs/plans/pickups-and-briefings.md).

---

## 10. Limits and known issues

* **New volumes of types other than airbox** (force, fog, reverb, light, creature) aren't yet in
  the world's volume grid, so only airbox triggers find them. A warning says so.
* **Items and effect points** can only go into quadrants that already have some (section 4).
* **The intro screen's text** is the base level's; your level's text shows as the objective
  line (pause, win and lose screens).
* **The live preview** shows your volumes, timers, variables, texts and patches; your own starts,
  items, effect points and objectives are drawn as marks but only take effect in a test drive (and in the shared level).
* **Base volumes**: only the sphere airboxes are listed and movable (box volumes aren't).
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
