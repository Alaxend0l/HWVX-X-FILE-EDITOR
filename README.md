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

### Car tools

| Command | What it does |
|---------|--------------|
| `hwvx-x textures <pack folder> <out>` | Dump a pack's textures as TGA (GameCube or PS2-prototype packs) |
| `hwvx-x port-car <prototype car pack> --into <GC HW_Cars folder> --id N` | Convert a PS2-prototype car into a GameCube car pack and add it as car `N` |
| `hwvx-x roster <GC HW_IPack folder> --name 'N=Name*' --cars <GC HW_Cars folder>` | Add cars to the car-select screens (range, names, rendered thumbnails) |
| `hwvx-x render-car <GC car pack> out.tga` | Textured preview render of a GameCube car |
| `hwvx-x export-gltf <GC pack or archive folder> [out]` | Export models to glTF binary (`.glb`) with embedded textures |

### Model export (glTF)

`hwvx-x export-gltf <GC pack or archive folder> [out]` writes GameCube models
as self-contained `.glb` files (glTF 2.0, textures embedded as PNG), which
Blender and most 3D tools can open.

```sh
hwvx-x extract orig/gc/files/HW_share.xgc work/HW_share     # shared textures (tyres etc.)
hwvx-x export-gltf work/HW_Cars/01_car_0 car0.glb --share work/HW_share
hwvx-x export-gltf work/HW_Cars cars_glb --share work/HW_share   # one .glb per pack
hwvx-x export-gltf work/w1_geo w1_glb --textures work/w1_tex --share work/HW_share
```

- Car packs are posed on their skeleton. Each bone becomes a node, and the
  rigid parts and breakable body parts hang off their bones. Skinned
  geometry becomes a glTF skin. `--damaged` adds the damaged body parts,
  `--scan` exports every geometry at the origin instead.
- Other packs (items, worlds, quad geometry packs) export every geometry as
  its own node at the origin. World geometry is already in world space.
  Quad packs take their textures from the world texture pack (`wN_tex`), so
  pass it with `--textures`.
- Shared textures are looked up by pack id in the input's own archive and in
  each `--share` folder. When a shared pack is missing, the command prints a
  warning and leaves those textures out.
- `--all-lods` adds the lower LODs as extra `..._lodN` nodes.
- Materials: lit groups become PBR materials. Unlit groups use
  `KHR_materials_unlit` with vertex colours. Translucent groups use `BLEND`,
  cut-out textures use `MASK`, and env-mapped chrome becomes a metallic
  material in the group's tint colour. The raw group flags, blend mode and
  colour are kept in each material's `extras`.

See [`docs/beta-cars/`](docs/beta-cars/README.md) for the full walkthrough
(adding Fandango, Sweet 16 II and Jester from the PS2 prototype) and the
format research behind it.

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
