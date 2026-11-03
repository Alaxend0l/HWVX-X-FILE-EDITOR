# Porting a PS2-prototype world level to the GameCube game

The leaked PS2 prototype contains a complete first chapter world —
**CH1_L2** — that differs from the shipped GameCube level: more/different
models, different collision, and prototype world textures. `hwvx-x
port-world` converts the prototype content and installs it into the
GameCube world archives so the shipped engine streams the prototype level.

The approach is **content replacement inside the retail host**: the
GameCube `ch1_l2.xgc` world pack stays as-is (its splines, props, triggers
and level logic keep working), and only the streaming content — quadrant
geometry, the world texture pack and the collision pack — is rebuilt from
the prototype and pointed at through the quadrant table.

* `PROGRESS.md` – the work log: what was found, what was decided, and why.
* The container format (XPS/XGC archives: header, table of contents, how
  `extract`/`pack` build, access and store them) is documented in the
  [root README](../../README.md#format-notes); this file documents the
  world-specific contract on top of it.

## Recipe

```sh
cargo build --release
B=target/release/hwvx-x

# 1. unpack the prototype level and the four GameCube world archives
$B extract orig/ps2-beta/XPS/ENGLISH/WORLDS_/SP_/CHAL1_/CH1_L2.XPS work/beta_ch1_l2
$B extract orig/gc/files/worlds_/geo_/w1_geo.xgc   work/w1_geo
$B extract orig/gc/files/worlds_/col_/w1_col.xgc   work/w1_col
$B extract orig/gc/files/worlds_/tex_/w1_tex.xgc   work/w1_tex
$B extract orig/gc/files/worlds_/sp_/chal1_/ch1_l2.xgc work/w1_world

# 2. convert everything (rewrites the GameCube extracts in place; the
#    original pack folders are kept next to them as <name>.bak)
$B port-world work/beta_ch1_l2 \
    --geo work/w1_geo --col work/w1_col --tex work/w1_tex --world work/w1_world

# 3. rebuild the archives into the game's data tree (backup first)
D=HWVX-SDL/orig/files
cp -n $D/worlds_/geo_/w1_geo.xgc   $D/worlds_/geo_/w1_geo.xgc.bak
cp -n $D/worlds_/col_/w1_col.xgc   $D/worlds_/col_/w1_col.xgc.bak
cp -n $D/worlds_/tex_/w1_tex.xgc   $D/worlds_/tex_/w1_tex.xgc.bak
cp -n $D/worlds_/sp_/chal1_/ch1_l2.xgc $D/worlds_/sp_/chal1_/ch1_l2.xgc.bak
$B pack --force work/w1_geo   $D/worlds_/geo_/w1_geo.xgc
$B pack --force work/w1_col   $D/worlds_/col_/w1_col.xgc
$B pack --force work/w1_tex   $D/worlds_/tex_/w1_tex.xgc
$B pack --force work/w1_world $D/worlds_/sp_/chal1_/ch1_l2.xgc
```

Then run HWVX-SDL with `--data HWVX-SDL/orig/files` and enter the first
chapter race: the engine streams the prototype quadrants instead of the
retail ones. Undo with the `.bak` copies: `cp` them back and re-run the
four `pack` commands.

The prototype archive (24 packs) contains one collision pack, 21 quadrant
geometry packs, one world texture pack and the world pack itself; step 2
reads all of them from that one extract.

## What `port-world` does

1. **Pairs the quadrant packs slot by slot.** The world pack holds a table
   of 21 quadrant records; both archives list their quad packs in that
   same slot order (proven by `tests/orig_worlds.rs`), so slot *i* of each
   side is the same corner of the map.
2. **Rebuilds each quadrant pack** from the prototype models (VIF strips →
   GameCube GX display lists, `convert::geometry`) into the GameCube quad
   layout: a 0x20 header pointing at a table of *(geometry id, 0, ref-reloc
   slot)* rows, models grafted 32-byte-aligned behind it. Geometry ids:
   the donor table's ids are reused positionally, extra models get fresh
   ids above every id in `w1_geo` (≥ `0x2d4c`), so no pack in the archive
   ever shares an id. Prototype quad packs carry no shader relocations;
   the rebuilt packs don't either — texture and vertex formats come from
   the model headers, like the retail packs that stream world content.
3. **Rebuilds the world texture pack** (`w1_tex`): keeps the GameCube
   shader table and the 308-byte shader-entry tail verbatim (re-pointing
   the pack's pointers and its 7 shader-entry texture relocations), and
   re-encodes the 153 prototype textures in place of the retail pixels —
   same order, so every material reference in the models still lands.
4. **Converts the collision pack** (`convert::collision`): the object
   graph is field-swapped per `Port_Swap.c`, the prototype's inline
   textures and palettes are dropped and the GameCube pack's own inline
   textures grafted back (the collision textures are the debug overlay,
   identical in role), sound references are rebuilt as a GameCube sound
   region, and the header's shared-pack references come from the donor.
5. **Patches the quadrant table** in the `ch1_l2` world pack: each record's
   `numGeoms` (+0x0C) and `packId` (+0x38) become the rebuilt count and
   `(count << 16 | quadId)`, and each quad pack's `sub_id` in the geo
   manifest (which `pack` writes into the archive TOC) is set to the same
   value. Nothing else in the world pack changes — 21×8 bytes.

## The matching contract

The loader (`FileStreaming.c:538-552`) finds a quad pack for quadrant *q* by
scanning the geo archive's TOC and matching:

```c
type == 7 (geometry) && worldId == q's archive world && entry id == quad packId
                    && (id >> 16) == pack's numGeoms
```

so the count is part of the identity: `packId = (numGeoms << 16 | quadId)`
must agree between the world record, the geometry header, and the TOC
`sub_id`. `port-world` updates all three together; `tests/orig_world_port.rs`
re-checks the whole chain (convert → join → split → parse every model →
patch the table) against the original archives.

GameCube quad ids are `0xffff` (the always-streamed hub) and `1..20`;
prototype quad ids are `906` and `5780..5799`. The low half of `packId` is
always kept from the GameCube record, so streaming ids survive; only the
high half (the geom count) moves.

## Useful while working

* `hwvx-x info <archive or folder>` — pack list with counts (`_gNN`) and
  pack ids (`qNNNN`, shown as the TOC sub_id).
* `hwvx-x export-gltf work/w1_geo out_dir --textures work/w1_tex` — every
  rebuilt model as `.glb`, textured through the rebuilt world pack.
* `hwvx-x textures work/w1_tex out_dir` — the rebuilt world textures as TGA.
* `hwvx-x verify orig/` — proves unmodified archives still rebuild byte for
  byte (only run against untouched extracts).

## Restoring

The retail GameCube archives are unchanged under `orig/`; the game's copies
under `HWVX-SDL/orig/files/worlds_/` have `.bak` twins. To go back: restore
the `.bak` files and rebuild nothing — the archives *are* the game data.
The GameCube quadrant table never changes in a way that would confuse the
retail packs (the low half of every `packId` was preserved).
