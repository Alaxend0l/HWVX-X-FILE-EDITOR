# Schemas from the C structs

How the JSON Schemas of the `.xdl` source format ([`README.md`](README.md) §10.4) are made from
HWVX-SDL's headers, and how a pack struct becomes a JSONC file and back. Step 3b of the plan;
built on HWVX-SDL branch `schemas`.

| file (HWVX-SDL) | what |
|---|---|
| `tools/gen_schemas.py` | parses the headers, checks every struct's layout, writes `schemas/` |
| `schemas/<Struct>.schema.json` | one draft 2020-12 schema per pack-data struct, committed |
| `tools/schema_observed.json` | value ranges seen in the retail packs (numbers only), committed |
| `tools/xdl_struct.py` | `extract` / `build` / `roundtrip` / `observe`: pack struct ⇄ JSONC |
| `tools/test_xdl_struct.py` | edits and error cases on one car |

```
python3 tools/gen_schemas.py [--check]                    layouts checked, schemas/ written
python3 tools/xdl_struct.py roundtrip ../build-survey [--parts] [--out build-xdl/roundtrip]
python3 tools/xdl_struct.py observe ../build-survey       refresh x-observed, then gen_schemas again
```

## 1. Parsing and layout checks

A small parser for the headers' own conventions (`src/game/include/*.h` and `port/gx.h`):
typedef'd structs and unions, `PTR32(T)` / `FPTR32` handle fields, multi-dimensional arrays,
anonymous and named inline structs and unions, `__attribute__((packed, aligned))`, type macros
(`THING_SLOT(n)`), `#define` and enum constants in array sizes. It computes the GameCube layout
(4-byte pointers, natural alignment) and checks it against the header itself:

* every `/* 0xNN ... */` field comment (2,490 of them; on `f32 x, y, z; /* 0x00 */` only the
  first declarator);
* every `_Static_assert`, also through the assert macros (`AI_ASSERT`, `CARA_ASSERT_OFFSET`...).

A mismatch fails the run. Today: **318 of 321 structs lay out, 0 offset-comment mismatches,
701/701 asserts hold.** Not laid out: the three bitfield structs of `msl_file.h` (the C
library's `FILE`). `LGWheels.h` is C++ and isn't parsed. Neither is pack data.

## 2. Type mapping

| C | JSON Schema |
|---|---|
| `f32`, `f64` | `number` (written with the shortest text that gives back the same f32 bits: `0.3`, `7400`, `-0.0`) |
| `u8 s8 u16 s16 u32 s32` | `integer` with the type's `minimum`/`maximum` |
| `T x[n]`, `T x[n][m]` | `array`, `minItems` = `maxItems` = n, nested per dimension |
| embedded struct / union | `{"$ref": "#/$defs/Name"}`, the def in the same file |
| anonymous member | its fields flattened into the parent |
| union | the first member stands for the slot; the others in `x-union-alternatives` |
| `PTR32(T)` | link: `["string", "null"]` with the link grammar, `x-link: {kind: object, target: T}` |
| `u32` marked `@xdl: link=T` | the same (handles the header types as integers) |
| `@xdl: texture` / `@xdl: geometry` | a link restricted to `.../texture/N` or `.../geometry/N` |
| enum | `integer` (s32) |

Every property also carries `x-offset`, `x-c-type`, and, from [`../fields/README.md`](../fields/README.md),
`x-confidence` and `x-read` (`{gc, ps2, pc}`). The object carries `x-c-struct`, `x-header`,
`x-size` and `x-hidden`. These are the converter's whole contract: `xdl_struct.py` reads only
the schema, never the headers.

**Units and hints** come from `@xdl:` markers at the end of a field's header comment
(`/* 0x1C max rpm ... @xdl: units=rpm */`): `units=` → `x-units`, `hint=` → `x-hint`,
`link=T`, `texture`, `geometry`. Marked so far: CarAInfo and its parts (rpm, ms, rad/s, rpm/s,
semitones, %). `x-observed` gives min, max and the count of distinct values over the 33
HW_Cars cars on GameCube and PC.

## 3. The undecoded-fields rule

From the read columns of `docs/fields/README.md`:

* **read by no release** (`no / no / no`): not a property. Listed in `x-hidden`
  (offset, size, C name, note); the compiler takes those bytes from the `extends` base, or zeros
  without one. CarAInfo hides its 12 pads.
* **read, but unnamed** (confidence `-`, some `yes` or `?`): a typed property `unk_0xNN`, the
  C name in `x-c-name`, typed by the table's Type column when it is a scalar of the same size
  (CarAEngine `unk_0x0C`: f32, read by PS2 and PC). `?` counts as read: WeaponInfo's four pads
  (PC unscanned) stay visible.
* a struct without a table (the `$defs`): every field visible; `pad*`/`unk*` named by offset.

Struct padding and the gaps between fields come from the base too.

## 4. Files: `$schema`, `format_version`, `extends`

Each schema allows `$schema`, requires `format_version` (1), and allows `extends` (a link).
`additionalProperties` is false, so a hidden field or a misspelling is an error. With
`extends`, every field is optional (`if: {required: [extends]}`); without it, every field is
required. A nested object (`$defs`) has no `required` of its own.

## 5. Links

The grammar is [`pairing.md`](pairing.md) §1, plus **objects as a field path below a root
object**, in the same way as pairing.md's `.../geometry/12/level/0/group/3`:

```
base://HW_Cars/car_5/CarAInfo/0               root table entry 0 of the car pack
base://HW_Cars/car_5/CarAInfo/0/engine        what CarAInfo.engine points to
base://HW_Cars/car_5/CarAInfo/0/aiScripts/2   an element of a pointer array
base://HW_Cars/share_2/texture/4              texture: GameCube index, via the pairing table
base://HW_Cars/car_5/geometry/14              geometry: header-scan order
uid://<1-13 base-36 chars>[/field...]         the project's own objects
```

The path is structural, so it is the same on every release. In the data:

* an object link is a `pointers.bin` slot;
* a texture link is a `texture_relocs` entry: the high half is the share pack
  (1 HW_share, 2 HW_Cars/share_2, 3 HW_Items/share_3, 0 the pack itself) and the low half is the
  release's texture index, mapped to the GameCube id through `pairing/<archive>.jsonl`;
* a geometry link is a local `ref_relocs` entry whose low half is the geometry's index in the
  header scan (GameCube 0x2003, PC 0x0002). This holds in all 66 cars.

The converter checks that each slot's relocation kind matches the schema. A `u32` that turns
out to be a pointer slot fails with "the schema should make it a link". That is how the CarA.h
markers were found.

## 6. CarAInfo end to end

`xdl_struct.py roundtrip` extracts each car's CarAInfo to `car.jsonc`, reads the text back,
validates it (`jsonschema` Draft 2020-12, or a built-in validator for the keywords used), and
writes it over a **scrubbed base**. The scrubbed base is the same car's bytes with every
visible field zeroed and its relocations dropped, so each visible byte has to come from the
JSON. The output is compared with the pack: bytes, pointer slots, texture, sound and ref
relocations.

* **CarAInfo: 66/66 byte-identical** (33 cars × GameCube and PC).
* With `--parts` (CarAEngine, CarAAccel, CarAStuntInfo, CarAHud, CarACollider, each extracted
  by its field path): **396/396**.
* GameCube and PC give the same JSON for 196 of the 198 objects. The two that differ are real
  tuning differences: car_10's collider `inertiaScale` (0.7 / 0.5) and car_25's engine
  `cameras.count` (2 / 3).

```jsonc
// CarAInfo base://HW_Cars/car_5/CarAInfo/0 (gc), from tools/xdl_struct.py
{
  "$schema": "../../../../schemas/CarAInfo.schema.json",
  "format_version": 1,
  "extends": "base://HW_Cars/car_5/CarAInfo/0",
  "viewOffset": [0, 2.2, 0],
  "damagePercent": 75,  // %
  "skidLife": 600,  // 16 ms
  "maxRpm": 7400,  // rpm
  "skidMarkTexture": "base://HW_Cars/share_2/texture/4",
  "reflectionGeom": "base://HW_Cars/car_5/geometry/14",
  "engine": "base://HW_Cars/car_5/CarAInfo/0/engine",
  "numGears": 5,
  "shiftSounds": null,
  ...
}
```

The writer starts from the base's bytes and relocations, so a sparse file
(`{"format_version": 1, "extends": "...", "damagePercent": 50}`) changes one byte. A texture
slot whose link is unchanged keeps the base's raw bytes. The loader overwrites them, and the
stored value is a tool artifact: an offset on GameCube, `0x10xxxx` on PC. A changed texture
slot is written as 0. In this step, object and geometry links resolve only inside the object's
own pack. A link to another pack, a `uid://`, or a link to the wrong type (`engine` →
a CarAAccel) is refused.

## 7. Open questions

* **Object ids.** `<Struct>/<root index>/<field path>` is new here. It reads well and is the same
  on every release, but an object reachable by two paths gets two ids. The extractor uses
  the field it reads. It should settle on one canonical path once sub-objects get their own
  files.
* **Counts beside arrays** (`numGears` + `gears`) are kept as separate fields while the arrays
  are still links into the base. Once arrays are inline, the count should come from their length.
* **`CarAInfo.soundSets`** is typed `PTR32(PTR32(CarASoundSet))` with `numSoundSets = 5`, but
  the target is `{0, pointer, f32, 0, f32}`: only `[1]` (the one CarA.c reads) is a pointer. The
  header type needs a look.
* **`?` treated as read.** This keeps WeaponInfo's pads visible until the PC scan covers them.
  It might be better to hide fields the GameCube (the port) never reads.
* **Unions**: the first member is the JSON name (`box`, not `spheres`). A marker for picking
  the member would help where the alternatives differ in shape (CarASlaveSet).
* **Markers in comments** end up in the fields table's Notes when it is regenerated.
  `gen_doc.py` should strip `@xdl: ...`.
