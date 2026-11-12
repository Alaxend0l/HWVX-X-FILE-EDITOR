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
  came from. `tools/mix_ui_check.py MANIFEST --set gc|ps2` counts them and checks the rule.

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

## Open: interface colours

The silver build's textures are the PS2's, but its interface *data* is the base's (GameCube):
colours of text, digits, fill bars, arrows and tints. On the PS2 the HUD's digits, fill bar,
waypoint arrow and speed readout are blue and the top bars silver-blue; in the silver build they
keep the GameCube's yellow/orange, and the white bar textures are tinted gold. A fully silver HUD
needs the PS2's interface colours too (object and animation-key colours of the paired interfaces),
or the PS2 as the base of the interface-bearing packs. Not done.
