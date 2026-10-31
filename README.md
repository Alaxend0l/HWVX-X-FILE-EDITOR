# HWVX X-File Editor

`hwvx-x` is a command-line tool that unpacks and rebuilds the data archives of
*Hot Wheels: Velocity X*:

| Extension | Build | Byte order | Notes |
|-----------|-------|------------|-------|
| `.xpc` | PC | little | |
| `.xgc` | GameCube | big | |
| `.xps` | PS2 | little | packs and sections on 2048-byte sectors |
| `.xps` | PS2 prototype (v177/179) | little | different layout (see below) |

The platform is detected automatically. Pass `--platform` to override it.

## Build

```sh
cargo build --release      # binary: target/release/hwvx-x
```

## Usage

```sh
hwvx-x info orig/gc/files/HW_Cars.xgc          # what's inside (works on folders too)
hwvx-x extract orig/gc/files/HW_Cars.xgc out/HW_Cars
hwvx-x pack out/HW_Cars HW_Cars.xgc

hwvx-x extract orig/gc/files work/gc           # whole tree, mirrored
hwvx-x pack work/gc build/gc/files             # rebuild every extracted archive
hwvx-x verify orig/                            # check that extract + pack is byte-exact
```

`extract` refuses to write into a non-empty folder, and `pack` refuses to
overwrite an existing file. Pass `--force` to override either. With `--force`,
`extract` replaces only folders that contain an earlier extraction.
`--raw` keeps each pack as one `.bin` instead of splitting it.

## Extracted layout

```
HW_Cars/
  manifest.json            archive: platform, version, packs in order
  00_share_2/              <index>_<type>_<id>
    pack.json              header fields + references to shared packs
    data.bin               the pack's objects (pointers stored as offsets)
    pointers.bin           where the pointer slots in data.bin are
    texture_relocs.bin     (offset, texture index) pairs
    shader_relocs.bin      (offset, shader index) pairs
    sound_relocs.bin       (offset, sound index) pairs
    ref_relocs.bin         pointer slots into this or a shared pack
    sound_pointers.bin / sound_data.bin   sound descriptors, samples, songs
  01_geometry_0_q12_g5/    geometry: <id>_q<quad pack id>_g<GX object count>
```

Pack types: `car`, `interface`, `item`, `link`, `world`, `collision`,
`world_texture`, `geometry`, `share`, `audio`, `music`.

Section `.bin` files keep the platform's byte order. When packing:

- the counts and sizes in each pack header come from the section file sizes,
  so they are never edited by hand;
- the archive's table of contents (offsets and sizes) and all alignment are
  recomputed;
- packs can be reordered, removed, or added by editing `entries` in
  `manifest.json`. The `summary` and `sections` fields are informational only.

Many original files hold leftover bytes in their alignment padding. These are
saved as `padding`, `gap_fill`, and `tail_fill` hex strings so that unmodified
archives rebuild byte for byte. If a section changes size, the saved bytes are
cut or zero-padded to fit.

## Format notes

Sources: `HWVX-SDL/src/game/DataGamePackLoad.c` (`DataPack_LoadFile`,
`DataPack_LoadDataPack`, `DataPack_LoadQuadPack`),
`HWVX-SDL/src/game/FileStreaming.c` (`LoadQuadrants`), and
`Pac-Kart-Documentation/hwvx*.html`. See the module docs in `src/archive.rs`
and `src/pack.rs`.

- Archive header: `0x020100A0, 0x040100AE, version, count`, followed by
  24-byte TOC entries `version, type, id, sub_id, size, offset`.
- Retail offsets are relative to the end of the TOC. In the prototype, each
  offset is relative to the end of its own TOC entry.
- The game locates geometry packs only through the TOC. It rebuilds
  `fileOffset` from the TOC at load time, so packs can change size.
- The prototype puts `ref_relocs` and `pointers` before `data` and uses no
  padding. Geometry packs align `data` to 32 bytes on GameCube and to 16
  bytes elsewhere.

## Tests

`cargo test --release` runs the unit tests. If `orig/{pc,gc,ps2,ps2-beta}`
exists, it also round-trips every original archive (386 files).
