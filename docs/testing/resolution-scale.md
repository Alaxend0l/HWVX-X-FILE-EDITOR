# Resolution scale (HWVX-SDL 0.4.1 part E): E3 checks

How the scale works is in HWVX-SDL's README (`HWVX_RESSCALE`, `resscale=`). These are the release
checks: macOS on 2026-11-13 (branch resscale-e3), Linux on 2026-11-20 (before tagging v0.4.1).

## macOS (2026-11-13, M2 Max)

M2 Max, macOS, GC data. Window 1280x960 points (2560x1920 pixels at 1x) unless said. Dumps
compared at 1x size (area downscale/upscale) with ffmpeg SSIM; window captures (screencapture -l,
the game's window only) for the present path. Pass = looks right next to 1x, no crash, HUD and
views in place, dumps at render size.

| | Metal | OpenGL | Vulkan (MoltenVK) | D3D12 (CrossOver) |
|---|---|---|---|---|
| attract demo 0.5 / 0.75 / 1.5 / 2 | pass (SSIM 0.97-0.98 / 0.99 / 0.995 / 0.995) | pass, same numbers | pass, same numbers | pass (1280x960 px window; 0.96-0.98 / 0.98-0.99 / 0.99 / 0.99) |
| race with shadows (challenge ch1_l2), 0.5 / 2 | pass (0.992 / 0.999) | pass | pass | pass at 2 (0.998) |
| stencil vs HWVX_SHADOW_COPY=1, 2x | identical | identical | identical | - |
| battle race 0.5 / 2 (no shadows: split-view mode) | pass | pass | pass | - |
| split screen 2/3/4 players, 0.5 / 2, 4:3 and 16:9 | pass | pass (4P) | pass (4P) | pass (4P 16:9 2x) |
| 4K window (clamped to 3600x2196 px), 2x = 7200x4392 | pass | pass | pass | - |
| cap (forced 4096 in a test build): 2x 5120x3840 -> 4096x3072 | pass, logged once | pass, logged once | - | - |
| window present (0.5 linear/nearest, 1, 2 box) | pass | not capturable (screencapture can't read GL windows) | pass | pass (0.75, 1.5, 2) |
| FPS strip (HWVX_FPSHUD) | fixed: same size at any scale | - | pass | pass |

- 1x byte-identical: tools/demo_compare.sh against pc-data (attract demo), and a 16:9 one-player
  race and 16:9 attract demo against pc-data.
- Renderers agree with each other at every scale (SSIM 0.9999+ between Metal, OpenGL, Vulkan; D3D12
  vs Metal 0.9999 where the sim agrees). The Windows build's attract demo diverges from the macOS one
  later in the demo (frame 3000: another moment of the race, not a rendering difference).
- Shadows: the challenge race draws them (HWVX_GPUTIME: shadow volumes, mask draw; the copy path's
  one EFB copy a frame at 5120x3840); the attract demo and battles draw none (the game skips them
  with more than one view bit).
- Editor: the browser's embedded view (tools/browser_embed_check.sh with HWVX_RESSCALE=1/2/0.5) is
  unscaled as designed, its frames 1652x1342 whatever the setting; the viewer in its own window
  (HWVX_VIEWER_WINDOW=1) at 2x reports 3200x2400, takes picks in those pixels (the centre picks the
  same node as at 1x) and saves screenshots at that size.

Found and fixed (resscale-e3):
- Split screen on a wide window was pillarboxed (the views mapped into the centred 4:3 area). Now
  the views fill the window, each projection widened like one view's (gx_draw.c MapViewport,
  Port_ViewWiden in split screen too), and each player's HUD moves with its view: centre with the
  view's centre, sides to the view's edges (InterfaceManager.c Port_HudPane/Port_HudShift; split
  thresholds 256/384 of 640 so the battle's speed stays by its radar).
- The game's 2- and 4-player HUDs sat 10 units toward the screen's middle lines (offsets 10/310,
  10/230 for views at 0/320, 0/240): now at their views' corners, 4:3 included (split screen only).
- The FPS strip shrank at 2x and grew at 0.5x (sized in EFB pixels): now sized for the window.
- HWVX_START=race:...,players=P (1-4): P local players, as the split car select leaves them (for
  split-screen tests; mpbattle/mpdrag).

Not tested here: a real GPU cap (16384 on every renderer here; only a forced 4096), a physical 4K or
external display, OpenGL's presented window (dumps only), D3D12 on real Windows hardware, the
editor's Test drive (it starts the game normally, so it follows graphics.cfg like any run).

## Linux (2026-11-20)

AMD Radeon 8060S (RADV STRIX_HALO, Vulkan 1.4), Fedora, Wayland compositor with the game under
Xwayland, GC data. Window 1280x720 (a tiling or scrolling window manager must float the game's
window, or it resizes it and the dumps are the tile's size). `tools/run_test.sh` with FAST=1,
`HWVX_PROMPTS=original`; dumps compared with the 1x run at the same frame (box-downscaled to 1x,
mean absolute grey difference out of 255).

| | OpenGL | Vulkan |
|---|---|---|
| attract demo 0.5 / 2: dump size, diff vs 1x | pass: 640x360 / 2560x1440, 1.7 / 0.4-1.4 | pass: 640x360 / 2560x1440, 1.7-3.3 / 0.3-0.5 |
| battle, 4 players (`HWVX_START=race:mode=battle,players=4`), 16:9, 0.5 / 1 / 2 | pass: views fill the window, HUDs in each view's corners; 3.8 / 1.9 | pass, same; 3.7 / 1.7 |

- OpenGL and Vulkan agree at 1x: attract demo 0.01-0.03, 4-player battle 1.4.
- Editor: `tools/browser_embed_check.sh` with `HWVX_RESSCALE=1` and `=2`: passes, the same 1416x1770
  screenshot both times (the embedded view is not scaled).
- Tests: `ctest` (HWVX_TESTS=ON, 12 tests, GC data) and `cargo test` in tools/xdlrs pass. The tests
  build needed `netbot` linked with libm on Linux (fixed for the release).

Not tested here either: the cap on a real GPU, a physical 4K display, D3D12 on Windows hardware.
