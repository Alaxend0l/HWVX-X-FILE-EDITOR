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
