# tools/xdlrs: the editor's tools in Rust

## The core (stage 1)

`tools/xdlrs` is a library crate (and a small CLI) that the editor (tools/browser) links, so the
release editor needs no Python. Stage 1 ports `xdl_struct.py` and `xdl_tac.py`; the level
compiler and the mod check (stages 2 and 3) build on it. The Python files stay: they are the
reference, and the research tools import them.

* Same input, same output: struct layouts from schemas/, f32 text (the shortest text that reads
  back as the same f32, `"0"` and `"-0.0"` for zeros, no `.0` on whole numbers), `json.dumps`
  rules (ensure_ascii, separators, `repr(float)`), JSONC as `jsonc_loads` reads it (comments,
  trailing commas, an unterminated string or comment is an error), byte order per release,
  `struct.pack`'s ranges and messages.
* A decoded f32 is held as the JSON value its text reads back as (an integer for `"7400"`),
  which is what the Python tools work on once a file is saved; packed back it is the same f32.
* Errors say which Python exception they are (`Kind::Xdl`, `Tac`, `Json`, `Struct`, `Graph`,
  and `Uncaught` where the Python stops with a traceback), so callers catch what the Python
  catches.
* The browser's JSONC code (`jsonc_edit`, the lenient reader) moved into the crate.
* Validation is `MiniValidator` (the Python's fallback): with the `jsonschema` package installed
  the Python tools check more (anyOf, enum, oneOf ...).

```
xdlrs struct extract|build|roundtrip|observe ...     as xdl_struct.py
xdlrs tac survey SURVEY_ROOT                         as xdl_tac.py SURVEY_ROOT
python3 tools/xdlrs_parity.py [--quick]              the port against the Python, byte for byte
```

The parity test runs both on every schema, the level and mod samples, every car object of the
GC, PC and PS2 surveys (round trip, extract, build with good and bad edits, observe), every
world pack's World record (decode, and the symbolic encoder xdl_graph uses), every retail TAC
parameter list, the samples' triggers with edits, and `xdl_tac.Extract` over xdl_graph's
extraction of every world pack.

### 10.8.y Stage 2: the level tools in Rust (for §10.6.7 / §10.8)

`xdlrs::level` (tools/xdl_level.py), `xdlrs::level_pack` (tools/xdl_level_pack.py) and what they
need of xdl_graph (`xdlrs::graph`: Extractor and Builder), xdl_gltf (`geom`: scan and the decode's
extents, no glTF), bsp_build/world_lib (`bsp`: a box volume's BSPN0) and the Port_Swap walkers
(`walk`: comp_survey.label_pack). The editor (tools/browser) calls them as a library: checks and
the live preview, test drives (in memory and as a world pack of the level's own) and Save as
mod run no Python. hwvx-x stays a subprocess (extracting the base archive, packing a level's
world pack). Save as mod's no-game-data check is `level_share::check_and_pack` over
`modcheck` (stage 3).

```
xdlrs level compile|check|base|mod|testdrive PROJECT ...   as xdl_level.py (same options)
xdlrs level pack PROJECT OUT_DIR --data DATA                as xdl_level_pack.py build
xdlrs graph extract PACK_DIR PROJECT | build PROJECT OUT [--check PACK_DIR]
python3 tools/xdlrs_parity.py --only graph,bsp,level --data GC_DATA
```

* Same output as the Python: level files (but `sourceHash`), errors and warnings (but where the
  text is a Python exception's or a JSON parser's), test drive JSON, mod folders, world packs
  byte for byte. The base project is xdl_graph.py's file for file, without the glTF (the level
  tools never re-encode geometry; a project with glTF is refused by the Rust Builder).
* Cache keys hash this crate's sources (build.rs, XDLRS_SRC_HASH) and schemas/ where the Python
  hashes its .py files: the two keep separate base projects, level files and mod indexes.
* The Builder validates with the `jsonschema` crate (Draft 2020-12), as the Python does with the
  jsonschema package.
* At run time the crate needs schemas/ (Env::new(root)); the pairing table
  (docs/xdl-format/pairing) only names PC/PS2 textures and geometry in base projects (without it
  they are `<release>:<index>` ids, consistent within one project).

## The mod check (modcheck)

#### 8.2.4 The mod check in Rust (tools/xdlrs, `modcheck`)

The editor packs mods without Python: `tools/xdlrs/src/modcheck` is `tools/xdl_mod.py`'s
`index`, `check` and `pack`, with the parts of `xdl_media.py` (archives, pack sections, texture
levels, sounds, songs and banks, structural ids) and `xdl_gltf.py` (vertex positions, the PS2's
through `ps2_geom.py`'s packet reader) they use. The Python stays the reference.

* Same decisions: the same refusals in the same order (place, reason, the base:// id or recipe
  to use instead), notes, printed text and exit codes (0 passed, 1 refused, 2 error); `pack`
  writes the same `.xmod` bytes (Python's zipfile layout: stored, sorted, fixed dates).
* Sounds decode through the port's own decoders: `build.rs` compiles `src/audiodec` (dr_wav,
  dr_flac, dr_mp3, stb_vorbis) into the crate, so a mod's sound is checked as the samples the
  game plays; a PCM16 WAV is read as Python's `wave` reads it. The quiet-run rule (QUIET = 256,
  `_loud_windows`) and the fingerprint (`sound_fp`, rustfft) are kept.
* PNGs read as Pillow reads them (every colour type and bit depth, tRNS compared on its low byte
  as Pillow does, palettes shorter than their indices); the gradient hash resizes as Pillow's BOX
  filter does on a float image, and numpy's float32 std decides "flat".
* The index is the same file: numpy's `.npz` with the same arrays (`b32`, `b256`, `whole`, `px`,
  `rgb`, `snd`, `vtx` with their `_src`, `dh`, `dh_src`, `obj`, `sfp`, `sfp_frame`, `sfp_src`,
  `meta`), written without zip64 and read with or without it: an index made by either is used by
  the other, and the parity test compares them array for array.
* `mod.jsonc` is checked as `xdl_mod.Manifest` checks it, member by member. (The Python doesn't
  run `jsonschema` over Mod.schema.json; nothing to match there.)

```
xdlrs mod index DATA... [-o OUT.npz] [--archives A,B] [--no-geometry] [-j N]
xdlrs mod check MOD --index INDEX [--strip-unchanged]
xdlrs mod pack MOD -o OUT.xmod --index INDEX [--strip-unchanged] [--allow-partial-index]
```

Library: `xdlrs::modcheck::check_and_pack(mod_dir, index, Some(out) | None, &Options)` ->
`Outcome { code, stdout, stderr, refused: [Refusal { place, why, use_instead }], notes, packed }`
(None: `check`), and `build_index(&[data dirs], out, &IndexOptions, repo_root)` -> the printed
summary.

Parity (`python3 tools/xdlrs_parity.py --only modindex,modfiles,modcheck`, or
`tools/xdlrs_parity_mod.py`): the whole GameCube index and the partial (HW_Cars, HW_IPack,
HW_Items) GC, PC and PS2 ones identical array for array, sound fingerprints included; PNGs of
every kind, WAV kinds, FLAC/MP3/Ogg (the game's sounds re-encoded, a remix) decoded to the same
samples and fingerprints; every case of `test_xdl_mod.py`, the samples and
mod_assets/monument_remix checked and packed to the same text, codes and bytes. Index of the
whole GameCube install: 26 s Python, 10 s Rust (4 threads each).

Where the Rust is stricter than the Python (inputs the Python would crash on or misread; the
Rust refuses the file as unreadable):

* a `.png` that is another format (Pillow opens a JPEG named .png and checks its pixels; the
  port doesn't read PNGs, the check only reads PNG);
* JSON with NaN / Infinity (Python's json accepts them);
* malformed tRNS chunks (Pillow ignores them, the png crate stops);
* a WAV whose chunk runs past the RIFF size, odd glTF values (a string as an index): the Python
  stops with a traceback.
