# HWVX Editor: crash and lag fixes

2026-11-21. Six reviews read the code in parallel: the browser UI; IPC and process handling; the level editor; the viewer server core; the viewer's world, level and media code; and xdlrs/hwvx-x. Paths below are relative to HWVX-SDL.

The fixes are on HWVX-SDL branch `editor-crash-lag`, rebased onto `origin/main` (HWVX Editor 0.2.1), as 4 commits. The branch has not been pushed, and the superproject's HWVX-SDL pin has not changed.

Verification:

- `tools/browser`: `cargo test` and `cargo check --target x86_64-pc-windows-gnu` pass.
- `tools/xdlrs`: the level tests pass.
- The game builds.
- The viewer tests that need game data finished in 0.00 s, so they probably skipped. A run with the data, and on Windows, is still to do.

## Crashes

| # | Where | What goes wrong | Trigger | Status |
| --- | --- | --- | --- | --- |
| C1 | `src/game/Port_Level.c` Unapply; `FileStreaming.c:415`; `ItemThing.c:59` | Re-applying a level frees the grown item arrays. Items already spawned still point into them, and Item_Release later writes into freed memory. | Edit a level that adds items after they have spawned (the editor re-applies about 250 ms after every edit), then move the camera or switch worlds. | **Open.** This is the most likely real crash. Fix: release the level's spawned items (or detach `thing->base.trigger`) in Unapply before freeing. |
| C2 | `src/port/viewer_world.c` Load | A failed world load leaves LoadWorldProcess's static state mid-way and the packs loaded. The next `world` resumes the failed load. | A broken mod world or a prototype world, then any other world. | **Fixed.** Added `LoadWorldProcess_Reset`, and the viewer unloads what did load. |
| C3 | `tools/browser/src/client.rs`, `testdrive.rs` | The log reader stops at the first non-UTF-8 line. The game's log pipe then fills and the game blocks. | A log line with Latin-1 or Shift-JIS bytes. | **Fixed** (`lossy_lines`). |
| C4 | `client.rs` reply reader | Non-UTF-8 in a reply ended the reader, so the connection dropped while the game still ran. | A non-UTF-8 name in a pack. | **Fixed.** |
| C5 | `src/port/viewer.c` LoadShares / CmdLoad | The game has 3 share-pack slots. With a 4th share needed, the pack loader dereferences NULL (`DataGamePackLoad.c:660` and others). | Packs from several worlds plus a car loaded without a world. | **Fixed:** the load is refused with an error. Better later: free shares nothing uses. |
| C6 | `tools/browser/src/media_ui.rs` | `clamp(32, n)` panics when n < 32. Hovering an empty sample read `pcm[0]`. | Mouse wheel over a short sound. | **Fixed.** |
| C7 | `media.rs` / `media_ui.rs` | A texture with no levels was indexed anyway. | A texture with 0 levels. | **Fixed:** it is now an error. |
| C8 | `level_edit.rs` edit_check | A panicking check thread was never marked done, so a new check spawned every frame. | Any compile panic. | **Fixed.** |
| C9 | `tools/xdlrs/src/level.rs` base_project | The edit check, test drive and Save as mod extract into one cache. One removed the folder another was writing. | F5 or Save as mod during the first check. | **Fixed** within one editor (a lock). Two editors at once: **open.** |
| C10 | `level_model.rs` load | One stray `triggers/*.jsonc` (`._a.jsonc`, `a copy.jsonc`) or a UTF-8 BOM stopped the project opening. A non-object `level.jsonc` panicked on the first edit. | USB drives, copied files, Notepad. | **Fixed.** Stray files are left out, a BOM is accepted (here and in xdlrs `jsonc::loads`), and a non-object root is refused on load. |
| C11 | `viewer.c` raycast | A `from`, `to` or `down` that was an object, not an array, dereferenced NULL. | A malformed request. | **Fixed.** |
| C12 | `tools/browser/src/ffmpeg.rs:130` | `major - 2` underflows for the unversioned library names. The `movies` test failed on main. | No versioned FFmpeg found. | **Fixed.** |
| C13 | `client.rs` Drop | When the editor crashes or is killed, the game keeps running with its window hidden. | Each crash adds one more background game. | **Open.** Fix: a job object on Windows, a parent-PID check in the viewer, or exit when no client is back within N seconds. |
| C14 | `main.rs` `store_value` | serde_json `IndexMut` panics on a path that doesn't fit. | Unlikely. | Open. |

## Lag and freezes

| # | Where | What | Status |
| --- | --- | --- | --- |
| L1 | `xdlrs/src/level.rs` compile / base_project | Every edit check rebuilt a lot: it walked the whole data folder and SHA-256'd the whole world archive. It also re-hashed the schemas (the cache was thread-local and every check runs on a new thread), and resolved the base and compiled twice when there was a spawn box. | **Fixed.** The schema hash is cached per process; the archive lookup and hash are cached per process, keyed on size and mtime; the base is resolved once. The `Env` (schemas parsed) is still rebuilt per check: **open.** |
| L2 | `main.rs` frame feed | The frame thread copied each frame (up to 33 MB at 4K) while holding the mutex the UI thread locks every frame. | **Fixed:** the copy is made outside the lock. Each frame is still copied into a `ColorImage` and uploaded in full: **open.** |
| L3 | `level_edit.rs` `schema()` | It cloned the 314 KB Trigger schema several times per frame. | **Fixed** (`Rc`). |
| L4 | `level_edit.rs` edited | The undo history had no cap: a full level clone per burst of edits. | **Fixed** (cap of 500). |
| L5 | `world_ui.rs` world_status | It re-sent `outline` on every status or input reply until the first answer came back. | **Fixed.** |
| L6 | `level_edit.rs` / `level_ui.rs` dialogs | osascript, PowerShell and zenity ran on the UI thread, so the window froze ("Not Responding" on Windows). | **Fixed** for Open level, the new level's folder, and Save as mod. Still synchronous: `content_ui.rs:436` and `menu_ui.rs:258`. **Open.** |
| L7 | `main.rs:1064`, `1620` outline tree | The whole tree is laid out every frame (`ScrollArea::show`, not `show_rows`), and the editor repaints at the game's frame rate. | **Open.** |
| L8 | `viewer.c:2245` | A full render and GPU readback run every frame while the preview is on, even when nothing changed. | **Open.** |
| L9 | `viewer_world.c:302` | A world load runs in one command and busy-waits, so the game window stops responding for seconds. | **Open.** |
| L10 | `viewer_world.c:163` RegionBoxes | About 106k BSP tests per quadrant in one frame. | **Open.** |
| L11 | `viewer_world.c:459` overlays | All overlay lines are rebuilt every frame. | **Open.** |
| L12 | `viewer_media.c:402` `textures` | O(textures × relocations), and it runs on every outline load. | **Open.** |
| L13 | `viewer.c:431`, `viewer_graph.c:440`, `:649` | `packs` walks the whole data folder, and `load` reads the table of contents twice. Node lookup is a linear `strcmp` over up to 200k nodes. | **Open.** |
| L14 | `level_edit.rs` per frame | `uses_of` serializes every trigger, `dirty()` deep-compares the whole level, and each list row does a `format!` per error. | **Open.** |
| L15 | `level_edit.rs:404` `write_copy` | It rewrites every trigger file on the UI thread 250 ms after each edit. | **Open.** |
| L16 | `client.rs:125`, `main.rs:56` | The 60 s start and reply timeouts. A slow first start (HDD, antivirus) is treated as a failure. | **Open.** |

## Windows and other platforms

| # | What | Status |
| --- | --- | --- |
| W1 | The world-pack test drive used `symlink_dir`, which needs admin rights or Developer Mode. | **Fixed:** falls back to a copy. |
| W2 | `find_hwvx_x` looked for `hwvx-x` with no `.exe`. | **Fixed** (`EXE_SUFFIX`). |
| W3 | `canonicalize` gives `\\?\C:\...` paths, and the viewer appends `/` to `HWVX_VIEWER_SCHEMAS`. That probably breaks loading the schemas (`viewer: 0 schemas`). | **Open, unverified.** Check a Windows `viewer.log`. A fix is `dunce::canonicalize` or stripping `\\?\`. |
| W4 | PowerShell dialog output isn't UTF-8, so non-ASCII paths come back garbled. | **Open.** |
| W5 | Linux without zenity: the "…" buttons silently do nothing. | **Open.** Show a message instead. |
| W6 | Linux `/dev/shm`: a small one (64 MB under Flatpak) refuses 4K frame buffers, and segments are left behind after a crash. | **Open.** |
| W7 | `find_archive` followed symlinked folders. | **Fixed.** |
| W8 | Two editor instances share `pref/testdrive` (edit copy, preview level, base cache). | **Open.** Use a per-process subfolder, or a lock file. |

## Smaller items

- `viewer.c:1478` and `:1586`: camera distance, zoom and speed values are not range-checked, so `1e999` becomes infinity.
- `viewer.c:575`: `DrawOne` passes 8 bone matrices. A mesh with higher bone indices would read past them.
- `viewer_media.c:613`: the MIDI track size is bounded by the song header's own field, not by the buffer.
- `client.rs:254`: a reply with no `seq` is taken as the current call's reply.
- `FileStreaming.c:628` (not verified): a share pack with no sounds may be loaded again on each use without the old copy being freed.

Checked and found fine:

- Shared-memory names fit macOS's 31-character limit.
- The seqlock order is correct on both ends.
- SIGPIPE is handled (`SO_NOSIGPIPE` on macOS, `MSG_NOSIGNAL` on Linux).
- JSON parsing is depth- and size-capped.
- Client indices are bounds-checked.
- hwvx-x `extract` and `pack` run on worker threads with stdin set to null.
