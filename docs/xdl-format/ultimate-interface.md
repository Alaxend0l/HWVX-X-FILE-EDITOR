# The ultimate build: interface art as one set

Belongs in: docs/xdl-format/ultimate.md (the "menus, and their textures" row of the summary table
and a section on interface textures), docs/xdl-format/README.md §9 (recipe fields), and
docs/menus/README.md (a note that a mixed build's menus and HUD are one release's art).

## Rule (the user's decision)

* Interface art is a set per release: the GameCube's and the PC's are **yellow** (gold), the PS2's
  **silver** (chrome, blue). A mixed build never combines them.
* The ultimate build's set is one choice: "Yellow (GameCube)" or "Silver (PS2)". The PC is never
  a source of interface art. Exception: **font pages come from the PC** when its page has the same
  glyphs, else from the set's release (else the base's).
* Everything that is not interface art keeps the per-texture "best" rule (§1.6).

## Recipe and launcher

* Recipe field `"interface": "gc" | "ps2"` (default `"gc"`). A rule `"from": "interface"` takes
  the set's copy (the built-in recipe uses it for every texture of HW_IPack, mcp and HW_brief,
  replacing the old `"from": "base"`).
* The launcher's Convert page, on the mixed game's row: a button "Menus: Yellow (GameCube)" /
  "Menus: Silver (PS2)", saved as `interface=ps2` in sources.cfg (absent: gc). For the built-in
  recipe it overrides the recipe's field. The choice is part of the build's stamp
  (Port_MixStamp), so changing it shows the build as out of date ("Build again").

## What counts as interface art (Port_Mix.c, Port_IfaceScan.c)

* Every texture a pack's interfaces show (Port_IfaceTextures: images, texture key animations,
  briefing characters' animations = art; text, box and briefing fonts = font; a texture used as
  both is art), in any pack, worlds included (pause, save screens, level HUD; some are also drawn
  by world geometry, which then gets the set's copy too: 928 such textures in non-interface packs).
* Textures of share packs that *other* packs' interfaces show: the car packs' HUDs draw
  HW_Cars share_2's gauges, the worlds' interfaces HW_share share_1's fonts, frames and effects
  (23 textures). A share pack is mixed before the packs that refer to it load, so `--mix-build`
  runs a first pass that loads every pack uncached and only records these references
  (Port_IfaceTexturesAll), then builds. Playing a mixed game that was never built (packs mix as
  they first load) has no such list: those 23 follow the "best" rule there.
* Pick: the set's paired copy; else, for a texture the pairing table leaves single because the
  releases drew it differently, the set's single texture of the same size and shape (alpha within
  24 levels, clearly closest, else the one at the same index): the GameCube flame logo <-> the
  PS2's chrome logo (ShapePair, 118 textures); else the base's own (a fallback).
* Fonts: the PC's paired page when GlyphDistance (the larger page box-filtered to the smaller's
  size, mean abs difference of premultiplied RGB and alpha) is under 12. Measured: every PC page
  pairs at the same size as the GameCube's and PS2's (64x64, 128x128, 256x256), distances 0..2.8,
  none rejected.
* The manifest writes every interface texture with `"ui": "art"|"font"`, whichever release it
  came from. `xdlrs mix ui-check MANIFEST --set gc|ps2` counts them and checks the rule.

## Numbers (retail GC+PC+PS2, base GameCube)

| | Yellow (gc) | Silver (ps2) |
|---|---|---|
| art from gc / ps2 / pc | 3,337 / 0 / 0 | 180 / 3,157 / 0 |
| of which by shape | - | 118 |
| fonts from pc / set / base | 97 / - / 11 | 97 / 11 / 0 |
| fallbacks (base's own) | 11 font pages (no PC copy) | 180 art + 11 fonts |

Silver fallbacks: platform glyphs and the GameCube controller picture (world packs' 11-13 or 10-12,
HW_IPack interface_3:24, _5:3, _6/_7:41,124,125, _8:25): kept, since the menus' button prompts
are the GameCube's; and race*l1 font pages (the PC lacks those archives, the PS2 has no pair).

Replaces the section "Open: interface colours" of docs/xdl-format/ultimate-interface.md.

## Interface colours (silver set only)

The packs stay the GameCube's (gameplay data, scripts, the menus' button prompts). For the silver
set, as each pack is mixed (Port_Mix.c, MixIfaceColours), its interfaces are paired with the PS2
copy of the same pack and the PS2's *look* is written into the base's interface data:

* Pairing: every interface header of both packs (Port_IfaceScan's header test; the PS2 blob read
  unrelocated with its own pointer list) by a longest common subsequence of their shapes (object
  count and the object types in order). Headers left in a gap with as many on both sides pair
  object by object (a longest common subsequence of (type, role, id)) when all but three of the
  smaller's objects align: the car HUD (GameCube 44 objects, PS2 45: one image more) and its
  second panel (38 / 37).
* Taken per paired object: the colour key tracks of the default animation and of each state
  (states paired by place, else by state type), RGBA only; key durations and counts stay the
  base's. A track whose key count differs takes, for each base key, the PS2 key whose time span
  holds that key's start (the GameCube's 2-3 key pulses become the PS2's one colour). Also an
  image's blend mode (IfaceImageDef +1: the PS2 draws its empty meters with mode 3 where the
  GameCube uses 2), a text's blend (+0xE), briefing characters' text colours, polygon points.
* Geometry an interface draws (image type 1: the HUD's 3D target arrow, HW_Cars share_2 geometry
  1, referenced from every car pack) has its groups' textures counted as interface art
  (PORT_IFACE_GEOMTEX; the builder's first pass records the cross-pack geometry slot). The arrow's
  flat colour textures (share_2 21 red, 22 yellow; the PS2's 21, 22 are blues) pair with nothing
  in the pairing table: an opaque single texture takes the set's single texture in the same place
  and of the same size.
* The yellow set is untouched: none of this runs, geometry textures stay with the "best" rule
  (checked: a yellow build's manifest and packs are byte-identical to the build before; one byte
  of hwb_m10.xdl, an audio pack, differs between any two builds, also with the old program).

Counts (retail GC+PC+PS2, base GameCube): 113 packs, 2,114 interfaces paired (128 object by
object), 2 not (one each in HW_IPack interface_6/_7; interface_4's single header has another
shape: PS2 pause/controller screens); 60,919 colour tracks, 4,299 keys changed, 423 tracks with
other key counts taken by time, 360 skipped (no PS2 state of that type); 200 blend modes; 687
briefing colours; 0 polygon points (none in the paired interfaces).

Checked side by side with the plain PS2 release at 1280x720 (tools/mix_pair_shots.sh): main menu,
challenge HUD (ch1_l3), battle HUD (bat1_l1), mission HUD (world1l1). The launcher's
MIX_SILVER_READY is 1: the Silver button shows.

## Tools (replace the Python ones in ultimate-interface.md)

* `xdlrs mix ui-check MANIFEST --set gc|ps2 [--base gc] [--list]` (was tools/mix_ui_check.py,
  removed): the interface textures of a build's manifest by release, the fallbacks, exit 1 when
  one breaks the set rule.
* `tools/mix_pair_shots.sh BUILD MIXPREF REFDATA GCDATA OUTDIR NAME SECONDS FRAMES [VAR=value...]`:
  the same scene in the mixed build and a plain release, joined side by side with
  `xdlrs mix pair LEFT.bmp RIGHT.bmp OUT.png [--half]`.

## Which PS2 copy: the one the paired object shows

The PS2 keeps two logos (blue glow and chrome, e.g. HW_IPack interface_1 textures 4/5 and 15/16)
where the GameCube has its flame logo, and its title shows the chrome one. The pairing table and
the shape test both see two equally good candidates, so the texture each paired PS2 *object*
shows decides (an image's texture, a texture key animation's keys, through both packs' texture
relocations): when the set has a copy and the PS2 object shows another texture of the same size,
that one is taken (16 textures: the title's logo, the logos of 7 mission worlds' screens). A base
texture two objects share that the PS2 gives two textures (the title's logo: the blue one fading
in, the chrome one after) takes the one not in its own place; the pack has no texture to spare
for the other, so the fade-in shows chrome too (hardly visible: it is dark and translucent). Art
the PS2 has no copy of stays the base's (the pad glyphs).

Menu frames compared with the plain PS2 (menu2_*.png): colours and logos match. Differences left,
all text or layout, not colour: the PS2's "MAXIMUM JUSTICE" subtitle and its flare (not added),
"PRESS START" vs the PS2's "PRESS START BUTTON", the GameCube's "LICENSED BY NINTENDO" line on
the legal screen, the button glyph (A vs the PS2's X, kept by design).
