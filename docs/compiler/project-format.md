# Project format — `hwvx.project.json`

A project is one JSON file plus the folders it references. Every archive
in `archives` is built once per selected target; the **target** selects
the platform and the output container. Unknown JSON fields are rejected
(`E0001`), so a typo fails loudly instead of being ignored.

The JSON Schema for this file is embedded in the binary:

```sh
hwvx-x schema project > hwvx.project.schema.json
```

and kept at `schemas/project.schema.json`. Editor tooling can point at
it with `"$schema"`.

## Complete example

```json
{
  "$schema": "./hwvx.project.schema.json",
  "schema_version": 1,
  "name": "demo-merge",
  "description": "PC files minus world 2, plus a custom interface pack.",
  "default_platform": "pc",
  "container": "xarchive",
  "targets": [
    { "name": "pc" },
    { "name": "pc-next", "container": "xnc" },
    { "name": "gc", "platform": "gc" }
  ],
  "default_targets": ["pc", "gc"],
  "archives": [
    {
      "output": "files",
      "base": {
        "from": "vendor/pc",
        "exclude": ["world:2", "car:41", "03_geometry_0_q2_g17"]
      },
      "packs": [
        { "kind": "hwvx_pack", "path": "mywork/interface" },
        { "kind": "raw", "path": "blobs/patch.bin", "type": "car", "id": 41 }
      ]
    }
  ]
}
```

## Top-level fields

| Field | Required | Meaning |
| --- | --- | --- |
| `$schema` | no | Pointer for editors; ignored by the compiler. |
| `schema_version` | yes | Must be `1` (`E0002` otherwise). |
| `name` | yes | Project name (reported by `--json`; `W0001` if blank). |
| `description` | no | Free text. |
| `default_platform` | no | Platform for targets that do not name one: `"pc"`, `"gc"`, `"ps2"`, `"ps2-beta"`. |
| `container` | no | Container for targets that do not name one: `"xarchive"` (default) or `"xnc"`. |
| `targets` | no* | Named build targets (below). At least one is needed to build (`E0011`). |
| `default_targets` | no | Names built when `--target` is absent; default is *all* targets. Unknown name → `E0010`. |
| `archives` | yes | The archives to produce (below). |

## Targets

```json
{ "name": "gc", "platform": "gc", "container": "xarchive" }
```

- `name` — unique (`E0011` for duplicates), used by `--target` and
  `default_targets`.
- `platform` — overrides `default_platform`; missing on every target
  **and** project → error.
- `container` — overrides the project `container`.

`hwvx-x build . --target pc:xnc` builds the `pc` target once more,
writing `.xnc` instead of `.xpc` — useful for emitting both containers
without declaring a second target.

## Archives

```json
{ "output": "files", "base": { ... }, "packs": [ ... ] }
```

- `output` — file name **without extension**; the target appends
  `.xpc` / `.xgc` / `.xps` / `.xnc` per platform and container
  (`E0020` for empty names, path separators, or names carrying an
  extension).
- `base` — optional: reuse a previously extracted archive folder.
- `packs` — explicit packs, appended after the base packs.

### `base`

```json
{ "from": "vendor/pc", "exclude": ["world:2", "geometry:2/q17"] }
```

`from` is a folder containing a `manifest.json` (what `hwvx-x extract`
writes). Packs are reused in manifest order with their original
`version` and `sub_id`. The manifest's platform must equal the target
platform — cross-platform vendor reuse needs SwapMap lowering and is
planned for milestone **M3** (`E0041` meanwhile).

`exclude` entries match base packs by any of these spellings:

| Spelling | Matches |
| --- | --- |
| `01_geometry_0_q2_g17` | the pack with that exact folder stem |
| `interface` | every pack of that type |
| `car:3` | pack id 3 of type car (any sub_id) |
| `geometry:2/q17` | that quad, any geometry flavor |
| `geometry:2/q17_g5` | exactly that quad + flavor |

### `packs` — source kinds

Tagged on `kind`.

**`hwvx_pack`** — a split-pack folder inside the project (a `pack.json`
plus section `.bin` files, exactly what `extract` produces per pack).

```json
{ "kind": "hwvx_pack", "path": "mywork/interface" }
```

`type`, `id` and `sub_id` default to what the folder stem encodes
(`03_geometry_0_q2_g17` → geometry, 0, `g<<16|q`); override them or use
a plain folder name plus explicit `"type"`/`"id"` (`E0032` when neither
works). The folder is joined for the **target's** platform, so a pack
extracted from a PC archive can be built into a GC archive as long as
its bytes are already correct for the target (plain re-layout of the
sections, no translation of section contents yet).

**`raw`** — pre-built pack bytes inserted verbatim, explicit key:

```json
{ "kind": "raw", "path": "blobs/patch.bin", "type": "car", "id": 41 }
```

`sub_id` defaults to 0, `version` to the release version (183). `type`
accepts names (`"car"`, `"world"`, …) or a numeric id.

**Future kinds** (`car`, `items`, `world`, `geometry`, `interface`,
`audio`) parse, appear in the schema, and fail with `E0040` naming the
milestone that implements them ([roadmap.md](roadmap.md)).

Common optional fields: `version`, and `output` (per-pack name override
used by the `.xnc` TOC; ignored by `xarchive`).

## Diagnostics

Stable codes — never renumbered, only added to.

| Code | Meaning |
| --- | --- |
| `E0001` | Project file unreadable or invalid JSON (incl. unknown fields). |
| `E0002` | Unsupported `schema_version`. |
| `E0010` | `--target` / `default_targets` names an unknown target. |
| `E0011` | Duplicate target name, no targets declared, or a target with no resolvable platform. |
| `E0020` | Invalid `output` name (empty, path separators, carries an extension). |
| `E0030` | Two packs in one archive would share a `(type, id, sub_id)` key. |
| `E0031` | A referenced file or split-pack folder does not exist. |
| `E0032` | No way to determine a pack's type/id (stem not a key, no explicit `type`/`id`). |
| `E0033` | Output file exists and `--force` was not given. |
| `E0040` | Source kind reserved for a future milestone (see the message). |
| `E0041` | Base folder missing, invalid manifest, platform mismatch, or a pack that fails to join. |
| `E0090` | Reading/writing files failed outside the sources above. |
| `W0001` | Project `name` is empty. |

Exit codes: `0` success · `1` valid project, build failed · `2` invalid
project or usage (parse, schema version, unknown target).
