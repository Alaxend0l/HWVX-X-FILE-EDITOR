# Custom worlds: auto-generated chunks (todo)

Goal: `hwvx-x world build` splits a custom world into quadrants (chunks) automatically, and the game
streams them like retail quadrants, instead of today's single hub quadrant. Next priority after HWVX
Editor 0.2.0. Background: `docs/plans/custom-worlds.md` (read all of it, incl. §6 onwards: what is
built, the collision passes, the later stages on streaming).

## Rules (from the user)

- Rust for the builder and tools (`src/custom_world/` in this repo), C for the game (HWVX-SDL); never
  Python, never `cargo fmt`.
- General rules for any modeller's world, nothing tuned to one model. Lava Cage (the user's Roarcage
  level) is a test case only: never commit it or anything derived from it.
- Work on a branch in a worktree (`world-chunks` in both repos); don't push or merge without the user.

## Where things are

- Game: `HWVX-SDL/src/game/Port_World.c` (a world laid over a base level as it loads; one hub
  quadrant today), the world loader and streaming (Quad/Region structs, neighbour lists), the
  prototype-worlds path `Port_ProtoWorldLoaded` (precedent), `src/port/viewer_world.c` (the viewer's
  `worlds`: quadrants loaded around the camera).
- Builder: `src/custom_world/{mod.rs,terrain.rs,gltf_in.rs}` here; `hwvx-x world build|demo|smooth|inspect|probe`.
- Data: `orig/gc/HWVX GC RIP/files`, `orig/pc`, `orig/ps2` (gitignored).
- Lava Cage: `~/Develop/wt-worlds/test-assets/lava_cage.jsonc` (scale 2.8333), built into
  `~/Develop/lava-cage-world`.

## Todo

1. [ ] **Study retail chunking** and write the numbers into custom-worlds.md: quadrant sizes and
   shapes, triangles/textures/bytes per quadrant, how regions and neighbour lists decide what loads,
   draw distance (Normal / Far / Everything), how collision relates to quadrants (per quadrant or
   world-wide), memory limits (the port's lifted ones). Measure several retail worlds per release.
2. [ ] **Design** the split: a spatial grid or adaptive subdivision by size and triangle density
   within the retail budgets; objects kept whole where sensible; neighbour/region lists generated.
   Decide collision: keep the existing world-wide cell grid, or per chunk (justify; borders must stay
   seamless).
3. [ ] **Builder:** implement the split and the streaming lists; project options to override (chunk
   size, budgets) with defaults that work; small worlds keep the hub-only path.
4. [ ] **Game:** load and stream the chunks in Port_World.c like retail quadrants; respect the draw
   distance setting; hub-only path unchanged.
5. [ ] **Synthetic tests:** `hwvx-x world smooth` at several sizes, plus a generated large world (e.g.
   a 4 x 4 km terrain with many objects); unit tests on the split (budgets met, every triangle in
   exactly one chunk, neighbour lists symmetric).
6. [ ] **Prove in the game:**
   - chunks load and unload around the car while driving (log lines; the viewer's loaded-quadrant list);
   - frame time and memory against the hub build;
   - collision unchanged: the builder's self-checks (phantom / hole / grazing-ray rates),
     `tools/world_pitch.sh` pitch spikes per 100 units, `tools/world_drive.sh` drives;
   - no seams or gaps at chunk borders (frames; `--show-position` and F9 to mark spots);
   - retail unchanged: `tools/demo_compare.sh` attract demo identical, a retail race's car log
     identical to main's.
7. [ ] **Lava Cage check:** rebuild `~/Develop/lava-cage-world` with chunks (keep the previous build as
   `lava-cage-world.bak-chunks`) so the user can drive it.
8. [ ] **Docs:** the design and numbers in custom-worlds.md; the README's `HWVX_WORLD` note; the
   editor guide's custom worlds part if user-visible options change.

## Related open items (not part of this)

- The world's scale: adjustable in a future "New world from a model" flow (default undecided).
- Custom worlds over world 3/5 bases on the PC (texture numbering).
- World 5's own mode-screen texture; drag results' clock.
