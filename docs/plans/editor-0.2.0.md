# HWVX Editor 0.2.0: what's left

State on 2026-11-20: HWVX-SDL `main` has the Content tab (new cars and weapons, retail changes,
sounds, Soundtrack, tracks with own pictures), the Media tab (every release's movies, the Audio
browser), the gizmo drag and patches beyond numbers, the level limits lifted, the disc ripper
(`xdlrs disc`), PC/PS2 movies through FFmpeg, the launcher's Credits tab, and the release checks.

## Blockers

1. **FFmpeg packaging** (branch `ffmpeg-packaging` in HWVX-SDL and the superproject, worktrees
   `~/Develop/wt-ffmpeg`, `~/Develop/wt-ffmpeg-x`): push the branch to run CI on every platform (it
   also carries the Flatpak x86_64 fix, `flatpak-ci-fix`), merge when green. The editor packages
   need it for Bink and PS2 movies. The Windows editor build fix (`ec1d98c`, xdlrs `shaders.rs`) is
   untested on Windows: MinGW is now installed here (`cargo check --target x86_64-pc-windows-gnu`).
2. **Two editor tests failing since before v0.4.1**: `tools/browser/tests/protocol.rs:66` (a web
   POST to the viewer should get no reply) and `tests/world.rs:85` (the world drawn: too few
   colours). Understand before release.
3. **The guide**: fold `tools/browser/docs-content-{tracks,music,sounds,retail}.md` and
   `docs-media.md` into `docs/level-editor.md` (section 9b and a Media section).
4. **Custom worlds**: phantom hits at small scales (branch `worlds-edges`, in progress). Ship
   marked experimental, or wait.

## Suggestions from the user (while trying the editor)

- **"Extract a disc…"** (File menu, and on the game data screen when a PC or PS2 image is picked):
  choose the image and a folder, a progress bar, then use the result as the game data. The work
  is `xdlrs::disc` (`Disc::open`, `extract`); the editor only needs the UI. About an hour.

## Open decisions

- The builder's default scale for glTF worlds in metres: ~2.8 units a metre (Lava Cage felt right
  at 2.8333) instead of 8.5? The plan (`custom-worlds.md`) still says 8.5.
- Custom worlds over world 3 and 5 bases on the PC (texture numbers differ): map them in the game.
- Credits page: a named credit for the original game's developer and publisher (wording from the
  user), and whether to keep the HWVX-GC-Decomp line.
- `test_content` should also find GameCube data at `orig/gc/*/files`.
- The beginner UX plan (`editor-ux-for-beginners.md`): its top items after 0.2.0.
