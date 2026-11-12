# Viewer protocol (HWVX-SDL viewer mode, the data browser)

Where it belongs: a new `docs/viewer-protocol.md` (src/port/viewer.h already points there), with a
one-line pointer from docs/xdl-format/README.md §10.8 (the node names it uses are §10.8's) and
from HWVX-SDL/README.md's list of switches.

## What it is

`HWVX_VIEWER=<port>` runs HWVX-SDL as a preview and editing server instead of the game: no menus,
saves or world; the engine is up (file layer, pack loader, renderer) and Main_Loop runs
`Port_ViewerFrame` (src/port/viewer.c, src/port/viewer_graph.c). Off by default; with the switch
unset nothing of it runs (the loader's one hook, `Port_ViewerNotePack`, returns at once), so
retail behaviour is unchanged.

- It listens on **127.0.0.1 only**; `HWVX_VIEWER=0` takes any free port and logs
  `viewer: listening on 127.0.0.1:N` (clients read N from the log).
- `HWVX_VIEWER_SCHEMAS=dir`: the struct schemas (`schemas/*.schema.json`, the ones with
  `x-c-struct`); default `schemas/` next to the program, one folder up, or in the current folder.
- One client at a time: a new connection replaces the old one.
- Commands run on the game's thread between frames, so the pack data never changes under a frame.
- Clients: `tools/browser` (Rust/egui: the data browser; `src/client.rs` is a reusable client),
  `tools/viewer_client.py` (Python module and one-shot CLI).
- Mouse in the game window: left drag orbits, right (or shift+left) drag pans, wheel zooms.

Always run it with `HWVX_PREF_DIR` set to a scratch folder (both clients do).

## Framing

Each request and each reply is **one JSON object on one line** (UTF-8, `\n`-terminated).

Request: `{"seq": 7, "cmd": "get", ...arguments}`. `seq` is optional and any JSON value; the reply
echoes it. (It is not `id`: `id` is an argument, a pack's number.)

Reply: `{"seq": 7, "ok": true, ...results}` or `{"seq": 7, "ok": false, "error": "why"}`.
Replies come in request order, except `screenshot`'s, which comes after the next frame is drawn
(a request sent after it can be answered first). A line that isn't a JSON object is answered
`{"ok": false, "error": ...}` with no seq. Lines with an `"event"` key and no seq are reserved for
notifications (none are sent yet); clients should skip them.

## Names

- **pack**: `ARCHIVE/TYPE_ID` with ARCHIVE the pack file's last path component: `HW_Cars/car_5`,
  `bat7_l1/world_0`, `HW_share/share_1`. `base://` in front is accepted. A request without `pack`
  means the pack shown (the last loaded or selected).
- **node**: §10.8's ids within the pack: `CarAInfo/0`, `CarAInfo/0/engine`, `geometry/14`,
  `texture/3`, raw nodes `.../p1c_r2`. `base://ARCHIVE/PACK/` in front is accepted.
- **type** of a pack: `car interface item link world collision world_texture geometry share audio
  music` (or its number 0..10).
- **field**: a schema property name, then `/index` into arrays and `/member` into embedded
  structs: `maxRpm`, `viewOffset/1`, `cameras/0/fov`. Or `@HEX:TYPE`, a raw store at that offset
  in the node (TYPE: u8 s8 u16 s16 u32 s32 f32, default u32), for nodes without a schema.

## Commands

| cmd | arguments | reply |
|---|---|---|
| `hello` | | `protocol` (1), `app`, `data` (the data folder), `release` (gc/pc/ps2), `renderer`, `schemas` (names) |
| `packs` | | `packs`: every pack of every pack file under the data folder: `{archive, name, type, typeId, id, release, pack, loaded}`. `archive` is what `load` takes (`worlds_/mp_/battle_/bat7_l1`). A disc image can't be listed (extract it). |
| `load` | `archive`, `type`, `id` | the pack: `{pack, archive, type, id, release, size, numNodes, typed, geometries, textures, sounds}`. Loads it as the game would (its share packs first, see below), builds its node graph and shows it. Loading a pack that is loaded loads it again from the files: **the reload** (edits are dropped). A pack of the same type that was loaded goes first (the game's slots). World quadrants (`geometry` packs) load with their world, not alone. |
| `unload` | `pack` | |
| `loaded` | | `packs`: the loaded ones as `load` describes them, plus `shown` |
| `outline` | `pack` | the pack's description and `nodes`: `{id, name, kind, type?, parent?, offset?, size?}` in graph order. `kind`: `typed` (a schema covers it), `raw`, `geometry`, `texture`, `shader`, `trigger`. `parent` is the node whose path reaches it first; roots have none. |
| `schema` | `type` (a struct name) | `schema`: the whole schema file |
| `get` | `pack?`, `node` | `{id, kind, type?, offset, size, ...}`. `typed`: `fields`: `{name, ctype, offset, units?, description?, confidence?, link?, target?, value}` in struct order; values are numbers, arrays, objects (embedded structs), or for links node ids (`CarAInfo/0/engine`), `base://...` ids into share packs, `@hex` for a slot no node covers, or null. `geometry`: `levels, skinned, bboxMin, bboxMax, center, radius, lods: [{groups, triangles}]`. `texture`: `index, textureId`. Non-typed nodes also: `bytes` (hex, first 256) and `slots`: `{offset, kind, target}` (pointer/texture/sound slots in it). |
| `set` | `pack?`, `node`, `field`, `value` | `{node, field, value}`: the value as now stored (read back, so a u8 set to 300 shows what was kept). Scalars take numbers (bools as 0/1); arrays an array of the full length (or set one element by path); embedded structs an object of the members to change; links a node id of the same pack, `@hex`, or null. Pointer slots can only change through link fields; a texture slot only to a texture the pack's relocations allow. Errors say why (`no field X in CarAInfo`, `an array of 3 is needed`, `links to other packs can't be set in a loaded pack yet`). |
| `select` | `pack?`, `node` or null | `selected`: the node the preview is about: a geometry is drawn alone; anything else (or null) draws the pack: a car pack as the game assembles the car (skinned body, rigid parts), any other pack's geometries side by side in rows |
| `focus` | same | the same, and the camera frames it |
| `camera` | any of `yaw`, `pitch` (degrees), `distance`, `fov`, `target` [x,y,z], `spin` (bool) | `camera`: the whole camera after the change |
| `orbit` | any of `dyaw`, `dpitch` (degrees), `zoom` (a factor), `pan` [x, y] (in units of the distance) | `camera`; turns `spin` off unless given |
| `background` | `color` [r, g, b] | |
| `screenshot` | `path` (.bmp, or .png with SDL ≥ 3.4) | after the next frame: `{path, width, height, drawn}` (`drawn`: draw calls queued). One at a time. |
| `status` | | `frame`, `drawn`, `shown`, `selected`, `camera` |
| `quit` | | the game ends after the reply |

### Edits

`set` writes into the loaded pack in memory, the bytes the game reads, so the preview shows the
change on the next frame where the preview depends on the value. Nothing is written to files: a
`load` of the pack again restores the files' values. Values the engine copies or builds at load
(converted geometry, display lists, textures uploaded to the GPU, a car's derived physics) don't
follow an edit until something rebuilds them; for those, the editing tools change the source
(.xdl / JSON project) and `load` again.

### Share packs

The loader can't load a pack in the middle of another (its header is a static, the open file is
shared), and the game never asks it to: the share packs a car, item or world needs are loaded
before it (GameSpecificInterfaces.c). `load` does the same: it reads the pack's header (`refs`)
from the file and loads the share packs it names first (and theirs). They stay loaded for the
session; a `load` of a share pack itself doesn't reread it while they hold it.

## For the mission editor (task #21)

The protocol is meant to grow without breaking clients (new commands, new reply keys; `protocol`
goes up only for incompatible changes). What #21 needs and how it fits:

- **set a field**: `set` (any typed node, nested paths, raw `@HEX:TYPE`).
- **move an object**: a world object's placement is a typed node once its struct has a schema
  (world objects are raw nodes until then): `set` its matrix/position field; the preview redraws
  from the pack each frame.
- **reload a pack**: `load` with the same archive/type/id after the project was rebuilt.
- Planned: `events` (`{"event": "changed", "pack", "node", "field"}` when the in-game picking or
  gizmo changes something; `{"event": "picked", "node"}` for a click in the preview), `pick`
  (`x, y` in the window: the node under it), links into other packs in `set`, the world's
  quadrants in the preview, and `load` from an .xdl project path.

## Tests and tools

- `cargo test --manifest-path tools/browser/Cargo.toml` with `HWVX_DATA=<GameCube files>`
  (`HWVX_GAME`, default `build/hwvx`): starts the viewer, loads `HW_Cars` car 5, checks the
  outline, gets and sets fields (and that a reload restores them), orbits, screenshots, quits.
- `tools/viewer_client.py --build build --data DIR 'load archive=HW_Cars type=car id=5'
  'get node=CarAInfo/0' 'screenshot path=/abs/shot.bmp'`: one-off requests from the shell.
- `hwvx-browser --load HW_Cars:car:5 --select CarAInfo/0/engine --app-shot out.bmp`: the browser
  with a pack and node open, its window saved, then it quits (screenshots of the browser itself).

## Changes after the code review


For the superproject's docs/viewer-protocol.md; each item says where.

## What it is (add after "It listens on 127.0.0.1 only")

- **A token.** A connection's first request has to be `{"cmd": "hello", "token": T}` with T the
  viewer's token, or the viewer answers `{"ok": false, "error": ...}` and closes it. The token is
  `HWVX_VIEWER_TOKEN`; unset, the viewer makes one up and logs `viewer: token T`. A first line that
  looks like HTTP (`POST / HTTP/1.1`, any method, or `HTTP/` in it) is closed without an answer:
  a web page can reach 127.0.0.1 and send a body of JSON lines, and that is not a client.
  `tools/browser` and `viewer_client.py` make up a token when they start the game and pass it;
  connecting to a running viewer they take `--token T` (default `HWVX_VIEWER_TOKEN`).
  Still one client at a time: a new connection replaces the old one even before its hello
  (a page could knock the browser off, not drive the viewer).

## Framing (add)

A request line is at most 1 MB; a longer one is answered `{"ok": false}` and the connection closed.

## Commands table

- `hello`: takes `token` (required on the first request; later hellos may leave it out).
- `load`: a pack that isn't in the file's table of contents is an error **before** anything is
  unloaded: the pack shown (and a loaded copy of the one asked for) stays.
- `set`: add argument `force` (bool, default false). An object link takes a node of the
  schema's `x-link` target type (a target `void` or none: any node), at a 4-byte aligned offset;
  another type is refused (`CarAInfo/0 is a CarAInfo, not a CarAEngine (force: true ...)`), and a
  raw `@hex` target needs `force: true`. Raw `@HEX:TYPE` offsets are hex that fits in 32 bits and
  in the node; array indices are 0 .. n-1 (decimal, nothing else). A store that touches any
  byte of a pointer slot is refused (a u16 at +2 of one, an f64 over one).
- `background`: `color` is three numbers, each clamped to 0 .. 255; anything else is an error.
- `screenshot`: `path` ends in `.bmp` or `.png`; an existing file there is replaced only when it
  is a BMP or PNG (`... is there and isn't an image: not overwritten`).

## Tests and tools (add)

`tools/browser/tests/protocol.rs` also checks: HTTP-looking and token-less connections dropped,
wrapped/oversized `@hex` offsets and bad indices refused, pointer-slot overlap refused, link type
and force, a missing pack leaving the shown one loaded, background and screenshot path checks.
`hwvx-browser --app-shot` exits 1 on an error reply, a lost viewer, or after 120 s.

## Embedded picture (frames through shared memory)


For the superproject's docs/viewer-protocol.md; each item says where.

### What it is (replace the "Mouse in the game window" bullet)

- **The window is hidden** in viewer mode: the game renders offscreen at the size the client asks
  for (`frames`) and shares each frame through shared memory; the browser draws it in its central
  panel. `HWVX_VIEWER_WINDOW=1` shows the game's own window as before (debugging; the browser's
  `--viewer-window`); there the mouse still orbits (left drag), pans (right / shift drag) and zooms
  (wheel). `HWVX_VIEWER_STATS=1` logs the readback's cost every second.
- Without `frames` the EFB is the (hidden) window's size, as before; `screenshot` saves the EFB at
  whatever size it has.

### Commands table (add)

| cmd | arguments | reply |
|---|---|---|
| `frames` | `width`, `height` (16x16 .. 3840x2160, physical pixels), or `on: false` | `on`, `name` (the shared memory), `key` (16 hex digits), `size` (its bytes), `slots` (3), `capacity` (bytes a slot), `headerSize` (4096), `width`, `height`. From the next frame on the EFB has that size (any shape: the 3D view widens or heightens as in "screen mapping", the camera's vertical field of view follows the panel). A size that fits the shared memory keeps it (same `name`); a bigger one gets a new one (new `name`, reopen it), with a quarter to spare. `on: false` stops and goes back to the window's size. |
| `input` | `events`: `[{type: "drag", button: "left"\|"right"\|"middle", dx, dy, pan?}, {type: "wheel", dy}, {type: "key", key}]`, `height?` | `camera`. In image pixels: a left drag orbits (the image's height is about 170 degrees), a right or middle drag (or `pan: true`) pans so the point under the pointer follows it; `wheel` `dy` notches (positive zooms in, x0.9 each). Keys: `f` frames what is drawn, `home` / `r` resets the view, `space` toggles spin, `left right up down` orbit 15/10 degrees, `plus minus` zoom. An unknown key or event type is an error (the others in the list still apply). |
| `pick` | `x`, `y` (image pixels), `select?` | `node` (the geometry node drawn there, or null), `pack`, `distance`. Basic: the ray against each drawn geometry's bounding sphere (as the last frame placed it), the smallest hit wins (a part over the body), nearest among equals. `select: true` also selects it. For #21: click-to-pick is `pick` then `select`/`get`; a finer pick (triangles, world objects) can replace it behind the same request. |
| `status` | | adds `width`, `height` (the EFB's), `headless`, `frames: {on, width, height, published, fps, ms, msCopy}` (`ms`: the readback's average cost a frame on the game's thread, `msCopy`: a published frame's copy into the shared memory) |

### Embedded frames (new section, after "Share packs")

The shared memory is POSIX `shm_open` (`name` like `/hwvx-<16 hex>-<n>`, mode 0600) or a Windows
named file mapping (`Local\hwvx-...`), opened read-write by the client (it writes `consumed`). The
client checks `magic` and `key`, and on POSIX unlinks the name right after mapping it (so nothing
else opens it and it can't outlive a crash; the game unlinks it too when replacing or freeing it).

Layout, little-endian:

| offset | field |
|---|---|
| 0 | `magic` u32 0x46565748 ("HWVF"), `version` u32 1 |
| 8 | `key` u64 |
| 16 | `headerSize` u32 (4096: slot 0's pixels), `slots` u32 (3), `capacity` u32 (bytes a slot) |
| 28 | `published` u32: frames published so far |
| 32 | `latest` u32: the newest one's slot |
| 36 | `consumed` u32: **written by the client**: the `published` count of the frame it last took |
| 40 | `format` u32 (1: RGBX8, the 4th byte is not alpha) |
| 64 + 32 i | slot i: `seq` (even: complete, odd: being written), `frame` (the viewer's frame number), `width`, `height`, `stride`, `published` (the count with it) |
| headerSize + i * capacity | slot i's pixels, top row first |

Reading (a seqlock): read `published`; if it is new, read `latest`, its slot's `seq` (odd: try
again), copy the pixels, read `seq` again (changed: try again), then store the slot's `published`
into `consumed`. The game writes a frame into the slot after `latest` and only grabs a new one while
at most one published frame is untaken, so with 3 slots the one a client is copying isn't
overwritten (the `seq` check covers the rest).

The readback doesn't stall the game: each frame's EFB is copied into a GPU readback buffer at its
end (SDL_GPU: a download submitted with a fence; OpenGL: a pixel buffer and a fence) and copied into
the shared memory at the end of the next frame, once the GPU is done (2 buffers; with both busy the
oldest is waited for). A client that stops taking frames stops the readbacks. Measured on an M-series
Mac (Metal), car 5, the browser taking every frame: 800x600 0.13 ms/frame, 1920x1080 0.26 ms,
3840x2160 0.79 ms, all at 60 published frames/s; input shows 1-3 frames later.

### tools/browser (Tests and tools: add)

- The central panel shows the picture as a texture (updated when a new frame arrives; a thread polls
  the shared memory and copies it out, off the UI thread). It asks for the panel's size in
  physical pixels (size x pixels_per_point, capped at 3840x2160 keeping the shape) 150 ms after the
  panel stops changing; meanwhile the last frame is stretched. A status line under it: fps, image
  size (and the size coming), the game's readback ms/frame and published frames/s. Drag orbits,
  right/middle/shift drag pans, the wheel zooms, a click picks (and selects the node in the
  outline), keys over the panel (F, Home, R, Space, arrows, +/-) go to `input`. The properties moved
  to a right-hand panel.
- `src/frames.rs`: `FrameReader` (open by name/key/size, `take` the newest new frame).
- `tests/frames.rs`: hello (with the token), `frames` at 400x300, 5 frames of that size, moving on
  and not blank; a still camera gives identical frames and a drag changes the picture; wheel zooms,
  right drag pans, a key, a bad key refused; `pick` at the centre finds a `geometry/N`, a corner
  nothing; a smaller size keeps the shared memory; the readback cost at 800x600, 1920x1080,
  3840x2160 (printed with `--nocapture`); over 3840 refused; `on: false`. `HWVX_RENDERER=opengl`
  runs it on OpenGL.
- `hwvx-browser --app-shot` now also waits for the picture at the panel's size and its stats.

## Media: textures, sounds, instruments, songs


For the superproject's docs/viewer-protocol.md (a new section "Media" after "Embedded frames", the
commands into the commands table, the tests under "Tests and tools"); a one-line pointer from
README §12.1 (tools/xdl_media.py: "the data browser shows the same media as the game decodes them,
docs/viewer-protocol.md Media").

### What it is (add)

- **Packs load with their sounds** in viewer mode: the viewer starts the sound manager
  (SoundManager_Init) as a state's start does. `HWVX_VIEWER_AUDIO=0` loads them without, as before.
- Nothing is decoded by the viewer itself (src/port/viewer_media.c): a texture is the texture
  object the game draws it with (m_gxTexObjArray, Render_CreateTextureSlot), decoded by the
  renderer's decoder (gx_tex.c `GX_PortDecodeTexObj`: the GameCube's tiled formats and CMPR as its
  texture unit mixes them, the PC's DXT/RGB(A), the PS2's palettised textures as PortPS2_Textures
  expanded them; a mod's colour recipe; the PC's colours squared, `Port_PCTexturesLookGC`); the
  levels are the ones the game samples. A sound is the sample in ARAM the mixer plays (after the
  loader's conversions: PCM16 to host order, the PS2's SPU-ADPCM decoded); a song is the MIDI
  player's copy (CreateMIDI) and its bank's samples.

### The media blob (new section)

Pixels and samples don't go over the socket: a second shared memory block (POSIX
`/hwvx-<key>-m<n>`, Windows `Local\hwvx-...`; opened and unlinked by the client as the frames' is).
A `texture` or `sound` reply has `blob: {name, key, size, serial, offset, length}`; reopen it when
`name` changes (it grows: a new one). Layout, little-endian:

| offset | field |
|---|---|
| 0 | `magic` u32 0x4D565748 ("HWVM"), `version` u32 1 |
| 8 | `key` u64 |
| 16 | `serial` u32: the reply's (0 while being written) |
| 20 | `length` u32, `dataOffset` u32 (64), `capacity` u32 |
| 64 | the payload |

The payload stays until the next `texture`/`sound` request rewrites it: check `serial` before and
after copying (src/media.rs `MediaReader`).

### Commands table (add)

| cmd | arguments | reply |
|---|---|---|
| `textures` | `pack?`, `shared?` (default true) | `pack`, `textures`: `{id, pack, index, textureId, drawn, format, gxFormat, width, height, levels, bytes, storedLevels, source, palette, flags: {wrapS, wrapT, linear, mipmaps, squared, mod}, usedBy, shared?}`. `id` is `texture/N`, or for the share packs' textures the pack uses (its texture relocations with a pack id) `base://ARCHIVE/share_N/texture/M`. `format`: GX names (`CMPR`, `RGBA8`, ...) or `PC_DXT1/PC_DXT5/PC_RGB8/PC_RGBA8` (the PS2's become PC_RGBA8); `source`: the release's stored format (`PSMT8` with `palette: {clut, colours}` on the PS2); `levels`: the ones the game samples (`storedLevels`: the header's). `usedBy`: the nodes whose texture slots name it (a geometry group's: its geometry). `drawn: false`: no texture object (past 512 texels). |
| `texture` | `node` (`texture/N`, or a `base://` id), `pack?`, `level?` | the listing's keys, `pixels: "rgba8"`, `levelData`: `[{level, width, height, offset, bytes}]` (every level, or the one asked for) in the blob, `alpha`: `opaque` / `cutout` / `blended` (level 0), `blob` |
| `sounds` | `pack?` | `pack`, `release`, `audio`, `sounds`: `{id: "sound/N", index, loaded, soundId, format (stored: pcm16be, pcm16le, pcm8, spu-adpcm), rate, length (samples), seconds, loop (sample, or null), bits, plays, volume}`; `song`: null or `{id: "song", kind: midi/it, division, tempo (us per quarter, the first), bpm, tempoChanges, ticks, events, notes, markers, channels, tracks (channels used: one SMF track), playing, tick, instruments}`; `instruments`: `{id: "instrument/N", drum, program, lo, hi, format, rate, length, loop, envelope: {attack, decay, sustain, release}}`. Only the pack whose song the MIDI player holds has one (one song at a time: loading a pack with a song drops the one held). |
| `sound` | `node` (`sound/N` or `instrument/N`), `pack?` | `rate, length, loop, bits, format, pcm: "s16"` (mono, little-endian) and `blob` |
| `play` | `node` (`sound/N`, `instrument/N`, `song` or `base://.../song`), `pack?`, `note?` (instrument: a MIDI note, GetNoteRate_), `mix?` (default false: what plays stops first) | `playing` (the base:// id), `voice` (sounds), `looping`, `device` (false: no output device, nothing is heard: the browser plays it itself). A sound plays as the game plays a sample (GetVoice, SetVoiceState: its volume, rate, loop); an instrument on a voice set up as a note on does; a song through SetMusicState and the sequencer. |
| `stop` | | `voices` (stopped), `song` (bool) |
| `status` | | adds `audio: {on, device, voices, song, tick?, last?}` |
| `hello` | | adds `media: 1` |

### For the mission editor and later edits (add to "For the mission editor")

- Media ids are node ids: `texture/N`, `sound/N`, `instrument/N`, `song`, with `base://` for
  another pack's; a texture link field's value (`texture/3`, `base://HW_share/share_1/texture/5`)
  is what `texture` takes, and `usedBy` goes the other way.
- Planned, behind the same names: `replace` `{node: "texture/N", level?}` with the RGBA in a blob
  the client writes (a `upload` request returns a writable blob of the size asked; the viewer
  encodes to the pack's format with the game's rules, rebuilds the texture object and the GPU
  copy), `{node: "sound/N"}` with PCM16 the same way (CreateSFX again). Like `set`, in memory only;
  files change through xdl_media.py `build` and a `load`.

### Tests and tools (add)

- `tools/browser/tests/media.rs` (`--test-threads=1`: one game at a time): HW_Cars car 5's
  textures and sounds against `tools/xdl_media.py extract` of the same pack, on the GameCube
  (`HWVX_DATA`) and the PC/PS2 (`HWVX_DATA_PC`, `HWVX_DATA_PS2`). Results: GameCube RGBA8, PS2
  (PSMT8 through the palette) and sounds/bank samples on all three: identical. GameCube CMPR:
  alpha identical, colours within 11 (the GameCube's 5/8-3/8 mixing and bit-repeat widening vs.
  xdl_media's DXT1 1/3-2/3 and x*255/31); transparent texels' hidden colour differs (average vs.
  black). PC DXT1/DXT5: within 4 (Apple BC rounding vs. // 3), after squaring xdl_media's colours
  as the game draws the PC's (also the PS2's) with the GameCube's look. Play/stop acknowledged,
  the voice counted in `status`, and heard: the mixed output (`HWVX_AUDIODUMP`) has the sound;
  the song's tick advances, stop stops it; an instrument at a note.
- `hwvx-browser --load HW_Cars:car:5 --select texture/0 --app-shot t.bmp` (or `sound/0`,
  `instrument/0`, `song`): the browser with that item open.

### tools/browser (add)

- The outline has Textures (the pack's, then each share pack's it uses), Sounds, and Song with its
  Instruments; each row says format, size/levels or rate/length/loop. A texture node, a texture
  link (in fields, slots, or a geometry's "Textures" list in the properties) and a "Used by" link
  jump between them.
- The central panel has tabs: 3D view / the open item. Texture: level, RGBA over a checkerboard /
  RGB / alpha alone, linear or nearest, wheel zoom around the pointer, drag pan, Fit, 1:1, the texel
  under the pointer (position, RGBA, hex), Export PNG (the level as decoded). Sound: waveform (min
  and max per pixel column; wheel zooms, drag pans, double-click shows all), the loop start marked,
  a playhead, Play (the game's mixer), Play here (the system's player: afplay, aplay, PowerShell;
  also when `play` says `device: false`), Stop, Export WAV; instruments can play at a note. Song:
  tempo, ticks, channels, Play/Stop and its position.
