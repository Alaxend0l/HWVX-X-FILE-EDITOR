# Compiler roadmap

The compiler is deliberately staged: each milestone ships something
runnable and keeps `E0040` on everything beyond its scope, so projects
that name a future source kind fail with a message pointing here.

## M0 — Scaffold (done)

- Pipeline: project → IR plans → linker → backends
  ([architecture.md](architecture.md)).
- Diagnostics with stable codes and exit codes 0/1/2.
- `hwvx-x build`, `hwvx-x schema project`, JSON build reports.
- JSON Schema at `schemas/project.schema.json`.

## M1 — Vendor-first builds (done, this is what ships today)

Build real archives from what the game already contains:

- `base`: reuse an extracted vendor archive (`hwvx-x extract` output),
  minus anything `exclude` matches.
- `hwvx_pack`: re-insert split-pack folders (edited in this editor, or
  extracted from another archive).
- `raw`: insert pre-built pack bytes under an explicit key.
- Both containers: classic `xarchive` and the new
  [XNC](custom-format.md) (`--target name:xnc`).

Everything an "expression pack" or "total conversion" workflow needs to
reshuffle stock content is in place. What is *not* in place: generating
pack contents from authored sources, and re-targeting vendor packs to
another platform (`E0041`).

## M2 — Car and items codegen

Turn authored data into car and item packs:

- Define a lossless JSON model for car and items packs (schema-first,
  like the project format) by extending the existing
  `src/car.rs`/`src/items.rs` parsers into round-trip codecs.
- Source kinds `car` and `items` emit `ResolvedPack`s.
- The linker gains its first real job: car → geometry cross-references
  are checked (and patched) after all packs in an archive resolve.

Unlocks: editing a car's stats/textures list in JSON and rebuilding
`files.xpc` without ever touching pack bytes.

## M3 — Cross-platform lowering

Reuse one authored project across PC/GC/PS2 targets:

- Port the SwapMap/carwalk knowledge (offset/pointer relocation tables
  between platform versions of the same pack) into a lowering pass
  inside `lower_archive`'s base step, so a PC-sourced pack can join a GC
  plan; `E0041` becomes a warning with a translation report.
- Requires a diff corpus: extract identical packs from two platforms and
  machine-check the known maps before trusting them.

Unlocks: `hwvx-x build . --target pc --target gc` from one project with
a single vendor base.

## M4 — Worlds, geometry, audio

- Source kinds `world`, `geometry`, `interface`, `audio`.
- Geometry authored as glTF (the editor already imports/exports it in
  `src/gltf.rs`/`src/render.rs`) compiled to world quads/flavors; the
  `sub_id = g<<16 | q` convention becomes a build-time detail.
- World packs emitted from a graph description of tracks (checkpoints,
  quad chains, spawns).
- Audio: container-in, container-out for `.wav`/streaming formats, plus
  loudness normalization as an opt-in pass.

Unlocks: new tracks and cars that were never in the game.

## M5 — XNC v1: compression

- `method = 1` (zstd) for the compressible pack families; entries stay
  individually method-tagged, so mixed raw/compressed archives are fine
  and readers of untouched entries never change.
- First new crate dependency of the compiler (`zstd`), gated behind a
  build feature so the tool stays buildable without it.
- `hwvx-x unpack` to convert `.xnc` back to extracted vendor folders
  (today `parse` covers the payload-agnostic view).

## Standing rules

- A milestone is "done" when an end-to-end test builds a real archive
  from a checked-in project under `examples/`.
- Diagnostics codes never renumber; new kinds flip from `E0040` to real
  behavior, never to a different code.
- Each milestone updates this file and
  [project-format.md](project-format.md) together.
