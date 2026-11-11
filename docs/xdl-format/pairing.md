# Cross-release pairing table

[`pairing/`](pairing/) maps every asset's structural id (`base://`, [`README.md`](README.md) §10.2) to
a fingerprint of that asset in each release that has it: GameCube (`gc`), PC (`pc`), PS2 retail
(`ps2`) and the PS2 prototype (`proto`). Mods use it to check base data on whichever release a
player owns (§8.1); mixed "ultimate" builds use its pairs to substitute an asset from another
release (§9). It holds ids, hashes, sizes and counts only, never game data, so it can be published.
`pairing.py --verify` checks that every value is one of those.

Generator: [`scripts/pairing.py`](scripts/pairing.py) (§6). 69,734 assets in 109 files, 19.4 MB.

## 1. Ids

```
base://<archive>/<pack>/<kind>/<index>
base://fmv_/<movie>/movie/0

archive  the archive's file stem as the GameCube spells it (HW_Cars, HW_IPack, w2_geo, bat1_l1,
         HWB_M01); matched case-insensitively. Archives the GameCube lacks keep their own
         release's spelling (the prototype's HW_SSEQ, HW_IT_PC, HW_MUSAK).
pack     <type>_<id> from the archive's TOC (car_5, share_2, interface_6, world_0,
         world_texture_0, audio_3), plus _q<n> for a quadrant geometry pack, n = the low 16 bits
         of its sub id (geometry_0_q12). The high half (its group count) is left out because it
         changes when a world is edited: the PC's w3_geo q12 has other group counts.
kind     texture | sound | bank | song | geometry | movie
index    the asset's index in the GameCube's own table:
           texture   the pack's texture table
           sound     the pack's SFX descriptor table
           bank      the instrument number in the pack's sndb bank
           song      0 (a pack has at most one song)
           geometry  the order of the geometry headers in the pack's data (the 0x2003 scan)
         or <release>:<index> for an asset the GameCube lacks, numbered in that release's table
         (base://HW_Cars/car_3/texture/ps2:5, base://bat3_l6/world_0/geometry/pc:82).
movie    the lower-case file stem (hw_intro, bglogo, hwtrail).
```

Examples: `base://HW_Cars/car_5/texture/3`, `base://HW_IPack/interface_6/sound/17`,
`base://w2_geo/geometry_0_q12/geometry/40`, `base://HW_music/music_1/song/0`.

The GameCube is the reference because it has every pack the retail releases have (including the
six race*l1 drag worlds the PC lacks). A level's group is addressed under its geometry
(`.../geometry/12/level/0/group/3`); the geometry's fingerprint covers it.

## 2. Files and records

One JSON Lines file per archive (`pairing/HW_Cars.jsonl`, `pairing/fmv_.jsonl`), sorted by pack,
kind and index so regenerating gives a clean diff. The first line lists the archive's packs and
the releases that have each:

```json
{"archive":"HW_Cars","packs":{"car_0":["gc","pc","ps2","proto"], ...}}
```

so an absent asset can be told apart from an absent pack. Then one line per asset:

```json
{"id":"base://HW_Cars/car_0/texture/0",
 "gc":{"i":0,"x":"24e0b8a8ffcffb4deb006ed830357aa7","s":2696,"w":64,"h":64},
 "pc":{"i":0,"x":"5ff82b0c3a7a28bd5d8476dae372fa2a","s":2752,"d":2.61},
 "ps2":{"i":0,"x":"f41929297e4fe037bceed0f073a3e6ed","s":6493,"d":2.55},
 "proto":{"i":0,"x":"855f35040b5aeff3d4cb696a1dd33d73","s":6472,"d":3.07}}
```

| field | meaning |
|---|---|
| `i` | index in this release's own table (differs from the id's index when tables shift) |
| `x` | exact fingerprint (§3.1); omitted when equal to `n` (PC samples, songs) |
| `n` | normalised fingerprint (§3.2), for sounds, bank samples, songs, geometry |
| `s` | bytes hashed for `x` |
| `c` | samples (sound, bank) or triangles (geometry) |
| `w`, `h` | texture size; omitted on later releases when equal to the first's |
| `m` | how this copy was paired with the record's first release: `index` (omitted), `content`, `order`, `name` (§4) |
| `d` | content distance to the first release's copy (§4); omitted when 0 |
| `bad` | 1: the copy could not be decoded (none in the current table) |
| `flags` | `only:<release>`: no other release has it. `ambiguous:<release>:<i,...>`: candidates in that release that matched equally well, so none was paired |

The first release in a record (in the order gc, pc, ps2, proto) is the one the others were paired
against; every other entry in the record is a pair with it.

## 3. Fingerprints

Hash: **BLAKE2b with a 16-byte digest** (`hashlib.blake2b(digest_size=16)`), lower-case hex.
Inputs are taken from the pack's sections as `hwvx-x extract` writes them (`data.bin`,
`sound_data.bin`, `pointers.bin`, `pack.json`'s header).

### 3.1 Exact (`x`): the asset in its release's own encoding

| kind | bytes hashed |
|---|---|
| texture | `u16le` format, width, height, level count as the header gives them, then the pixel data of every level from the header's pointer, as stored (GameCube CMPR/RGBA8 tiles, PC DXT1/DXT5/RGB/RGBA, PS2 PSMT8/PSMCT32 linear), then for PSMT8 its 0x400-byte CLUT as stored. Level sizes: GameCube CMPR 32 bytes per 8×8 tile, RGBA8 64 per 4×4; PC DXT 8/16 per 4×4 block; PS2 1 or 4 bytes per texel; level count 0 means 1 |
| sound | the sample bytes the descriptor points to (pointer, length), as stored: GameCube PCM16 BE, PC PCM16 LE, PS2 SPU-ADPCM |
| bank | an instrument's sample bytes (bank data + entry offset, entry length), as stored |
| song | the `RIFX` `MIDI` chunk (size field + 8); the prototype's `IMPM` module from its offset to the end of the section |
| geometry | header (0x40), LOD distances (4 × levels), each level record (0x18); per level its group records (0x20, the texture field at 0x18–0x1B zeroed), each followed by its draw data (GameCube display list, `size` at +0x10; PC index list, 6 × triangles; PS2 16-byte prefix and DMA `ret` packet, or skin records up to the null one and their packets), then the vertex buffer (GameCube 0x10 header and arrays up to its end offset; PC 0x20 header and each array for max index + 1 vertices). Every pointer slot (from `pointers.bin`) is zeroed, so the hash does not depend on where the geometry sits in the pack. Shadow-volume groups contribute their records only |
| movie | the whole file |

### 3.2 Normalised (`n`): the asset in a release-neutral form

| kind | normal form | equal across |
|---|---|---|
| sound, bank | samples as PCM16 little-endian (GameCube's swapped; the PC's are already that) | GameCube = PC wherever both have the sample. PS2 is 4-bit ADPCM, so it has no `n`; its pairs carry a correlation distance instead |
| song | the chunk itself (no conversion) | all three retail releases, except one song (below) |
| geometry | `u32` level count; per level a kind byte (`d` drawn, `s` shadow) and `u32` group count; per drawn group `u32` triangle count and its triangles **sorted**, each as 15 `int64le`: three vertices of (x, y, z, u, v), each `round(value × 4096)`, the triangle rotated to start at its least vertex (winding kept); a shadow group is the byte `S`. Triangles: GameCube display lists (strips and fans expanded as `ps2_geom_check.gc_group_tris` does), PC index lists, PS2 packets re-oriented (`ps2_geom.orient`) | GameCube = PC in 20,379 of 20,423 pairs (the rest are art edits). GameCube = PS2 in 10,606: the PS2 shifts uvs by whole numbers per triangle and drops some triangles, though positions match in 21,560 of 21,652 (`d` = 0) |
| texture | none: every release encodes its own texels (PC DXT from the source art, GameCube CMPR with squared colours, PS2 palettes). Pairs carry a content distance instead | |
| movie | none (THP, Bink, MPEG-2) | |

## 4. Pairing

Per pack, every release is paired against the GameCube's assets; what the GameCube lacks is
paired among the other releases in the order pc, ps2, proto. A release's pack is compared only
with the same pack (same archive, type, id) in the others. Nothing is paired on a guess: an asset
with no clear partner stays single (`only:`) or is flagged `ambiguous`.

| kind | rule | `d` |
|---|---|---|
| texture | **index** when both tables have the index, sizes match and its distance is within 1.0 of the best in its row and column; otherwise **content**: greedy on distance, under 24. A content candidate with a different-looking alternative (another candidate within max(1.0, d/4) whose own thumbnail differs by ≥ 1.0) makes both sides `ambiguous`, unpaired. Retail pairs need equal sizes; prototype pairs don't | mean absolute difference of 16×16 box thumbnails of premultiplied RGB (0–255), GameCube colours taken to the PC space (`sqrt(255·v)`) |
| sound | GameCube–PC: **index** when the samples are equal, else **content** (equal samples elsewhere in the pack: the interface packs reorder). Lossy copies: **index** at correlation ≥ 0.95, else **content** among sounds of ±15% duration at ≥ 0.95 and 0.02 better than any alternative with other bytes | 1 − best correlation (lags ±8 samples, resampled to the first copy's rate, the PS2's silent first ADPCM block dropped) |
| bank | **index**: a bank is one instrument table in every release | as for sounds; a measure only (short looped waveforms resample badly: median 0.008, p90 0.29) |
| song | **index** if the format matches (`MIDI`); the prototype's `IMPM` modules are not paired | 0 equal, 1 different |
| geometry | by bounding box in order (bit-equal across releases unless edited, as `geom_survey.align`): **index** at the same position, **content** at another; equal-length runs without equal boxes by **order** (retail always, prototype at d < 0.5); what is left by triangles (count within 10%, d ≤ 0.1, unambiguous) as **content** | 0 when `n` is equal, else 1 − shared position-only triangles / the larger triangle count |
| movie | **name** (file stem) | none |

`content` with GameCube–PC textures mostly means a shifted table: the GameCube has extra entries
(its pad glyphs) before the shared ones; the PS2 has its own extras (env maps at index 0, DualShock
glyphs, [`../ps2-version/README.md`](../ps2-version/README.md) §2.2).

## 5. Coverage

`pairing.py --summary`. *paired*: in a record with another release; *paired retail*: with another
retail release; *single*: no partner (`only:`), *pack only here*: the pack itself exists in no
other release; *ambiguous*: left single because candidates tied; *paired, ambiguous to another*:
paired with one release, tied in another.

| kind | release | present | paired | paired retail | single | single, pack only here | ambiguous | paired, ambiguous to another | `n` equal to gc | method (vs first release) |
|---|---|---|---|---|---|---|---|---|---|---|
| texture | gc | 11,673 | 11,419 | 11,419 | 223 |  | 31 | 423 |  |  |
| texture | pc | 10,970 | 10,867 | 10,849 | 58 |  | 45 | 382 |  | index 4,571, content 6,261 |
| texture | ps2 | 11,759 | 11,421 | 11,324 | 283 |  | 55 | 423 |  | index 7,436, content 3,888 |
| texture | proto | 12,861 | 4,906 | 4,906 | 7,471 | 5,507 | 484 | 20 |  | index 735, content 4,171 |
| sound | gc | 4,723 | 4,721 | 4,721 | 2 |  |  |  |  |  |
| sound | pc | 4,347 | 4,345 | 4,345 | 2 |  |  |  | 4,345 | index 4,134, content 211 |
| sound | ps2 | 4,723 | 4,717 | 4,688 | 6 |  |  |  |  | index 4,574, content 114 |
| sound | proto | 4,502 | 1,958 | 1,958 | 2,544 | 462 |  |  |  | index 84, content 1,874 |
| bank | gc | 1,492 | 1,492 | 1,492 |  |  |  |  |  |  |
| bank | pc | 1,370 | 1,370 | 1,370 |  |  |  |  | 1,370 | index 1,370 |
| bank | ps2 | 1,492 | 1,492 | 1,492 |  |  |  |  |  | index 1,492 |
| song | gc | 60 | 60 | 60 |  |  |  |  |  |  |
| song | pc | 54 | 54 | 54 |  |  |  |  | 47 | index 54 |
| song | ps2 | 60 | 60 | 60 |  |  |  |  | 60 | index 60 |
| song | proto | 48 |  |  | 48 | 3 |  |  |  |  |
| geometry | gc | 21,652 | 21,652 | 21,652 |  |  |  | 383 |  |  |
| geometry | pc | 20,467 | 20,423 | 20,412 | 44 |  |  | 346 | 20,379 | index 19,512, content 888, order 12 |
| geometry | ps2 | 21,652 | 21,652 | 21,652 |  |  |  | 383 | 10,606 | index 21,615, order 37 |
| geometry | proto | 28,362 | 9,449 | 9,449 | 18,530 | 15,399 | 383 |  |  | content 9,327, order 122 |
| movie | gc | 5 | 5 | 5 |  |  |  |  |  |  |
| movie | pc | 7 | 5 | 5 | 2 |  |  |  |  | name 5 |
| movie | ps2 | 5 | 5 | 5 |  |  |  |  |  | name 5 |
| movie | proto | 4 | 2 | 2 | 2 |  |  |  |  | name 2 |

Distances of the pairs (median / p90): textures PC 2.2 / 4.6, PS2 2.3 / 5.0, prototype 2.8 / 6.3;
sounds PS2 0.003 / 0.015, prototype 0.005 / 0.018.

What stays single or ambiguous, and why:

* **Textures, retail.** Single: the per-platform glyphs, logos and item rings and the PS2's env maps
  and extra car textures ([`ultimate.md`](ultimate.md) §1.7): GameCube-only mostly in HW_Items (14),
  HW_IPack (11) and the world packs (≤ 6 each), PS2-only mostly in HW_IPack (23) and the world
  packs (≤ 9 each). Ambiguous (GameCube 31, PC 45,
  PS2 55, all but one in world packs): small near-flat or look-alike textures where two different
  candidates are equally close and the indices don't agree. A mixed build keeps the base's copy.
* **Sounds.** GameCube-only: the race*l1 copies (race1l1 #34, race4l1 #40) of a sound whose PS2
  version is a different recording (the 0.4 s bat2_l9 #30 of `ultimate.md` §3, correlation 0.37);
  its PS2 copies in world4*, ch3_l1/2 are the six PS2 singles. PC-only: `HW_IPack/interface_6|7`
  `sound/pc:17`, in the PC's merged car-select header; the GameCube's #17 there (the one sample the
  PC lacks) pairs with the PS2 only.
* **Songs.** One song differs on the PC: `bat3_l6`, `bat6_l6`, `race4l2` and four other world packs
  share a song whose PC copy has a few changed header and event bytes (31,224 vs 31,218 bytes),
  so `d` = 1 there; all others are byte-identical. The prototype's songs are Impulse Tracker
  modules, another format: listed, not paired.
* **Geometry.** PC-only 44: w5r1_geo quadrant 14 (23), bat3_l6 (9), world3 (8), race2l2 (1): the
  PC's world edits (`ultimate.md` §2); car_26 (3), where the PC's geometries differ from the
  GameCube's. Every GameCube geometry has its PS2 copy.
* **Movies.** PC-only `hw_gal`, `hw_mark`; prototype-only `hwtrail`, `m1intro`. The PS2's `hwboot`
  pairs by name though it is a longer cut.
* **Prototype.** Paired only where its pack has the retail key and the content matches: 38% of
  its textures, 43% of its sounds, 33% of its geometry. Most of the rest is in packs retail
  doesn't have (its own world quadrant numbering, the CH1_L2 level, HW_SSEQ and others); in shared
  packs it is prototype art. The 383 ambiguous geometries are small ones (often one
  triangle) with several equally close prototype candidates; the 484 ambiguous textures are
  prototype look-alikes.

## 6. Using the table

**A mod** names base data by id and the fingerprint it was made against:

```json
{"base": "base://HW_Cars/car_5/texture/3",
 "expect": {"gc": "<x>", "pc": "<x>", "ps2": "<x>"}}
```

The tools fill `expect` from this table, so a mod made on one release lists every release's
fingerprint. For sounds, songs and geometry `n` is enough: one value checks the GameCube and the
PC copy.

**The compiler**, building the player's cache:

1. Finds the asset: the archive and pack from the id, then the entry for the player's release; its
   `i` is the index in that release's table (it differs from the id's when the table shifts).
2. Computes the fingerprint of the player's copy as §3 defines it and compares it with the mod's
   `expect` for that release (or with `n`). A mismatch says which release the copy is, or that it
   was changed; an id without an entry for the player's release is reported as release-only
   (`only:`) instead of building something wrong.
3. Mixed installs resolve per file: the release of the file holding the pack picks the entry.

**A mixed build** (§9) substitutes an asset only along a pair: the base's entry and the chosen
release's entry in the same record. Records flagged `ambiguous` towards that release, and single
ones, keep the base's copy. `d` lets the rule table set a threshold (a texture with `d` near the
24 limit, or a geometry with `d` > 0, is an edit, not the same art). The MANIFEST section records
the id and the `x` it took.

## 7. Regenerate

```sh
B=target/release/hwvx-x; S=HWVX-SDL/build-survey
$B extract orig/gc/files $S/gc
$B extract "$HOME/Library/Application Support/HWVX-SDL/HotWheelsVelocityX/pc" $S/pc
$B extract orig/ps2/XPS/ENGLISH $S/ps2
$B extract orig/ps2-beta/XPS/ENGLISH $S/beta
cd docs/xdl-format/scripts
python3 pairing.py ../../../$S            # ~1 min on 12 cores, writes ../pairing/
python3 pairing.py --verify               # only ids, hashes, sizes, counts
python3 pairing.py --summary              # the table of §5
```

Movies are read from the releases' folders (defaults: `orig/gc/files/fmv_`, the import's
`pc/FMV_`, `orig/ps2/XPS/ENGLISH/fmv_`, `orig/ps2-beta/XPS/ENGLISH`; `--fmv pc=DIR` to change
one). Needs numpy; run without `-I` so the user site-packages load. The output is deterministic:
two runs give identical files. Reuses `ps2_tex.py`, `ps2_audio.py`, `ps2_geom.py`,
`ps2_geom_check.py` ([`../ps2-version/scripts`](../ps2-version/scripts)) and the decoders of
`tex_survey.py`.
