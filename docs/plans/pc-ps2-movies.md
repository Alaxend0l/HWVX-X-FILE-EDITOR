# Playing the PC and PS2 movies in HWVX-SDL

Status: research and plan, nothing built. Written 2026-11-20.

HWVX-SDL plays the GameCube's THP movies (`src/port/thp.c`, `src/port/thp_jpeg.c`) behind
the game's `Mpeg_Create/StartMovie/Heartbeat/Render/Destroy` interface. With a PC or PS2
folder, `Mpeg_StartMovie("FMV_\bglogo", ...)` finds no THP file, logs
`movie ...: not found or not a THP file`, and the game skips the movie. This plan covers
what it takes to play the other two releases' movies.

## 1. The formats

### GameCube (the reference, checked with ffprobe on `orig/gc/.../files/fmv_/`)

THP files, no extension: `thqlogo bglogo hwboot hw_intro hw_end`. 640x480, 29.97 fps,
MJPEG-like frames (4:2:0), GameCube DSP ADPCM audio, 48000 Hz stereo.

### PC: Bink 1

* Files: `FMV_/thqLogo.bik BGlogo.bik hwboot.bik hw_intro.bik hw_end.bik hw_gal.bik
  hw_mark.bik` (the list `src/port/pcdisc.c` copies off the CD into `<pref>/pc/FMV_/`).
  The PC has two movies the GameCube lacks (`hw_gal`, `hw_mark`).
* The exe imports `binkw32.dll` by ordinal (`HWVX-PC-Decomp/src/auto/text_0043B2A0.cpp`).
  It calls `BinkBuffer*` functions, which belong to Bink 1. Bink 2 ships as `bink2w32.dll`.
  `docs/pc-version/README.md` also says "video is Bink (`binkw32.dll`, `FMV_\*.bik`)".
* No PC CD or `.bik` file is on this machine, so the Bink revision (`BIKf`..`BIKi`), size,
  frame rate and audio track are **unverified**. Expected (from memory): about 640x480 at
  29.97 or 30 fps, with Bink Audio (DCT or RDFT). Check the first 4 bytes and header with
  `ffprobe` once a disc is imported.

### PS2: MPEG-2 program streams (`.PSS`)

Checked on the July 2002 prototype disc (`~/Downloads/Hot Wheels Velocity X Early Code 16th
July 2002.iso`, files `XPS/ENGLISH/{THQLOGO,BGLOGO,M1INTRO,HWTRAIL}.PSS`; its `FMV_` folder is
empty). The retail layout is `fmv_/*.pss` (`docs/ps2-version/README.md` §5, same format).

* Container: MPEG-2 program stream, pack header `00 00 01 BA` with the MPEG-2 marker.
* Video: stream `0xE0`, MPEG-2 Main Profile @ Main Level, 720x480, SAR 8:9 (4:3),
  29.97 fps, 4:2:0, `progressive_sequence = 0`. ffprobe reports bottom field first; `idet`
  on THQLOGO finds about a third of the frames interlaced (BFF). THQLOGO: 252 frames, 8.4 s.
* Audio: private stream 1 (`0xBD`), sub-stream `0xA0`. The first packet starts with an
  `SShd` header: `type 1` (PCM s16 little-endian), `0xBB80` = 48000 Hz, 2 channels,
  interleave `0x200` bytes; then `SSbd` and its size. ffprobe does not see this track. No
  PS ADPCM is used in the files checked (a type-0 file would need the SPU ADPCM decoder,
  about 50 lines).
* The PS2 game plays them with Sony's libmpeg on the IPU: the prototype ELF names
  `MpegPlayback_.c`, `sceMpegDemuxPss`, `sceMpegGetPicture`, `_ipuVdec`, and the same
  `Mpeg_Create/Mpeg_Heartbeat/Mpeg_Destroy` names as the GameCube, and builds
  `cdrom0:\%s;1` with `.PSS` appended. (`HWVX-ALL-Decomp/HWVX-PS2-Decomp` is not checked out.)

## 2. Open-source decoders

The repo is MIT (`HWVX-SDL/LICENSE`). Builds: Windows, macOS, Linux, Android, Flatpak, and a
browser test build; SDL3 is the only big dependency, small single-file libraries live in
`src/third_party` (dr_wav, dr_flac, dr_mp3, stb_vorbis).

| Option | Formats | Licence | Fit |
|---|---|---|---|
| pl_mpeg | MPEG-1 video, MP2 audio, MPEG-PS demux | MIT | Single header, fits like dr_*. **Does not decode MPEG-2**: "MPEG2 Video ... is not supported by this library" ([repo](https://github.com/phoboslab/pl_mpeg)). Usable as the base for an MPEG-2 decoder. |
| FFmpeg libavcodec | `binkvideo`, `binkaudio_dct`, `binkaudio_rdft`, `mpeg2video`, `smackvid`, `adpcm_psx`; demuxers `bink`, `mpeg` (checked with the local ffmpeg 9.0.2 `-decoders`) | LGPL 2.1+ (GPL only with `--enable-gpl`) | The only open Bink 1 decoder that stands alone from an engine. FFmpeg's checklist asks for dynamic linking, the exact source offered next to the binaries, and credits ([legal](https://ffmpeg.org/legal.html)). Heavy to build per platform; a `--disable-everything --enable-decoder=...` build keeps it small (size **unverified**, likely 1-3 MB). |
| libmpeg2 | MPEG-1, MPEG-2 MP@ML | GPL v2+ | Last release 0.5.1, 2008 ([site](https://libmpeg2.sourceforge.io/)). Linking it makes the shipped binaries GPL. Avoid. |
| ScummVM `video/bink_decoder.cpp` | Bink 1 | GPL v3+, based on FFmpeg's | Tied to ScummVM's classes ([source](https://sources.debian.org/src/scummvm/2.7.0%2Bdfsg-1/video/bink_decoder.cpp)). Avoid. |
| RAD Bink SDK | Bink | proprietary ([RAD](https://www.radgametools.com/binksdk.htm)) | The PC's `binkw32.dll` is 32-bit Windows only. Not usable. |

Not found: any MIT/BSD MPEG-2 or Bink 1 decoder. The MultimediaWiki Bink page is incomplete
(its DCT and residue sections are "TODO") ([wiki](https://wiki.multimedia.cx/index.php/Bink_Video)),
so a clean-room Bink decoder from documents alone is not realistic; writing one by reading
FFmpeg's `bink.c` makes it a derived work under the LGPL.

Patents: MPEG-2 video patents have expired everywhere but Malaysia and the Philippines
(Wikipedia, [libmpeg2](https://en.wikipedia.org/wiki/Libmpeg2); **unverified** beyond that).
The MPEG-2 spec (ITU-T H.262) is paid ([ITU](https://www.itu.int/rec/T-REC-H.262)); the
MPEG-1 parts pl_mpeg already has cover most of it, and FFmpeg's `mpeg12dec.c` can be used as a
behaviour reference by running it, not by copying it.

### What MPEG-2 adds over pl_mpeg's MPEG-1 (from memory, check against H.262)

Sequence and picture coding extensions (f_code per direction, intra DC precision, picture
structure, top_field_first, frame_pred_frame_dct, q_scale_type, intra_vlc_format,
alternate_scan), quant matrix extension, the non-linear quantiser table, VLC table B-15,
field and frame motion vectors and field DCT in frame pictures, field pictures, MPEG-2
mismatch control. Main Profile allows dual-prime; check whether these files use it. All in
4:2:0 at 720x480. A first count: 600-1000 lines on top of pl_mpeg's video decoder.

## 3. Convert at import, or decode at runtime

* **Import-time conversion to THP.** The launcher would still need both decoders to read the
  source, plus a baseline JPEG encoder (stb_image_write, public domain/MIT; THP's frames carry
  no `0xFF 00` stuffing, which `thp_jpeg.c` relies on, so stuffing must be undone) and a DSP
  ADPCM encoder. Files grow a lot (the GC's `hw_intro` is 162 MB for 2:17) and 720x480 is
  scaled or kept. It saves no decoder work, so it is not worth it.
* **Conversion with the user's own ffmpeg** (desktop only): `ffmpeg -i X.bik -c:v
  mpeg1video -c:a mp2 X.mpg`, then pl_mpeg plays `.mpg`. FFmpeg has no THP muxer or encoder
  (none listed by the local ffmpeg), so the target is MPEG-1, not THP. No licence question
  for the repo (nothing linked), but it needs a manual step and doesn't help Android,
  Flatpak's sandbox or the browser build. Good as a stopgap and as a test oracle.
* **Runtime decoding** behind the existing `Mpeg_*` interface: one player per format, chosen
  by what `Port_OpenDiscFile` finds (`FMV_/name` THP, `FMV_/name.bik`, `fmv_/name.pss`).
  The frame clock, fixed-step mode, audio stream, volume and `GX_PortDrawImage` (4:3 safe
  area) in `thp.c` all carry over. This is the recommended route.

Legal/practical: no game data, movies, or converted movies in the repo or in CI; tests run on
the developer's own discs (as the THP tests do). Converted files, if any, live in the user's
save folder next to the imported data.

## 4. Recommendation

1. **PS2: write the decoder in-house, MIT.** Vendor pl_mpeg into `src/third_party`, extend
   its video decoder to MPEG-2 MP@ML, and write a small PSS demuxer for the `0xBD`/`0xA0`
   PCM track (pl_mpeg's demuxer already parses packs and PES headers). Weave the two fields
   of interlaced frames as they come; add a simple line blend only if combing shows at the
   window's scale. No licence change, works on every platform including the browser build.
2. **PC: optional FFmpeg (LGPL), loaded at runtime.** Bink 1 has no permissive decoder.
   `SDL_LoadObject` libavcodec/libavformat/libavutil if present (`avformat_open_input`,
   `avcodec_send_packet`, `avcodec_receive_frame`, `sws` not needed: convert YUV to RGBA by
   hand as `thp_jpeg.c` does). Release packages ship a minimal shared FFmpeg built with only
   the Bink decoders and demuxer, with its source and configure line next to the downloads;
   Flatpak can take FFmpeg from its runtime instead (the freedesktop runtime's FFmpeg
   extension: **unverified** whether it includes Bink). Without the library, the movie is
   skipped as today. A user can still convert the `.bik` files with their own ffmpeg (stage 2
   below). If FFmpeg is acceptable for Bink anyway, it could also decode the PS2's MPEG-2,
   but then PS2 movies depend on an optional library; keep PS2 on the in-house path.

## 5. Staged plan (each stage about one agent-day)

Testing in every stage: boot without `HWVX_START` (it skips the intro movies) with
`--data <pc or ps2 folder>`; `HWVX_DUMP=dir` saves every 60th presented frame (movies present
through `GX_DrawCopyDisp`, which calls `DumpEFB`, `gx_draw.c`); add
`HWVX_DETERMINISTIC` so dump N is movie frame `N*60*fps/60`, comparable with
`ffmpeg -i X -vf "select=eq(n\,K)" -frames:v 1 ref.png`. The THP path is the reference for
timing, the log line (`movie ...: WxH, N frames at F fps, audio C ch R Hz`), volume and skip.
A small `movietest` tool (like `xdltest`) decodes the first frames of a file to BMP for PSNR
against ffmpeg's output, run locally only (no data in CI).

1. **Player split.** Move the format-independent parts of `thp.c` (clock, fixed step, audio
   stream and gain, read-ahead, RGBA buffer, `Mpeg_*` entry points, movies-off) behind a small
   per-format interface (open, read frame, decode picture). THP stays the only format.
   Test: the GameCube intro plays as before; dumps match the pre-change dumps byte for byte.
2. **MPEG-1 via pl_mpeg, and the conversion stopgap.** Vendor pl_mpeg (MIT); play
   `FMV_/name.mpg` if present (BT.601 YUV to RGBA, MP2 audio). Add `tools/movie_convert.py`
   that runs the user's ffmpeg over an imported `pc/FMV_` or `ps2/fmv_` folder.
   Test: converted `thqLogo.bik`/`THQLOGO.PSS` play in the PC and PS2 folders.
3. **PSS demux and PCM audio.** Find `fmv_/name.pss` (and `.PSS`, the prototype's root
   names); route `0xE0` to the video decoder and `0xBD`/`0xA0` PCM (`SShd`/`SSbd`,
   `0x200`-byte L/R interleave: check by ear and against the length) to the audio stream.
   Test: the audio of THQLOGO.PSS plays in sync with a blank or MPEG-1-decoded picture.
4. **MPEG-2 frame pictures.** Extensions, B-15 table, non-linear quantiser, alternate scan,
   frame/field motion vectors, field DCT, mismatch control. Test: `movietest` PSNR against
   ffmpeg on THQLOGO and BGLOGO above 40 dB on every frame.
5. **MPEG-2 field pictures and the rest; PS2 in game.** Field pictures if any file uses them,
   dual-prime if used, interlace handling, 720x480 shown at 4:3. Test: all prototype `.PSS`
   (and retail `fmv_/*.pss` when a disc is at hand) through `movietest`; full intro in
   `--data ps2`, `HWVX_DUMP` frames against ffmpeg's; Android and browser builds start a movie.
6. **Bink through optional FFmpeg.** `SDL_LoadObject` loader, Bink demux/decode, YUV to
   RGBA, Bink Audio as float or s16 to the SDL stream, missing-library log and skip. Test:
   all seven `.bik` in `--data pc`; the ending (`hw_end`) and the two PC-only movies, with
   whatever plays them; renaming the library away skips movies cleanly.
7. **Packaging FFmpeg.** Minimal shared FFmpeg build in `packaging/` per desktop platform
   and Android, source tarball and configure line published with the release, credits in
   the README and launcher; Flatpak from the runtime if it has Bink. Test: a release build on
   each platform plays `thqLogo.bik`; licence files present.

Stages 1-5 give PS2 movies with no new licence; 6-7 give PC movies with an optional LGPL
library. Stages 2 and 6-7 are independent of 3-5.

## Stage 7 as built: packaging FFmpeg (HWVX-SDL branch `ffmpeg-packaging`)

`packaging/ffmpeg/build.sh TARGET WORKDIR PREFIX` (HWVX-SDL; details and the LGPL checklist
item by item in its `packaging/README.md`, "FFmpeg"). FFmpeg **9.0.2** (newest point release
of the newest branch; 9.1 appeared the day of this work), sha256
`8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`, signature checked
against FFmpeg's release key, unmodified. LGPL v2.1+ only: `--disable-everything
--disable-autodetect`, shared only, no programs; demuxers `bink,binka,mpegps,mpegvideo`
(mpegps needs the raw `mpegvideo` demuxer to probe its video stream's codec), decoders
`bink,binkaudio_dct,binkaudio_rdft,mpeg1video,mpeg2video,pcm_s16le,pcm_s16be,adpcm_psx`
(the Bink video decoder is configured as `bink`, not `binkvideo`), parser `mpegvideo`,
protocol `file`, `--disable-x86asm`. libavformat 63, libavcodec 63, libavutil 61 (bink.c's
range is 60-63).

| Package | Where | Added |
|---------|-------|-------|
| Windows `.zip` (was a bare `.exe`) | DLLs beside `hwvx.exe`, `licenses/FFmpeg/` | 3.8 MB unpacked, 1.6 MB zipped |
| Linux `.tar.gz` (was a bare program) | `.so.N` beside `hwvx` | 2.7 MB, 1.1 MB in the tar.gz |
| AppImage | `usr/lib` (run path `$ORIGIN/../lib`) | 1.0 MB |
| macOS `.app` | `Contents/Frameworks`, linked from `Contents/Resources` (SDL's base path) | not measured |
| Android | `lib/ABI/libav*.so` (unversioned), `assets/licenses/FFmpeg` | 6.0 MB (stored uncompressed) |
| Flatpak | the 24.08 runtime's own FFmpeg 7.1 (`libavformat.so.61`, LGPL, has the Bink and MPEG-2 decoders) | 0 |

Every package has `licenses/FFmpeg/` (COPYING.LGPLv2.1, FFmpeg's LICENSE.md, NOTICE.txt with
version, source, sha256, configure line); every release gets `ffmpeg-9.0.2.tar.xz` and its
`.asc` beside the packages and an FFmpeg paragraph at the top of its notes. swscale and
swresample are built and shipped but bink.c doesn't load them (about half the size).

Checked locally with the built libraries: `avcodec_license()` is "LGPL version 2.1 or
later", the configuration has no gpl/version3/nonfree; all seven PC `.bik` and five PS2
`.PSS` decode without an error (thqLogo.bik 241 video frames and 387840 samples at 48 kHz,
THQLOGO.PSS 242 frames 720x480; about 2000 frames/s on one core without assembly); the
game's lookup (folder + soname order on Linux, through the AppImage's run path, by full
path on Windows under Wine); the Windows cross build (Ubuntu 24.04 MinGW-w64: no
libgcc DLL after `-static-libgcc`), the Linux packages in an Ubuntu 22.04 container like CI,
both Android ABIs with NDK r28c (16 KB pages, `libavformat.so` soname). The 24.08 runtime's
FFmpeg was checked in the runtime (x86_64: loaded, LGPL, decoders present; aarch64: its
configure string). Not checked: macOS (no Mac here), the APK on a device, CI itself.

FFmpeg's `mpegps` demuxer does not see the PSS audio (`SShd` in private stream 1): the game
still reads that stream itself (type 1 is s16le PCM; `pcm_s16le`/`adpcm_psx` are in the
build if it wants FFmpeg to decode it). `org.freedesktop.Platform.codecs-extra` (25.08 on)
is GPL (`--enable-gpl`) and is added to the library path when installed, so a move of the
Flatpak to 25.08 should look at which FFmpeg the game then finds.

## Open questions

* The PC `.bik` details (revision, size, audio) once a CD is imported.
* Whether the retail PS2 files use dual-prime or field pictures (scan headers in stage 5).
* Which movie names the PC and PS2 games ask for (`hw_gal`, `hw_mark`, `M1INTRO`, `HWTRAIL`)
  and from which state; the GameCube asks only for its five.
