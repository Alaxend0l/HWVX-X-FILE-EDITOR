# Windows memory and performance (task #31)

For HWVX-SDL/README.md (the Windows paragraph under "Renderers" and the
HWVX_STATS notes) and packaging/README.md (Windows build).

## The report

v0.3 on Windows, GameCube data from an NKit image: Task Manager showed
2,186.7 MB and the game felt slow.

## Measured under CrossOver/Wine (macOS, Win10 64-bit bottle `hwvx-win64`)

The GameCube attract demo (deterministic, no input), D3D12 (SDL_GPU's
default on Windows; CrossOver provides it). `tools/wine_run.sh` runs it,
`tools/stats_summary.py` sums the log by the minute. "Resident" is the Wine
process's resident size as macOS sees it. Wine's PrivateUsage (Task
Manager's figure) is useless: it stays at 5121 MB and sometimes wraps.

| build | minutes | fps | work ms/frame | resident MB (end) | SDL transfer buffers kept |
|---|---|---|---|---|---|
| v0.3 | 10 | 59.7-60.0 | 1.1-2.0 | 401 -> 542 (flat from min 4) | (not logged) |
| pc-data | 10 | 59.8-60.0 | 0.7-1.3 | 377 -> 457 (flat from min 6) | 53 MB |
| winperf | 3 | 59.7-60.0 | 0.8-1.2 | 373 -> 430 | 6-13 MB |

So under Wine there is no growing leak in the attract demo, in v0.3 or
now, and no slowdown. The 2.2 GB isn't reproduced. The texture-cache leak
fixed in 9de5e41 was macOS-measured: 494 textures, 48 MB after 8 minutes.
It is the likely main cause on a real GPU driver, where each texture also
costs driver memory. The fix below is the other place where memory piled
up on the Windows path.

## Fixed

- **SDL_GPU transfer buffers** (`gx_sdlgpu.c` CopyPass). Before, the
  game's staging copies went through one transfer buffer that grew to the
  largest batch ever (a level load's textures and meshes: about 35-53 MB).
  It was mapped with `cycle=true` at every batch. SDL (D3D12, Vulkan,
  Metal alike) keeps every buffer it cycles to, each one the whole size,
  until the buffer is released. So the memory kept was batches in flight
  times the largest load. In the attract demo that is one batch a frame,
  so 53 MB. Menus and races that copy the EFB or clear more than once a
  frame would cycle more copies. That part is reasoned, not measured on
  Windows.
  Now:
  - a batch over 8 MB gets a transfer buffer of its own, released once it
    is copied;
  - the kept transfer buffer and the loose-vertex buffer are appended to
    within a command buffer, and cycle only at its first batch.
  Kept: 53 -> 6-13 MB (Wine). Frames are the same: GC, PC and PS2 demos are
  frame-identical to pc-data on macOS.
- **The Windows build failed** on pc-data: `main_sdl.c` used printf
  without `<stdio.h>`, which MinGW doesn't pull in. Fixed.
- **libwinpthread-1.dll.** A POSIX-threads MinGW (Homebrew's, MSYS2's)
  made the .exe import it, for `_Thread_local` in Port_Mix.c and xdl.c.
  Windows doesn't have that DLL. The exe now links with `-static`. CI's
  Ubuntu MinGW (win32 threads) didn't hit this.

## NKit images

Dolphin (Source/Core/DiscIO) reads a GameCube NKit image (`.nkit.iso`)
like any plain image. Its only special case is `IsNKit()`: "NKIT" at
0x200. NKit drops the junk between files and rewrites the file table's
offsets to the packed layout, so `disc.c` reads it through the table as
usual. It reads only what is asked for, with no whole-image load, and
needs no change.
- `.nkit.iso` ends in `.iso`, so it is found.
- `.nkit.gcz` is refused with the convert-to-.iso message, like `.gcz`.

`tools/make_gc_image.py DISC OUT [--nkit]` builds a plain image or one
with NKit's layout from an extracted disc (sys/ + files/). Its output is
the player's data: never commit it.

Verified on macOS with the GC attract demo, 40 s, deterministic:
- frames identical from the folder, a plain .iso and an NKit-layout
  .nkit.iso (254 compared, 0 different);
- footprint at frame 4260: folder 490 MB, .iso 595 MB, .nkit.iso 595 MB.
  The NKit layout costs nothing over a plain image. Image vs folder is
  about +100 MB at that frame, but folder runs alone vary by 70 MB
  (422/490). Another agent's game was running at the same time, so this is
  not conclusive. It is not a whole-image load either way: the image is
  815 MB.
Not tested: a real NKit dump (none here, and no nkit tool), and image
reading under Wine.

## Other Windows-path checks (reasoned, from the code)

- Logs: a GUI build sends SDL's log to OutputDebugString only, which costs
  nothing without a debugger. Nothing logs per frame unless HWVX_STATS or
  HWVX_FPSLOG is set.
- The display-list cache is bounded (8192 entries, one vertex pool). The
  GL backend reuses its buffers. Pipelines are capped (MAX_PIPELINES).
  Audio uses one ring.
- Pacing: SDL_DelayPrecise to 60 Hz, then the vsync wait.
- Not tested: real Windows drivers, OpenGL under Wine, and Vulkan under
  Wine (CrossOver's bottle gave D3D12).

## HWVX_STATS additions

- `process:` the process's memory: Windows private bytes and working set,
  macOS physical footprint and resident size, Linux RSS.
- `staging:` copy passes per frame, and the MB of transfer buffers SDL
  keeps, counting every copy it cycled to.
