# Compiler

The **compiler** turns a declarative project (a `hwvx.project.json` plus a
folder of sources) into ready-to-play Velocity X archives:
`.xpc` / `.xgc` / `.xps` files for each configured target, or `.xnc`
files in the next container this tool defines.

It is the natural layer above the existing `extract` / `pack` tools:

- `extract` turns one shipped archive into editable files;
- `pack` turns one extracted folder back into one archive;
- **`build`** takes *many* sources — vendor-extracted packs, project-own
  pack folders, raw blobs — and composes any number of archives for any
  number of platforms and containers in one command.

## Quick start

```sh
# 1. Extract the vendor archives once (base material)
hwvx-x extract orig/pc/files vendor/pc

# 2. Describe what you want
$EDITOR hwvx.project.json      # see examples/demo-project/

# 3. Build everything
hwvx-x build . --force
hwvx-x build . --target pc --target gc --force
hwvx-x build . --json          # machine-readable report
```

Exit codes: `0` success, `1` build failed (inputs valid, build error),
`2` invalid project or bad usage.

## Documentation map

| Document | What it covers |
| --- | --- |
| [architecture.md](architecture.md) | Pipeline, crates layout, design rules |
| [project-format.md](project-format.md) | `hwvx.project.json` reference + diagnostic codes |
| [ir.md](ir.md) | The intermediate representation and lowering |
| [custom-format.md](custom-format.md) | The **XNC** next container specification |
| [roadmap.md](roadmap.md) | Milestones M1..M5 (codegen, cross-platform lowering) |

The JSON schemas live in [`/schemas`](../../schemas) and are embedded in
the binary (`hwvx-x schema project`).

## Scope today (M1: vendor-first)

The compiler assembles archives from material that already exists:

- **base** — a previously extracted archive folder, packs reused verbatim
  (or a subset, via `exclude`);
- **hwvx_pack** — a split-pack folder stored inside the project (this is
  where the existing `extract`/`pack` editor flow feeds the compiler);
- **raw** — any pre-built pack bytes with an explicit `type`/`id`.

Source kinds that require *generating* packs (`car`, `world`, `geometry`,
`items`, `interface`, `audio`) are declared in the schema so projects can
already carry them, but the compiler stops with a clear `E0040` error
naming the roadmap milestone that will implement each one. Nothing fails
silently and nothing pretends to work.
