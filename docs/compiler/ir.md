# Intermediate representation

The IR is intentionally tiny: everything the frontends produce and the
backends consume is a **resolved pack** — finished bytes plus a TOC key.
That keeps the code-generating milestones (M2+) from ever touching the
linker or the backends.

```rust
ArchivePlan {
    output: String,            // file name without extension
    target_name: String,
    platform: Platform,        // from the target, never the archive
    container: Container,      // xarchive | xnc
    packs: Vec<ResolvedPack>,  // storage order = report order
}

ResolvedPack {
    kind: PackKind,            // Cars | World | Geometry | Interface | Generic(u32)
    type_id: u32, id: u32, sub_id: u32,   // the TOC key
    version: u32,
    data: Vec<u8>,             // complete pack, TARGET byte order
    source: SourceId,          // { kind: "base"|"hwvx_pack"|"raw", detail }
    output: Option<String>,    // per-pack name override (xnc TOC)
}
```

Defined in `src/compiler/ir.rs`; `link.rs` and `backend.rs` only ever
see these types.

## Lowering (project → plans)

`lower_archive(project_dir, decl, target)` runs per archive × target:

1. **Base.** Read `<base.from>/manifest.json` (`E0041` on trouble). Its
   platform must equal the target platform — vendor packs are reused
   verbatim, so a PC base can only serve a PC target until SwapMap
   lowering (M3) learns to translate. Each non-excluded entry is
   rebuilt to bytes exactly like `hwvx-x pack` does: `Raw` entries are
   read as-is, `Split` folders go through `pack::join(platform, …)`
   (`E0031`/`E0041`). Exclusions are matched by
   `exclude_matches(pattern, entry)` — stem, `type`, `type:id`,
   `type:id/qN[_gM]` (see
   [project-format.md](project-format.md#base)).
2. **Explicit packs.** Each `PackSource` resolves through
   `source_pack()`: `hwvx_pack` folders are joined for the target
   platform with keys defaulted from the folder stem (`parse_stem`);
   `raw` blobs are read verbatim; future kinds emit `E0040` naming the
   milestone.
3. An archive that resolves to zero packs is an error (`E0031`) rather
   than a silently empty file.

Then `link::check_plan` flags duplicate `(type, id, sub_id)` keys
(`E0030`) — the game's loader takes the first TOC match, so a duplicate
would shadow a pack instead of replacing it.

## Keys and stems

- `key_string(type, id, sub)` renders canonical keys:
  `car:3`, `world:0`, `geometry:0:q2_g17` (geometry shows the quad and
  flavor split out of `sub_id = g<<16 | q`).
- `parse_stem("03_geometry_0_q2_g17")` recovers `(geometry, 0,
  g17<<16|q2)` from the folder names `hwvx-x extract` writes; the
  leading index (`03_`) is ignored. Unknown shapes return `None`, and
  the source must then carry explicit `type`/`id` or fail with `E0032`.

## Where later milestones plug in

Every future source kind ends its life as one or more `ResolvedPack`s:

| Milestone | Source | What it emits |
| --- | --- | --- |
| M2 | `car`, `items` | `ResolvedPack` for cars/interface, plus a `PendingRef` so the linker can resolve car→geometry cross-references before bytes are final. |
| M3 | base + target platform ≠ manifest platform | SwapMap/carwalk translation between the base steps of lowering — backends unchanged. |
| M4 | `world`, `geometry`, `audio` | world/quad-geometry packs; glTF → geometry via the existing `gltf`/`render` code. |

The linker grows reference checks (car → geometry id, world → quad
chains) once there are generated packs to validate; vendor-first plans
skip it because the vendor archive is already internally consistent.
