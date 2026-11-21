# HWVX Editor: crash and lag fixes (0.2.2)

2026-11-21. The editor 0.2.2 fixes everything both reviews found, except the items under "Left open" below. Paths are relative to HWVX-SDL.

## How it was done

- **First review:** six parallel reviews of the code as of `pc-data`.
- **Second review:** the code `main` gained since `pc-data` (`pc-data..32b00ff`, about 33k lines), plus a re-check of every first-review item on `main`.
- **Release:** an earlier 0.2.2 build was cancelled before it published, and its tag was deleted. The release ships both rounds of fixes:
  - **The first round:** `6640c24..fed6f6c`.
  - **This round:** seven fix branches merged into `editor-crash-lag`, plus a follow-up for `menu_ui` and the `Anim.c` fix below. That is 57 commits and 49 files.

## Verification

Run on macOS against the merged branch:

- **`tools/browser`:** `cargo test` passes with `HWVX_DATA` set to the GC data and hwvx-x on the PATH. That covers 15 groups, including `world`, `protocol`, `level_edit`, `testdrive` (6), `frames`, `media` and `content_music`.
- **Windows:** `cargo check --target x86_64-pc-windows-gnu` is clean.
- **`tools/xdlrs`:** `cargo test --release` passes with GC data. That includes `joyride_templates` (every mode, collectibles kept and removed, on all 5 Joyride worlds) and the real-disc tests.
- **Game:** builds with Ninja, with no new warnings.
- **C1 scenario:** a temporary test applied ch1_l2 8 times while items spawned, took it out, switched worlds and applied it again. It ran cleanly under AddressSanitizer, and items did not pile up.
- **C13:** killing the editor ended the hidden game within 1 s.
- **Movies:** seek results match decoding from the start, on GC THP and PS2 PSS.
- **Not run:** the editor was never opened by hand, so the new spinners, gizmo throttle and outline scrolling haven't been seen on screen. The C code was only syntax-checked for Linux and Windows; the CI release build compiles it there.

## What changed, by area

| Area | Items | Notes |
| --- | --- | --- |
| Game level code (`Port_Level.c`) | C1, C15, C16, L30, L31, T5 | Spawned things are handed back or released before Unapply frees. Copies start with no runtime slots. `modeWorld` failures no longer touch the live pack, and the mode pack is kept across applies. BSP bounds are cached. Emitter.c: `ActionFXPointChangeMode` no longer reads NULL. |
| Game animation (`Anim.c`) | (new) | The last leaf bone popped below the hierarchy stacks. Found by AddressSanitizer via `Viewer_DrawCar`; the values were unused. |
| Viewer server | L8, L10-L13, camera, MIDI, bones | A still preview draws at about 4 fps when nothing changes. Region boxes are spread over frames, and overlays are rebuilt only when their inputs change. Relocations are sorted once, the TOC is cached, and node lookup is hashed. Camera values are finite-checked and clamped. `DrawModel` was capped at 40 bones (the real overrun). |
| Process and platform | C13, W3, L1, L16, L2, W6, W8, seq | The game exits when the editor is gone, plus a Windows job object. `\\?\` paths are stripped, `Env` is pooled, and timeouts are longer, with late world loads kept. Frames are uploaded without an extra copy. On Linux, `/dev/shm` is reserved up front with a clear error. A second editor gets its own scratch folder, and extraction takes a lock that works across processes. |
| Level editor | T1-T4, L29, L14, L15, L7, C14 | Templates compile both ways. "Keep them" restores only the collectibles' triggers, and the stuck undo after a drag is fixed. Gizmo drags are throttled, with one snapshot per drag. Per-frame results are memoised, and `write_copy` runs in the worker. The outline is virtualised, and `store_value` no longer panics. |
| Content editors | C17, C24-C27, L6, L23-L28 | Crashed workers clear the busy state. Hand-edited `patch` and non-ASCII names no longer panic. `playable` is capped. Unlink and Delete never remove a linked project. Every dialog goes through `ask_then`, uses UTF-8 and reports missing zenity. Audio decodes once on a thread, archives are read on a thread with a bounded cache, and per-frame listings are cached. Imports and test copies run off the UI thread, and the playlist uses one worker. |
| Disc extraction | L17, L18, C22, C23, C28, W9, W10, W4 | The destination is checked off-thread, debounced and without following links. Cancel and progress work every MB. The `.cue` BOM and multibyte lines are handled, sizes are checked before allocating, and the gcz table is bounded. Only GameCube images count as data. Windows-unsafe and case-twin names are refused. The remembered path keeps its spaces. |
| Movies | C17-C21, L19-L22, thp.c | No Instant underflow. Header sizes are bounded, and a frame's own channel count is used. Greyscale JPEGs are expanded. Probing and opening run on threads, and Bink/PSS seek by key frame (a PS2 seek went from about 2 s to 0.02 s). The WAV is written off-thread, repaints follow the frame rate, and decodes can be cancelled. The game's `thp.c` gets wrap-safe sizes and checked reallocs. |
| First round (earlier) | C2-C12, L1-L6, W1, W2, W7 | See the git history of `fed6f6c`'s parents. |

## Left open

- **L9:** a world load still runs in one command. The window keeps responding (`SDL_PumpEvents` every 50 ms), but the load isn't spread over frames.
- **Movie seeking:** Bink seeking is untested because no Bink files are available locally. A failed seek falls back to decoding from frame 0.
- **Movie WAV:** the player programs can't start mid-file, so the WAV is still written on each play or seek, now off the UI thread.
- **Test drive on crash:** the test-drive game is deliberately not killed when the editor crashes. It is a visible window the player can close.
- **One dialog at a time:** `ask_then` runs one dialog at a time. A second dialog action while one is open is refused with a message.
- **Smaller items, not done:**
  - `FileStreaming.c:635-640`: a share pack with no sounds may be reloaded without the old copy being freed.
  - `Port_ContentData.c` `OpeningOnAnotherWorld` leaks `kept`.
  - `ffmpeg.rs`: two copies of libavutil could be loaded at once.
