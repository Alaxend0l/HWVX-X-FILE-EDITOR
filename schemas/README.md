# Schemas

JSON Schemas for files this toolchain reads. They are kept in sync with
the Rust types by hand (both are small); `cargo test` parses each one as
valid JSON, and the project schema is embedded in the binary.

| File | Describes | Rust type |
| --- | --- | --- |
| `project.schema.json` | `hwvx.project.json` build projects | `src/compiler/project.rs` |

Dump the embedded copy to check for drift or to point an editor at it:

```sh
hwvx-x schema project > schemas/project.schema.json   # git diff should be empty
```

Conventions:

- Draft 2020-12, `additionalProperties: false` wherever the compiler
  uses `deny_unknown_fields`, so editor validation matches build-time
  `E0001` behavior.
- `$defs` hold reused enums (`platform`, `container`, `packType`) and
  object shapes; `$id` is a stable `hwvx:` URN, not a URL.
- Reserved-for-future-milestones variants (`interface`, `car`, `items`,
  `world`, `geometry`, `audio` pack sources) stay in the schema and fail
  at build time with `E0040` — the schema describes the language, the
  compiler describes what is implemented.

The extracted-pack side of the toolchain (`pack.json` / `manifest.json`)
is intentionally not schema'd yet: its shape is still changing with the
car/items codecs (milestone M2), while the project format is frozen at
`schema_version: 1`.
