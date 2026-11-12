# Asset limits for modders (measured)

Linked from README.md §8.2.3. Line numbers are as of HWVX-SDL `pc-data` 92fea6b.

Each limit names the place it comes from and what kind it is:

* **format**: the retail data's layout. Raising it means a new pack format (`.xdl`), or new
  content only.
* **engine**: the original game code, kept by the port.
* **port**: a port constant.

"Test" names what hits the limit. Release numbers come from `tools/asset_stats.py` (all three
releases, counts and extremes only) and from runs with `HWVX_HEAPLOG=1`.

## Summary

| what | limit | kind | where | retail uses | raised here? |
|---|---|---|---|---|---|
| texture size | **2048 x 2048** (was 512 in the game, 1024 in GX_TexGet) | port | `PORT_MAX_TEX_SIZE` port.h; Render_Texture_.c:81; gx_tex.c:766, 879 | 256 x 256 at most, every release | yes, 512 → 2048 |
| texture mip levels | 12 (full chain of 2048); the game uses `(int)((n-1)/1.15)+1` of n; GC RGBA8 has none | port / engine | `PORT_MAX_TEX_LEVELS`; Render_Texture_.c:87, :65 | GC 0-5, PC/PS2 up to 9 | yes, 11 → 12 |
| texture formats | GC CMPR, RGBA8 (+ I4/I8/IA4/IA8/RGB565/RGB5A3 in GX); PC DXT1, DXT5, RGB8, RGBA8; PS2 PSMCT32, PSMT8 | format | gx_tex.c:63-73, 229-330; Port_PS2.c:6-15 | GC 1/4, PC 4/5, PS2 1/2 | - |
| non-power-of-two | accepted; sizes not a multiple of 4 upload as RGBA8 | port | gx_tex.c:75-79, 612 | none in retail | - |
| texture memory | `HWVX_TEX_BUDGET` 512 MB of GPU textures, least recently used dropped (no error) | port | gx_tex.c:845-869 | a race: 225 textures, 17 MB | no need |
| texture cache slots | 1024 entries (2048-slot hash); full → LRU | port | gx_tex.c:49, 360 | | - |
| texture ids | 0x1FF retail, 0x7FF with prototype content | engine / port | port.h `PORT_MAX_TEXTURES` | | - |
| vertices per level | GC: 65,536 of each attribute (u16 indices); PC: **32,767** (bit 15 flags strips) | format | xdl_gltf.py `_enc_gc_level`/`_enc_pc_level`; Port_Geometry.c:47 | 8,311 (worlds) | GC encoder splits draws, see below |
| triangles per group (one draw) | GC: 21,845 per draw command (u16 count); the encoder now emits several | format → tool | xdl_gltf.py `MAX_DRAW`; gx.c:145 (`GXBegin` u16) | | yes (tool) |
| position precision | f32 everywhere, no fixed point (POS F32 frac 0) | format | Render_Geometry_.c:866; Port_Geometry.c:5-11 | | - |
| normals | stored at 1/64 steps (s8) in the display-list cache: about 0.9° worst case | port | gx_fifo.c:300-311 | | proposal |
| skinning | 1 bone per vertex, weight 1.0; **8 bones** per level; 10 GX matrices | format / engine | Port_Geometry.c:48; Render_Geometry_.c:498-503; gx_internal.h:10 | | - |
| TEV / textures per material | 16 TEV stages, 8 texture maps, 8 texgens | engine (GX) | gx_draw.c:29; gx_internal.h:60-72; gx_tevgen.c:45 | | - |
| collision | the car's collider box (or hull) only: mesh polygons don't matter | engine | CarA.c:290-318, 142-195 | | - |
| game heap | **128 MB** (`HWVX_ARENA_MB` 8-1024) | port | port_internal.h:16; main_sdl.c `Port_ArenaSize` | peak **16.3 MB** (8-player battle) | switch added |
| running out of heap | was: a silent quit to the title, or models missing; now a fatal error naming the size | port | MemoryManager.c `Allocate` | never | yes |
| ARAM (samples) | 64 MB emulated (16 MB on the GameCube; raised by the streamed-music work) | port | ax.c `ARAM_SIZE`; Audio_.c:46-47 | peak **5.6 MB** (challenge race, measured at 16 MB) | done on pc-data |
| ARAM blocks | **255** descriptors in the original: the tight one; now 1024 | engine → port | Audio_.c `AUDIO_MEMBLKS` | **185** in use in a challenge race | yes, 255 → 1024 |
| sample formats | PCM16 or PCM8, mono; ADPCM voices are silenced | port | ax.c:305-349; Voice_.c:146-157 | all PCM16 (PS2 ADPCM decoded at load) | - |
| sample rate / pitch | any rate; rate x pitch / 48 kHz between 0.0015 and 2.93 | engine | Voice_.c:455-461 | 8,000-44,100 Hz sounds; bank samples 1,116-220,942 Hz | - |
| voices | **64** shared by effects and music; the lowest priority is stolen | port + engine | ax.c:21; Voice.h:14, 119 | | kept: retail-identical stealing |
| songs | one MIDI track, 16 channels, 128 programs, 128 drum keys, 64 loop markers | format / engine | MIDI.c:4-20, 102-105 | 60 songs, all 1 track | - |
| instrument banks | no count limit besides 128 programs/drums and ARAM | format | MIDI.c:93-100 | 36 instruments at most | - |
| sound length | none in the engine; mods: 10 minutes per file (decoder cap) | port | audiodec.h `AUDIODEC_MAX_FRAMES` | 26.4 s longest | - |
| worlds | see docs/worlds/README.md "Budgets" and "Hard limits in the code" | | | | - |

## Textures

**The cap.**
* The game itself refused anything over 512 texels a side (`Render_Texture_.c:81`, `tex->texWidth
  > 0x200`): the slot got no texture object and drew untextured.
* GX_TexGet refused over 1024 (gx_tex.c:766, :879).
* No retail texture is over 256 x 256 on any release (11,673 GC, 10,970 PC, 11,759 PS2 textures;
  `tools/asset_stats.py`).

So both checks are now `PORT_MAX_TEX_SIZE` (2048, port.h), and the mip cap is
`PORT_MAX_TEX_LEVELS` (12). Retail goes through exactly the same code as before: nothing it
loads is near either value.

2048 is OpenGL ES 3.0's guaranteed size, so every renderer takes it. 4096 would need a
check against the device's maximum (SDL GPU and desktop GL have 8192 or more).

**Test:** `test_texlimits` (ctest) uses generated RGBA8 and CMPR images (no game data) with a
stub renderer:
* 256, 1024 and 2048 sides, square and 2048 x 512, all reach the renderer at their size;
* RGBA8 gets 1 level: the game drops an RGBA8 texture's mips, Render_Texture_.c:65;
* CMPR gets the levels the game asks for: 10 of 2048's 12;
* 640 x 480 is taken as it is;
* 4096 is refused by both checks.

**Formats a mod can use.** A mod brings no textures of its own yet: it recolours base textures
with recipes (§8.2). New textures go through the media build (`xdl_media.py build`, the same
size as the base: other sizes need the `.xdl` compiler) or a level's own world pack. Either
way, the formats are the release's:
* GameCube: CMPR or RGBA8;
* PC: DXT1, DXT5, RGB8 or RGBA8;
* PS2: PSMCT32 or PSMT8 with a CLUT.

**Memory.**
* A 2048² texture with its mips is about 22 MB as RGBA8 on the GPU, 2.8 MB as CMPR/DXT1. The
  pack's bytes sit in the game heap until the pack goes.
* `HWVX_TEX_BUDGET` (512 MB) only drops least-recently-used GPU copies, which are re-made on
  use. It never fails.
* There is no per-pack texture limit. The largest retail pack's textures: 7.2 MB (GC
  `HW_IPack/interface_6`).

## Geometry

* **Indices.** GameCube display lists index each attribute (position, normal or colour, uv)
  with 16 bits, so a level holds at most 65,536 different values of each.
  * One `GX_DRAW_TRIANGLES` command counts at most 65,535 vertices (21,845 triangles). The
    encoder (`tools/xdl_gltf.py`) put a whole group in one command and failed past that
    (`'H' format requires ...`). It now writes several commands into the group's list.
  * The PC's index lists use bit 15 to flag strips, so a level holds at most **32,767
    vertices** (`Port_Geometry.c:47`, the encoder's check).
  * The renderers take 16-bit indices (gx_gl.c:643, gx_sdlgpu.c:904).
  * **Test:** `tools/geom_limits_probe.py` puts a generated sphere in place of a car group,
    encodes it for each release and decodes it back. All counts are vertices.

    | release | encodes | refused |
    |---|---|---|
    | GC (before the split) | 9,940 | 11,475 |
    | GC (after the split) | 64,980 (129,238 triangles) | 69,938, with a clear error |
    | PC | 29,890 | 39,903 |

    Positions come back exactly. Not yet drawn in game.
* **Precision.** The game's geometry is f32 (POS/NRM F32, frac 0: Render_Geometry_.c:866-868;
  Port_Geometry.c:5-11). It has no fixed point, on any release. The f32 step:

  | distance from the model's origin | step |
  |---|---|
  | 1 unit | 1.2e-7 |
  | 100 units | 7.6e-6 |
  | 1,000 units | 6.1e-5 |
  | 10,000 units | 0.001 |

  A car is about 10 units long and a world about 10,000 across: no visible error for a large or
  a very fine model.
  * The port's display-list cache stores normals at 1/64 per component (gx_fifo.c:300-311):
    up to about 0.9° off. That is exact for S8 data, and visible only as faint banding on very
    smooth, shiny surfaces.
* **Skinning** is rigid:
  * one bone per vertex, weight 1;
  * at most **8 bones** per level (`MAX_BONES`, Port_Geometry.c:48; the PC table's counts,
    Render_Geometry_.c:498-503);
  * GX's 10 position matrices (gx_internal.h:10) bound a draw.
* **Materials.** 16 TEV stages, 8 texture maps and 8 texgens per draw (GX hardware:
  gx_draw.c:29, gx_internal.h:60-72, gx_tevgen.c:45). The shader caches are 4,096 TEV keys,
  1,024 pipelines and 64 samplers (port).
* **Mod pipeline.** A mod can't add geometry yet: glTF in a mod is only checked for game data.
  New geometry goes through `xdl_graph.py` source projects (the probe above uses their
  encoder).

## Collision

Car physics uses the car's collider: `CarAInfo/0/collider`, a box (type 0) or a convex hull
(type 1) (`CarA_Init`, CarA.c:290-318). Inertia comes from its corners (CarA.c:142-195), and
mass, restitution and health from its record (CarA.c:688, 715-729).

The render mesh's polygon count has no effect on handling, collisions or spawn placement.
Spawn placement uses the combined envelope of all cars' boxes, including added ones.

## Memory: the game heap

The heap is 128 MB (`PORT_ARENA_SIZE`; the GameCube had 24 MB). `HWVX_HEAPLOG=1` now logs each
MB the peak passes, as `heap 0: peak X of Y MB`.

| run (GameCube data, `HWVX_AUTOPILOT=1`) | peak |
|---|---|
| challenge race, ch3_l2 (the largest world pack) | 16.0 MB |
| mission (adventure 0) | 11.9 MB |
| battle, 5 cars | 15.3 MB |
| online battle, 8 players (`online8_test.sh`), all 33 car packs loaded | 16.3 MB |
| the same, all players in car 25 | 14.7 MB |

* **Headroom.** About 110 MB, minus fragmentation: a pack is one block.
* **What a pack may add.** A car or item pack loads as one allocation of its whole size. The
  largest retail pack is 10 MB (`HW_IPack/interface_7`). Packs adding up to about 100 MB on top
  of a full race still load.
* **Running out.** Measured with `HWVX_ARENA_MB=12` and `16`, a battle start.
  * Before this change, the game either went quietly back to nothing and quit with exit status 0,
    or kept going with models missing ("geometry: out of memory converting a pack").
  * Now `Allocate` stops with a clear error: *"The game ran out of memory: N bytes asked of its
    16 MB heap ... Turn off mods that add large packs, or raise HWVX_ARENA_MB."* (exit 134, the
    log has the heap's use).
  * `OSAllocFromHeap` logs the largest free block and the peak.
* **The arena is cheap to grow.** It is now `calloc`ed rather than allocated and `memset`, so
  only pages the game writes cost memory. The size (8-1024 MB) is room, not use.

## Audio

* **The mixer** (ax.c, port; the streamed-music work owns it):
  * plays mono PCM16 or PCM8 voices, panned into 48 kHz stereo;
  * resamples linearly at step = rate x pitch / 48,000 (16.16);
  * ADPCM voices are silenced (ax.c:346, a TODO). Every retail sample is PCM16 by the time it
    plays: the PS2's SPU-ADPCM is decoded at load (Port_PS2.c).
  * Loops: a loop start in the descriptor (bytes for PCM16), played from there to the sample's
    end (Voice_.c:161-170, ax.c:375-381). There is no loop end: a loop end in a source file cuts
    the sample there (below).
* **Rates.** Any rate in the descriptor; the game limits rate x pitch to 0.0015-2.93 x 48 kHz
  (Voice_.c:455-461).
  * Retail sounds: 8,000 Hz (91), 11,025 (1,258), 22,050 (3,328), 44,100 (46) on GC.
  * Bank samples run from 1,116 to 220,942 Hz at their base note: the game plays those notes
    lower.
* **ARAM.** 64 MB emulated on pc-data (the GameCube's 16 MB, raised by the streamed-music
  work; ax.c `ARAM_SIZE`). The game sizes its heap from it (Audio_.c:46-47). `HWVX_HEAPLOG=1`
  now logs it as `aram: peak X of Y MB, N of 1024 blocks`. Measured at 16 MB:

  | run | peak | blocks in use |
  |---|---|---|
  | challenge ch3_l2 | 5.61 MB | 185 |
  | battle | 4.91 MB | 181 |
  | mission | 2.86 MB | 69 |

  * **The binding limit was the descriptor table**, `g_memblk[255]` (Audio_.c). One block per
    sound and bank, so a mod adding about 70 sounds to a race ran out ("Insufficient ARAM",
    the sound silent) with 10 MB of ARAM free.
  * Raised to 1024 (`AUDIO_MEMBLKS`). The free list hands blocks out in the same order, so the
    first 255 are exactly as before.
* **Voices.** 64: `AX_MAX_VOICES` (ax.c:21) and `NUM_VOICES` (Voice.h:14). Effects and music
  share them, and the lowest priority is stolen.
* **Songs.**
  * One MIDI track (the parser reads `MTrk` at 0x1A: MIDI.c:4-20).
  * 16 channels, 128 programs, 128 drum keys, 64 loop markers (CC 0x6E) (MIDI.c:102-105).
  * Polyphony comes out of the 64 voices.
  * Retail: 60 songs, every one type-0 with one track, up to 302 KB.
* **Banks.** `count` instruments, each with a key range, ADSR and 8/16 bits. Retail has at most
  36 instruments and 1.98 MB (`HW_music/music_3`). The only limits are 128 programs/drums and
  ARAM.

## Raising the rest: proposals

| limit | what it would take | risks | for new content only? | effort |
|---|---|---|---|---|
| ADPCM voices | decode GC DSP-ADPCM in ax.c (`PORT_TODO`) | none: no retail data reaches it (the port decodes to PCM16 first) | yes | half a day; not needed |
| normals 1/64 → full precision | store `s16` normals at 1/16384 in the compact vertex (gx_fifo.c:300-311, `GXVertex.nrm`), or f32 | vertex size grows (memory, upload bandwidth); demo frames change by rounding | the display-list cache is shared | half a day plus an A/B |
| PC 32,767 vertices per level | a new PC level layout (u32 indices, strips by record) or splitting a level's groups into several vertex arrays | a new pack format: `.xdl` only | yes (`.xdl`) | days |
| GC 65,536 values per attribute | 16-bit indices are GX's (`GX_INDEX16`); direct (unindexed) attributes would lift it | bigger display lists | yes | a day (encoder + port check) |
| 8 bones per level, 1 weight | `MAX_BONES`, the LOD count table, `numBones = 8` (Render_Geometry_.c:502), `GX_NUM_POSMTX`, and the shaders' matrix arrays; weights need a new vertex format and a skinning shader | the game code reads the 8-entry table on every release | `.xdl` only | days |
| textures 4096 | `PORT_MAX_TEX_SIZE` 4096 with a check against the renderer's max (GL `GL_MAX_TEXTURE_SIZE`, SDL GPU's limits), downscaling past it | memory: 88 MB RGBA8 each | yes | an hour |
| arena default 128 → 256 MB | `PORT_ARENA_SIZE` | none (lazy pages now), but no need: retail peaks at 16 MB | - | 1 line |
| 512 MB texture budget | `HWVX_TEX_BUDGET` exists | low-end GPUs: the budget is a cache, not a limit | - | - |

## Compressed audio input (Part B, as built)

**What a mod can do now.** `mod.jsonc` has a `"sounds"` member (schemas/Mod.schema.json):

```jsonc
"sounds": {
  "base://HW_Cars/car_7/sound/0": "sound/engine.ogg",
  "base://HW_Cars/car_7/sound/1": { "file": "sound/idle.mp3", "loop": 1200 }
}
```

* The file is the modder's own WAV, FLAC, Ogg Vorbis or MP3, in the mod. `sound/gc:3` names one
  release's sound only.
* The port decodes it as the pack loads (src/audiodec, linked into the game), mixes it to mono
  and resamples it to the base sound's rate (windowed sinc).
* It then replaces the pack's sound, through the path a mixed build uses (`PortMix_SFXSet`,
  Port_Mods.c "sounds").
* Where the loop comes from:
  1. the file's own tag: a WAV `smpl` chunk, or `LOOPSTART` with `LOOPLENGTH` or `LOOPEND` as
     Vorbis comments (Ogg, FLAC) or ID3v2 TXXX frames (MP3), in the file's samples, scaled to
     the rate;
  2. else `"loop"`;
  3. else, if the base sound loops, the whole sound.

  The game loops from the loop start to the sample's end, so a loop end cuts the sample there.
* A pack with mod sounds skips the `.xdl` cache.
* Two mods replacing the same sound are reported, and the later one applies.
* `HWVX_MODS_LOG=1` logs each one: `mods: Tone test: base://HW_Cars/car_7/sound/0:
  sound/tone.ogg, 22050 samples at 11025 Hz, loop 0`.

**The decoder: no ffmpeg.** No pure-Python decoder for MP3 or Vorbis is available offline, so
src/audiodec uses the official files the streamed music vendors in src/third_party (README.md
there): miniaudio 0.11.26 for its dr_wav, dr_flac and dr_mp3 (MP3 gapless: lengths exact), and
stb_vorbis 1.23. `hwvx_audiodec` (CMake) is linked into the game, where stb_vorbis's
implementation is musicstream.c's, and built as the helper `hwvx-audiodec IN OUT.wav|- [--mono]
[--rate HZ]` (installed beside the game; its own stb_vorbis in src/audiodec/stb_vorbis_impl.c).

Loop tags are parsed by our own code, with bounds checks. The decoders are limited to 256 MB
files and 10 minutes, and decode only files a mod names.

**The tools.**
* `xdl_media.py build` takes `.flac/.ogg/.mp3/.wav` for a sound or bank sample (`file` /
  `sample`): any channels and rate, decoded with `hwvx-audiodec` (`HWVX_AUDIODEC`, or found in
  the build or beside the game), mixed to mono, resampled to the sidecar's `rate` (the same
  filter as the port: Python and C agree to the sample), and encoded as the pack needs (PCM16
  in its byte order, or SPU-ADPCM on PS2).
  * A loop tag in the file wins over the sidecar's `loop`.
  * A WAV at another rate or with two channels used to be refused. It is now converted, with a
    note.
* `xdl_mod.py` takes `.flac .ogg .mp3` (the installer, Port_Mods.c, too) and checks the
  **decoded** audio for game data:
  * the exact samples of each channel and of their mix, against every base sound, and their
    32-byte runs;
  * a spectral fingerprint per 16 ms frame (15 band-energy-slope bits, Haitsma-Kalker style, in
    seconds and hertz so the rate doesn't matter), voted by time offset. A game sound
    re-encoded lossily or resampled is refused as "sounds like the game's sound (base://...;
    13 of its 22 frames line up)".
    * Thresholds: 8 frames and 15% of the file's frames.
    * 120 generated sounds (noise, chirps, chords, square waves) against a whole GameCube index
      scored at most 2: no false positives.
    * The index gains `sfp` tables. An index made before them still works, with a note that
      sounds were checked by exact samples only.

**Tests.**
* `tools/test_xdl_media.py`, on each release:
  * a WAV at twice the sidecar's rate is resampled;
  * a stereo WAV is mixed down;
  * a non-sound file is refused;
  * a FLAC with a `LOOPSTART` tag becomes exactly its PCM, loop kept (PS2: ADPCM at 51 dB).
* `tools/test_xdl_mod.py`:
  * a game sound as FLAC, MP3 (96k), Ogg (44.1 kHz stereo) and resampled to 32 kHz is refused;
  * the modder's own FLAC, MP3 and Ogg over base sounds pack;
  * a missing file, a texture id and a path out of the mod are refused.
* Manually, a generated 440 Hz tone with `LOOPSTART=1000 LOOPLENGTH=20000`, decoded to
  32 kHz:
  * all four formats keep the loop (726-15,238);
  * MP3 keeps its length exactly;
  * Python and the C helper agree sample for sample.
* `tools/mod_sound_test.sh BUILD DATA OUT` runs the whole path in a race:
  * a generated 700 + 1050 Hz loop as Ogg Vorbis (ffmpeg only makes the test file), over car 7's
    two engine sounds;
  * packed (`.xmod`), installed with `hwvx --install-mod`, then raced;
  * the log shows both sounds decoded (22,050 samples at 11,025 Hz, loop 0);
  * `HWVX_AUDIODUMP` holds the 3:2 pair (350 + 525 Hz at idle pitch) 41 dB over the band. The
    same race without the mod gives 22 dB.
