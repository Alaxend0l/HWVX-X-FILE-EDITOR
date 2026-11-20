# HWVX Editor 0.2.0: what's left

State on 2026-11-20: HWVX-SDL `main` has the Content tab (new cars and weapons, retail changes,
sounds, Soundtrack, tracks with own pictures), the Media tab (every release's movies, the Audio
browser), the gizmo drag and patches beyond numbers, the level limits lifted, the disc ripper
(`xdlrs disc`), PC/PS2 movies through FFmpeg, the launcher's Credits tab, and the release checks.

## Blockers

1. ~~FFmpeg packaging~~: **done** (merged 2026-11-20; CI run 44720015219 green on every platform,
   after `tar --no-same-owner` for the Flatpak sandbox). The Flatpak overlayfs fix ran on Azure
   that time: proven once an x86_64 Flatpak job lands on AWS (its step prints the mount). The
   Windows editor build fix (`ec1d98c`) still wants a check: `cargo check --target
   x86_64-pc-windows-gnu` in tools/browser (MinGW is installed), or the editor CI.
2. **Two editor tests failing since before v0.4.1**: `tools/browser/tests/protocol.rs:66` (a web
   POST to the viewer should get no reply) and `tests/world.rs:85` (the world drawn: too few
   colours). Understand before release.
3. **The guide**: fold `tools/browser/docs-content-{tracks,music,sounds,retail}.md` and
   `docs-media.md` into `docs/level-editor.md` (section 9b and a Media section).
4. **Custom worlds**: the phantom hits are fixed (merged 2026-11-20; the user: "collision has been
   nearly flawless"). Left: the base level's moving and animated things still show in a custom
   world (on Lava Cage over Monument City: the train, a spinning tire). `src/game/Port_World.c`
   drops the base's volumes, speakers, creatures, radar map and moves its teleporters; it must also
   drop (or hide) the base's animated instances, movers and paths (the train's track), keeping
   only what the world asks for. Ship custom worlds marked experimental.

## Suggestions from the user (while trying the editor)

- **"Extract a disc…"** (File menu, and on the game data screen when a PC or PS2 image is picked):
  choose the image and a folder, a progress bar, then use the result as the game data. The work
  is `xdlrs::disc` (`Disc::open`, `extract`); the editor only needs the UI. About an hour.

## After 0.2.0

- **Auto-generated chunks** for custom worlds (the user's next priority once 0.2.0 is out): split a
  world into quadrants/regions with streaming, instead of one hub (docs/plans/custom-worlds.md,
  stages after 3), by general rules (size, triangle density, a spatial grid), measured on several
  meshes; Lava Cage as the real-world check.

## Open decisions

- The builder's default scale for glTF worlds in metres: ~2.8 units a metre (Lava Cage felt right
  at 2.8333) instead of 8.5? The plan (`custom-worlds.md`) still says 8.5.
- Custom worlds over world 3 and 5 bases on the PC (texture numbers differ): map them in the game.
- Credits page: a named credit for the original game's developer and publisher (wording from the
  user), and whether to keep the HWVX-GC-Decomp line.
- `test_content` should also find GameCube data at `orig/gc/*/files`.
- The beginner UX plan (`editor-ux-for-beginners.md`): its top items after 0.2.0.
