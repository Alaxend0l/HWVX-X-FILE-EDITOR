# XNC — X Velocity Next Container

`XNC` is a new pack-container format defined by this toolchain and
implemented in [`src/xnc.rs`](../../src/xnc.rs) (the normative
reference — this page and the code must stay in step). Targets with
`"container": "xnc"` write `.xnc` files; the classic game-readable
containers remain `xarchive` (`.xpc`/`.xgc`/`.xps`).

Why a new container instead of only `XArchive`:

- **Fixed, explicit layout** — always little-endian headers, versioned
  entries, no offset-relative-to-TOC quirks that differ between retail
  and prototype.
- **Integrity** — CRC32 on the header and per payload, so a half-copied
  file is detected at parse, not at load.
- **Metadata without arithmetic** — each entry carries `sub_id`, size
  and an optional name directly; nothing must be recomputed from a
  sidecar.
- **Room to grow** — a `method` byte reserves compression (ids 1/2 are
  spoken for: zstd, deflate) and 64-bit sizes, while v0 stays
  dependency-free raw storage.
- **Payload-compatible** — an entry's payload is a complete pack in the
  target's byte order, exactly as `XArchive` stores it, so unpacking an
  `.xnc` feeds the existing editor flow unchanged.

## Layout (version 0, all integers little-endian)

### Header — fixed, `0x14` bytes

| Offset | Size | Field | Notes |
| --- | --- | --- | --- |
| `0x00` | 4 | `magic` | `58 4E 43 1A` (`"XNC\x1A"`) |
| `0x04` | 1 | `version` | container version, `0` |
| `0x05` | 1 | `flags` | `0`; no bits defined yet |
| `0x06` | 2 | `reserved` | `0` |
| `0x08` | 4 | `count` | number of TOC entries |
| `0x0C` | 4 | `toc_len` | byte size of the whole TOC |
| `0x10` | 4 | `hcrc` | CRC32-MPEG-2 of the header with `hcrc` set to 0 |

### TOC — `count` entries, exactly `toc_len` bytes

Each entry is a fixed `0x30`-byte part plus its name:

| Offset | Size | Field | Notes |
| --- | --- | --- | --- |
| `0x00` | 4 | `version` | entry schema version, `0` |
| `0x04` | 4 | `type` | pack type id (same id space as `XArchive`) |
| `0x08` | 4 | `id` | pack id |
| `0x0C` | 4 | `sub_id` | quad/flavor; `u32::MAX` = absent |
| `0x10` | 8 | `data_size` | uncompressed payload size |
| `0x18` | 8 | `offset` | payload start, **relative to the data area** |
| `0x20` | 1 | `name_len` | bytes of name following the fixed part (≤255) |
| `0x21` | 1 | `method` | `0` = raw; `1` zstd and `2` deflate are reserved, unimplemented |
| `0x22` | 8 | `stored` | on-disk payload size (v0: `== data_size`) |
| `0x2A` | 4 | `dcrc` | CRC32-MPEG-2 of the stored payload |
| `0x2E` | 2 | `reserved` | `0` |
| `0x30` | n | `name` | ASCII only, e.g. `HW_CARS`; absent when `name_len == 0` |

### Data area

Starts at `align_up(0x14 + toc_len, 16)`; space between the TOC and the
data area is zero-filled. Each payload begins at `data_area + offset`
with `offset` chosen so every payload is **16-byte aligned**; gaps are
zero-filled. v0 stores payloads raw, in build order.

```text
0x00  header (0x14)
      TOC entries (toc_len)
      zero pad -> 16-byte boundary
D     payload 0            (offset 0)
      zero pad -> 16-byte boundary
D     payload 1            (offset …)
      …
```

## Rules (normative)

1. A file is an XNC container iff magic and version byte match
   (`is_xnc`). Writers must set all reserved fields to 0.
2. `hcrc` covers the 20 header bytes with `hcrc` itself zeroed.
   Parsers must verify it **before** trusting `count`/`toc_len`.
3. Entry names must be ASCII; `name_len == 0` means no name.
4. v0 parsers must reject `method != 0`, `stored != data_size`, and
   nonzero flags — fail loudly, never guess.
5. `dcrc` guards the *stored* bytes (post-compression, once methods
   exist; equal to the raw payload in v0).
6. Keys `(type, id, sub_id)` should be unique per container; the
   compiler enforces this before writing (`E0030`).
7. Offsets and sizes must not overflow the file; parsers bound-check
   every entry before copying.

## Checksum

CRC32-MPEG-2: polynomial `0x04C11DB7`, init `0xFFFFFFFF`, input and
output **not** reflected, no final XOR. Test vector:
`crc32_mpeg2(b"123456789") == 0x0376E6E7`.

## Versioning

- Minor evolution of entry semantics is allowed under the same
  container `version` only via fields already present (e.g. new `flags`
  bits that old readers may reject).
- A layout change (new header/entry fields, new mandatory method)
  bumps the header `version` byte; readers must refuse unknown versions
  rather than half-parse.
- Milestone **M5** introduces v1: `method = 1` (zstd) for compressible
  pack families. Files remain readable in the sense that v0 readers can
  report the reserved method clearly — the point of reserving ids now.

## Implementations

| | |
| --- | --- |
| Writer | `XncFile::build` — computes alignment, `stored`, both CRCs; streams header + TOC + data in one pass, no back-patching. |
| Reader | `XncFile::parse` — validates header CRC, bounds, methods, data CRCs; returns entries + payloads. |
| Tests | `src/xnc.rs` — CRC vectors, roundtrip (empty payload, unnamed, multi-entry), header/data tamper detection. |
| CLI use | `hwvx-x build . --target <name>:xnc` writes `.xnc` per archive. |
