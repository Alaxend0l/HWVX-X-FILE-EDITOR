# HWVX Editor: UX ideas from other tools

This note looks at how established game data viewers and level editors handle
first-time users, and turns that into a prioritised list for HWVX Editor
(hwvx-browser). The editor is an egui app whose 3D view is rendered by the game
itself in viewer mode, so ideas that need a custom renderer go under "Later".
The quick wins assume one day of work and are aimed at one person: a modder
opening the tool for the first time, who needs to know what they are looking
at, what to do next, and whether their level is ready to share. Each claim
about another tool links to the page it came from; anything from memory is
marked "unverified".

Done on 2026-11-20, not repeated below: the first-start screen asking for the
game data or disc image (with the game launcher's choice offered), and the
File/Edit/View/Tools/Help menu bar.

**Checked against the editor (2026-11-20):** items 4, 7 and 10 below already
exist (F focuses the selection and Home resets the camera; the fly camera has
wheel speed, Shift and Q/E; the test drive has a "from here" start), so they
are kept only as references. Item 2 is half there: checks already re-run a
quarter second after each edit and their count shows next to Live preview;
what is missing is clicking a problem to select it and move the camera to it.

## Quick wins for the one-day push

Ordered by value to a first-time modder. Effort is rough and assumes the
existing panels and game-control plumbing.

1. **Start page with recent projects and three clear actions (1-2 h).**
   When nothing is loaded, the central area shows "Open level project",
   "New level project", "Browse a world" and a list of the last 5-10 level
   folders (each with name, path, last opened). The same list goes under
   File > Open Recent. Every empty panel gets one line that says what to do
   ("No pack loaded - pick one in Packs on the left"), instead of being blank.
   Why: TrenchBroom opens on a welcome window with recent maps plus
   "New map..." and "Browse..."; Dolphin's empty game list says
   "Double-click here to set a games directory" so the empty state itself is
   the instruction.
   (TrenchBroom manual; Dolphin bug tracker #10406, #9500.)

2. **Clickable errors list that selects and frames the problem (2 h).**
   Clicking a row in the checks list selects the volume/start/item/trigger in
   the level lists and inspector, and moves the camera to it (see item 4).
   Re-run checks on every edit (or after save) so the list stays current, show
   the error count in the status bar, and add Tools > Check level with a
   shortcut. Right-click on a row: "Go to", "Copy message".
   Why: TrenchBroom's Issue Browser is a live list where selecting an issue
   selects the offending objects, with Fix/Hide on right-click; Hammer's
   Check for Problems (Map menu, Alt+P) is non-modal and has a "Go to" button.
   (TrenchBroom manual; Valve Developer Community Hammer Map Menu and
   Check For Problems pages; TWHL tutorial.)

3. **"Ready to share" indicator in front of Save as mod (1-2 h).**
   A small badge in the toolbar/status bar with three states: red = checks
   report errors; amber = no errors, but not test-driven since the last
   change; green = checks pass and the level was test-driven (F5) after the
   last edit. Save as mod stays allowed in every state but shows the state and
   the reason in its dialog. Clicking the badge opens the errors list.
   Why: Trackmania/ManiaPlanet's editor has exactly this red/orange/green
   validation flag; orange means "needs to be tested at least once", and any
   change resets validation. Clicking the red flag lists what is missing.
   (ManiaPlanet "How to use the map editor".)

4. **(Already in the editor: F, Home.) Frame selected / frame all (2-3 h).**
   F (and Numpad .) moves the game camera to look at the selected object;
   Home (or Shift+F) frames the whole level. Also a "Frame" button in the
   inspector and in the right-click menu of outline/level lists. Keep the
   camera rotation, only move it, so the user is not disoriented.
   Why: Blender's View > Frame Selected (NumpadPeriod) and Frame All;
   TrenchBroom's "Automatic Navigation" centres the camera on the selection
   without rotating it. This is the single most used "where is this thing"
   action in every 3D editor. Needs one new viewer-mode camera command if the
   orbit buttons cannot already target a point.
   (Blender manual, Navigation; TrenchBroom manual.)

5. **Status bar that answers "what am I looking at" (1-2 h).**
   One line along the bottom: loaded pack or world name, selected item type
   and name (e.g. "Trigger 3 - checkpoint"), camera position and fly speed,
   game state (running / compiling / test drive), unsaved marker, error count.
   Also put a dot or asterisk in the window title when the level is unsaved.
   Why: TrenchBroom's main window has an info bar under the viewports that is
   context-sensitive; Ultimate Doom Builder users rely on the dirty marker in
   the title bar (forum report, otherwise unverified).
   (TrenchBroom manual; zdoom forum thread on UDB test map.)

6. **Controls cheat sheet on F1 and in Help (1 h).**
   Help > Controls opens a small window with the camera keys, editing keys and
   F5/Ctrl+S/undo, generated from the same table the input code uses so it
   cannot drift. Show the shortcut next to every menu item (egui
   `Button::shortcut_text`) and add hover tooltips to the bottom-bar icon
   buttons.
   Why: noclip.website keeps a full Controls table (WASD, Shift faster,
   scroll wheel for speed, B to reset camera, Z to hide UI); ManiaPlanet's
   editor has a quick help button and H for a helper dialog; TrenchBroom
   documents and lets users rebind every shortcut.
   (noclip.website README; ManiaPlanet editor doc; TrenchBroom manual.)

7. **(Already in the editor.) Free-fly camera speed and reset, noclip style (1 h).**
   In Worlds/level fly mode: scroll wheel changes fly speed (shown in the
   status bar), Shift = fast, a slow modifier, Q/E down/up, and a "reset
   camera" key that returns to the level start. Levels in a racing game are
   large; a single fixed speed is the first thing new users fight with.
   Why: noclip.website's WASD mode uses exactly these bindings (scroll wheel
   adjusts movement speed, Shift faster, backslash slower, B reset).
   (noclip.website README.)

8. **Right-click context menus on tree and list rows (1-2 h).**
   Outline nodes: "Frame in view", "Copy name/path", "Export..." (where a
   viewer already exists: texture to PNG, sound to WAV), "Show raw node".
   Level list rows: "Frame", "Duplicate", "Delete". This makes actions
   discoverable without reading docs.
   Why: GCFT puts extract/replace/delete on right-click of files inside a
   GCM/RARC and "Open Image" on right-click of a texture; BrawlCrate plugins
   are driven from right-click on tree nodes; AssetStudio's Export menu saves
   textures as PNG and audio as WAV/OGG.
   (GCFT README; BrawlInstallerPlugins README; AssetStudio README.)

9. **Car picker by name, not number (1 h, if the car stats table from 0.4.1
   C1 is reachable from the editor).**
   A combo box showing "12 - <car name>" with type-to-filter. Same pattern for
   any other field the schemas mark as an enum or index into a known table.
   Why: this is listed as a known limit; every inspector in the tools above
   shows names for references (TrenchBroom's entity browser, FModel's
   property view) - general convention, not a specific cited feature.

10. **(Already in the editor: the test drive's "from here".) Test drive from here.**
    Shift+F5 starts the test drive with the car placed at the current camera
    position/heading (or at the selected start), so testing a later section
    does not need a full lap.
    Why: ManiaPlanet's test mode lets you "instantaneously test your map at a
    certain position", Enter places the car at the cursor; Doom Builder family
    editors have "test from current position" (unverified).
    (ManiaPlanet editor doc.)

Suggested order for the day: 1 (start page, recent projects), 2 (click a
problem to go to it), 5 (status bar, unsaved marker in the title), 6 (controls
window, F1), 3 (ready-to-share badge), then 8 and 9 if time is left.

## Later

- Draggable axis gizmo with grid snap and a visible grid size (TrenchBroom:
  grid 0.125-256, snapping applies to most operations; Blender transform
  conventions).
- Automatic quick fixes and "hide this issue" in the errors list (TrenchBroom
  Issue Browser, Hammer Check for Problems "Fix").
- Build/test profiles: a list of ordered steps (compile, copy, launch) with a
  dry-run "Test" button that prints what would happen (TrenchBroom compile
  profiles).
- Rebindable shortcuts with conflict highlighting (TrenchBroom Keyboard
  preferences).
- Auto-detect the game install and list it at first run, with a manual
  "add" fallback (FModel Getting Started: detected games are offered when
  none is selected).
- Compare two data states and show only new/changed nodes, useful for
  reviewing what a mod changes (FModel "compare" of archive states).
- Drag and drop a pack, level folder or .xmod onto the window to open it
  (GCFT opens dropped files in the matching tab).
- Top-down orthographic map view for placing starts and triggers along the
  track (TrenchBroom 2D XY/XZ/YZ views; noclip Ortho mode).
- Freeze/step game time and savestates in the viewer, to inspect moving
  things (noclip: period freezes time, 1-9 load savestates).
- Hot-reload edited resources into the running game and a list of loaded
  resources (HE2 DevTools resource browser for Sonic Frontiers, which also
  runs inside the game like HWVX's viewer mode).
- Debug visualisation toggles for volumes/colliders in the game view (HE2
  DevTools level editor; Lorenzi's KMP editor shows course data such as
  checkpoints and respawn points visually).
- Preserve unknown sections on save and say so in the UI (Lorenzi's KMP
  editor states that sections it does not edit are kept).

## Sources

- TrenchBroom manual, https://trenchbroom.github.io/manual/latest/ -
  welcome window with recent maps, missing-game-path handling, info bar,
  automatic navigation, grid, Issue Browser, compile/launch profiles,
  rebindable shortcuts.
- noclip.website README, https://github.com/magcius/noclip.website - camera
  controls table, speed on scroll wheel, reset/hide-UI keys, time freeze,
  savestates.
- Blender manual, Navigation,
  https://docs.blender.org/manual/en/3.4/editors/3dview/navigate/navigation.html
  - Frame Selected (NumpadPeriod) and Frame All.
- ManiaPlanet "How to use the map editor",
  https://doc.maniaplanet.com/map-editor/how-to-use-the-map-editor - red/
  orange/green validation flag, test at a position, H helper dialog.
- Trackmania wiki, https://wiki.trackmania.io/content-creation/map-editor -
  author time set by validation run (supporting item 3).
- Valve Developer Community, Hammer Map Menu,
  https://developer.valvesoftware.com/wiki/HammerMapMenu and Check For
  Problems, https://developer.valvesoftware.com/wiki/Hammer_Check_For_Problems_Dialog
  - Alt+P check, non-modal, "Go to"; via search results (pages refused direct
  fetch). TWHL tutorial, https://twhl.info/wiki/page/Tutorial%3A_Diagnosing_Problems/8569
  - Go to / Fix usage.
- Dolphin bug tracker, https://bugs.dolphin-emu.org/issues/10406 and
  https://bugs.dolphin-emu.org/issues/9500 - empty game list text that tells
  the user to set a games directory.
- FModel wiki, Getting Started, https://github.com/4sval/FModel/wiki/Getting-Started
  - game auto-detection on first launch, manual add, archive compare.
- GameCube File Tools (GCFT), https://github.com/LagoLunatic/GCFT - drag and
  drop opening, right-click extract/replace/delete, per-format tabs.
- AssetStudio, https://github.com/Perfare/AssetStudio - File > Load
  file/folder, Export menu with per-type formats (PNG, WAV, ...).
- BrawlInstallerPlugins (BrawlCrate), https://github.com/squidgy617/BrawlInstallerPlugins
  - actions driven by right-click on tree nodes.
- HE2 DevTools, https://github.com/angryzor/sonic-frontiers-devtools -
  in-game editor: resource browser with hot reload, debug volumes/colliders.
- Lorenzi's KMP editor, https://github.com/hlorenzi/kmp-editor - visual
  editing of course points; unedited sections preserved on save.
- Ultimate Doom Builder: only forum posts found
  (https://forum.zdoom.org/viewtopic.php?p=1252536); F9 test map, F4 map
  analysis and "test from current position" are unverified.
