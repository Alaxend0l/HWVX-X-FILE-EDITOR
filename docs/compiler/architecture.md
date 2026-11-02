# Architecture

```text
                     hwvx.project.json
                            │
              frontend       ▼        src/compiler/project.rs
        serde parse + semantic validation (E00xx/E002x/E003x)
                            │
             lowering        ▼        src/compiler/ir.rs
   base folders ──► join() per pack   (vendor packs, project packs, raw)
                            │
              linker         ▼        src/compiler/link.rs
        duplicate (type,id,sub_id) key checks (E0030)
                            │
             backends        ▼        src/compiler/backend.rs
        xarchive (Archive::build)  |  xnc (XncFile::build)
                            │
                            ▼
        build/<output>.<xpc|xgc|xps|xnc>  +  BuildReport (JSON)
```

## Modules

| Module | Responsibility |
| --- | --- |
| `compiler/project.rs` | The project file data model (`Project`, `ArchiveDecl`, `PackSource`…), loading (`hwvx.project.json`), semantic validation. `deny_unknown_fields` everywhere: a typo is an error, not a surprise. |
| `compiler/target.rs` | Targets: named `(platform, container)` pairs, `--target` resolution with `name:container` overrides. |
| `compiler/ir.rs` | Lowering. Resolves every pack to bytes in **target byte order** plus a TOC key `(type, id, sub_id)`. Reuses `pack::join` so a split folder always rebuilds exactly like `hwvx-x pack` would. |
| `compiler/link.rs` | Whole-archive checks (duplicate keys today; reference validation for codegen packs later). |
| `compiler/backend.rs` | Emit. `xarchive` reuses `archive::Archive::build` (the exact code `pack` uses); `xnc` uses `xnc::XncFile::build`. |
| `compiler/diag.rs` | `Diagnostic { severity, code, message, location, hint }` and the stable code table. |
| `compiler/mod.rs` | The driver (`build_project`) and the serializable `BuildReport`. |
| `xnc.rs` | The XNC container reader/writer (spec in [custom-format.md](custom-format.md)). |

## Design rules

1. **Reuse, never fork.** The compiler does not re-implement pack
   joining or archive layouting; it calls the same `pack::join` /
   `Archive::build` the `extract`/`pack` roundtrip is verified against
   (`tests/orig_roundtrip.rs`). A byte-level bug fixed for `pack` is
   fixed for `build` for free.
2. **Diagnostics are contract.** Every user-facing failure has a stable
   code (see [project-format.md](project-format.md#diagnostics)); codes
   are never renumbered, only added to. Rendering is friendly text for
   humans, JSON for machines (`build --json` embeds the same items).
3. **Exit codes:** `0` built, `1` valid project but build failed,
   `2` invalid project / unknown target / usage. Scripts can branch on
   "my fault vs their fault".
4. **No new dependencies.** The whole stack stands on the existing
   `anyhow` + `serde` + `clap` set; the XNC format reserves compression
   method ids but does not link a compressor until a milestone needs it.
5. **Vendor-first.** Everything the M1 compiler writes comes from
   extracted vendor packs, project split folders, or raw blobs, so the
   output is game-valid by construction. Codegen lowers *into* the same
   `ResolvedPack` IR in later milestones; backends never learn what a
   "car" is.
6. **One archive x one target = one plan.** The build matrix is
   `archives × targets`. The archive's platform comes from the target,
   never from the archive declaration — this is what makes a single
   project emit PC/GC/PS2 sets identically.

## Data flow types

```rust
Project ──lower_archive()──► ArchivePlan { output, platform, container,
                              packs: Vec<ResolvedPack> }
ResolvedPack { kind, type_id, id, sub_id, version,
               data: Vec<u8>,          // complete pack, target byte order
               source: SourceId }
ArchivePlan ──link::check_plan──► backend ──► bytes on disk + ArchiveReport
```

`ResolvedPack` is deliberately dumb: metadata + finished bytes. Any
future source (car JSON, glTF geometry, SwapMap-ported vendor packs)
ends as a `ResolvedPack`, so linker and backends stay untouched.
