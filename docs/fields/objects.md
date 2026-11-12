# World objects and triggers: decoded fields

Belongs in: superproject `docs/fields/` (a new `objects.md` next to README.md), with a short
pointer from `docs/worlds/README.md` ("the World's records") and from
`docs/xdl-format/README.md` § source projects (trigger files are a new file form).

Counts are over every retail world pack: GC 57, PC 51, PS2 57 (`tools/world_fields.py`,
`tools/xdl_tac.py SURVEY_ROOT`). Confidence: **symbols** (a name from a release with debug
symbols or the game's own strings), **code** (named from what the port's code does with it),
**guessed** (named from the data only; never read by the code).

## Trigger files (TAC)

A world pack's trigger table (the pack header's `num_triggers` / `trigger_offset`) is written
by `tools/xdl_graph.py extract` as one file per trigger, `trigger/<n>.jsonc`, schema
`schemas/Trigger.schema.json`: the record, its conditions, actions and else-actions inline,
and every parameter list decoded by its kind:

```jsonc
{
  "id": "base://ch1_l2/world_0/trigger/4",
  "type": "airbox_entered",
  "param": {
    "volumes": ["base://ch1_l2/world_0/World/0/volumes/19"],
    "cars": ["base://ch1_l2/world_0/World/0/tileCars/0"]
  },
  "paramWords": 5,
  "conditions": [{"type": "true"}],
  "actions": [
    {"type": "fx_point_change_mode",
     "data": {"mode": "on", "points": [".../World/0/quads/18/contents/fxPoints/4", "..."]},
     "paramWords": 9, "count": 4294967295},
    {"type": "fx_point_change_mode", "data": {"mode": "off", "points": ["..."]},
     "paramWords": 9, "delayInit": 500, "count": 4294967295, "delay": 500}
  ],
  "mode": "all"
}
```

Rules of the form:

- A field left out is zero (only `type` is required); the extractor leaves out every zero field.
- Inline arrays carry no count (`numConditions`, `numActions`, `numElseActions` follow the
  lists). A list the extractor can't inline (another node points into it, a kind that doesn't
  decode) stays a link, with its count.
- A parameter list is an object of its kind's items (`x-variants` in the schema, from Trigger.h's
  `TAC_PARAMS_*` macros). Keywords (`TacToken`) are names ("on", "toggle", "add", "less"...),
  numbers otherwise. Bytes after the list in its node are kept as `"tail"` (hex, or words when
  they hold links); zero padding is not shown (the layout's `pad`).
- Links name the World's records by their table entry: `World/0/tileCars/0`,
  `World/0/volumes/19`, `World/0/quads/18/contents/fxPoints/4`, `World/0/variables/2`.
- Building checks the file against the schema and names the place of an error
  (`trigger/4.jsonc: .../type: 'fx_point_change_mood' has no parameter layout`).
  A list may grow or shrink; the pack is re-laid out.

All 22,793 retail parameter lists decode and re-encode byte-identically (16,480 action lists,
6,313 trigger/condition lists); every world pack of the three releases round-trips.

### Trigger (0x2C), also a condition's record

| field | off | confidence | notes |
|---|---|---|---|
| type | 0x00 | symbols | `TriggerType`: the game's `triggerLabelsArray` and the test functions' symbols |
| param | 0x04 | code | parameter list of the kind (`TAC_PARAMS_TRIGGER_*`) |
| paramWords | 0x08 | guessed | never read; the exporter's word count of `param` (equal, or a constant off per kind, in every pack) |
| conditions / numConditions | 0x0C/0x10 | code | condition records (only type/param used) |
| actions / elseActions / numActions / numElseActions | 0x14-0x20 | code | elseActions: PlayerVehicleInAir's landing |
| state | 0x24 | code | `TriggerState` flags (armed, done, fired); run time, 0 in every pack |
| mode | 0x28 | code | `TriggerCond`: all, any, not_all, none |
| pad29 | 0x29 | - | 0 in every pack, never read |

### TriggerAction (0x18)

| field | off | confidence | notes |
|---|---|---|---|
| type | 0x00 | symbols | `ActionType`: pActionFuncArray's function symbols |
| data | 0x04 | code | parameter list (`TAC_PARAMS_ACTION_*`) |
| paramWords | 0x08 | guessed | as Trigger.paramWords |
| delayInit | 0x0C | code | ms |
| count | 0x10 | code | runs left, 0xFFFFFFFF always |
| delay | 0x14 | code | ms left; = delayInit in every pack |

### Kinds and counts (all releases)

Actions: fx_point_change_mode 2775, variable_change_value 2163, interface_play 1757,
vehicle_change_control 1249, hud_change_mode 1051, camera_set_targets 1044,
timer_change_mode 1070, music_change 1009, timer_change_value 648, print_debug_string 581,
end_interface 569, destructible_change_mode 511, code_variable_modify_value 432,
target_arrow_set_target 337, vehicle_change_mode 265, airbox_change_mode 176,
item_change_mode 162, round_complete 120, nav_point_change_mode 111, pause_tac_system 87,
variable_change_mode 87, moveable_change_mode 84, vehicle_teleport 72, reset_game_round 54,
set_player_order 30, vehicle_remove_item 27, vehicle_give_item 9.
Never in a retail pack: end_level, music_set_song, pause_game, speaker_change_mode,
vehicle_ai_set_named_path, vertex_color_seq_change_mode, set_player_vehicle,
vehicle_remove_creature, the four *_interface_pack, script_light_change_mode,
mission_obj_change_mode, swap_ai_script, play_sound_sequence, creature_change_mode.

Triggers and conditions: airbox_entered 1807, variable_value_check 1167, level_start 876,
true 733, code_variable_value_check 498, interface_end 381, item_pickup 287,
destructible_killed 153, vehicle_killed 132, timer_value_check 130, game_round_x_complete 54,
game_rounds_complete 54, item_dropped 26, airbox_exited 6, movable_volume_entered 6,
item_volume_entered 3. Never in a retail pack: airbox_stop_in, creature_killed, moveable_end,
p2p_activated, creature_dropped_off, interface_pack_end, vehicle_damage,
player_vehicle_gear, player_vehicle_in_air, creature_picked_up, sound_seq_queue_end,
creature_volume_entered.

Parameter items still `WORDS` (not decoded; none occurs in a retail pack, and the GC build's
functions for most of them are empty): creature_killed, moveable_end, p2p_activated,
creature_picked_up, music_set_song, speaker_change_mode, vehicle_ai_set_named_path,
set_player_vehicle, vehicle_remove_creature, script_light_change_mode.

### Enums (Trigger.h)

- `TacToken` (code): one numbering of every keyword the exporter writes: off 0, on 1, toggle 2,
  kill 3, set_speed 6, pause 7, resume 8, ai_hold 9, spawn 10, start 11, stop 12, to_start 13,
  to_end 14, add 15, subtract 16, set 17, less 20, greater 21, equal 22, show 23, hide 24,
  remove 25, kill_hidden 26, lock_control 34, unlock_control 35, order_loop 39, order_jump 41,
  damage_lost 42, damage_below 43, multiply 45, divide 46, greater_equal 47, less_equal 48.
- `TacCameraMode` (code; 27-30 fill TacToken's gap): look_at 27, orbit 28, zoom 29, player 30.
- `TacPlace` (code, Camera_TriggerActionGetLocation): car 1, instance 2, point 3, path 4,
  camera 5, point4 6. Retail packs use car only.
- `TacOperand` (code): constant 0, variable 1, code_variable 2.

camera_set_targets: `{"player": 0, "camera": {"kind": "orbit", "fov", "radius", "height",
"speed", "around": {"kind": "car", "car": link, "offset": [x, y, z]}, "at": {...}}, "end": 31}`.
player 1-4 sets that player's camera, 0 every camera. The exporter writes two places after
every mode (also `player`, which reads neither) and a 31 after them (`end`, never read).

## The World's records (World.h)

World packs: the World (0x00) is typed, and its counted tables as arrays (`World/0/tileCars`,
`.../quads/<n>/contents/instances`...) with an `items` list; named ints show by name.

| struct | records (gc/pc/ps2) | decoded |
|---|---|---|
| QuadInstance 0x40 | 38988 / 37203 / 38988 | all but pad37[5] (0) |
| WorldVolume 0x30 | 24213 / 23121 / 24213 | all; `data` per type still raw |
| QuadFxPoint 0x40 | 6148 / 5572 / 6148 | all but pad3A[6] (0) |
| QuadItem 0x40 | 2693 / 2369 / 2693 | all but pad2C, pad32, pad3A (0) |
| WorldQuad 0x50 | 818 / 782 / 818 | pads: 0x14 (VertexColorSeqManager's vertex colours, 0 in packs), 0x18 (384, 512, 768...: unknown) |
| QuadContents 0x1C | 818 / 782 / 818 | all |
| TilePath 0x20 | 678 / 650 / 678 | pad18 (0/1, unknown) |
| WorldVariable 0x10 | 480 / 408 / 480 | pad0 (0-3), pad8 (1, 2, 4, 8, 16, 128: a bit each, never read) |
| TileCar 0x60 | 265 / 255 / 265 | pad36 (0/1, with maxVehicle 1/32) |
| Teleporter 0x70 | 128 / 120 / 128 | pad64[0xC] (0) |
| WorldObjective 0x08 | 107 / 95 / 107 | all |
| WorldTimer 0x10 | 79 / 67 / 79 | pad4 (1, 2, 4, 8, 16, 64: as WorldVariable.pad8) |
| World 0xA0, WorldGameData 0x110 | 57 / 51 / 57 | pads 0 in every pack |
| TileSpeaker, TileLight, TileCreature | 0 in every retail world | schemas only |

New names (this branch):

| field | confidence | evidence |
|---|---|---|
| World.numLights (0x60) | code | VertexColorSeqManager_Init's loop count over `lights`; 0, 1, 4, 11, 15 |
| WorldVolume.type: `VolumeType` | code | airbox 0 (every volume a trigger names: 618 of 618, GC), force_box 1, reverb 2, camera 3 (Camera_UpdatePosition_Default keeps the car's up inside), fog 4, creature 5 (AI_Creature spawn volumes; PC rescue zones), light 6 (LightManager) |
| WorldVolume.shape: `VolumeShape` | code | bsp 0, sphere 1 |
| editorId: TileCar 0x20, QuadInstance 0x20, QuadFxPoint 0x20, QuadItem 0x10, TilePath 0x00, WorldVolume 0x00, Teleporter 0x40 | guessed | never read; a different small number per record of a world (sequential runs), the same in all three releases |
| TileCar.unused1C | guessed | 0xCDCDCDCD in every pack (MSVC's uninitialised fill) |
| TileCar.mode, QuadInstance.mode, QuadFxPoint.mode, QuadItem.mode as `TacToken` | code | set from the actions' keywords (on, off, kill) |

Pos vectors' `w` is 0xCDCDCDCD (-4.3e8 as f32) in placed records: uninitialised, kept as is.

## Tools

- `tools/xdl_tac.py SURVEY_ROOT`: decode and re-encode every retail parameter list.
- `tools/world_fields.py [--words] [--struct S]`: per field of the World's records, zeros,
  distinct values, pointer slots.
- `tools/typed_bytes.py [--tools DIR] [--by-schema]`: bytes of the world packs by kind
  (typed, trigger, geometry, texture, raw), for a before/after against another checkout.
- `tools/tac_survey.py`: the research tool the parameter layouts came from.

Typed bytes, GC world packs (177.2 MB; 25.4 MB geometry, 100.0 MB texture images): typed or
trigger 0.4 MB (0.8 % of the rest) on pc-data, 5.4 MB (10.4 %) on this branch. The largest raw
parts left: animation keys (keys.vec4 7.0 MB, keys.active/uv/color 6.9 MB), StateAnim 4.8 MB,
ObjAnim 2.3 MB, interface objects (IMObject[] 4.0 MB, ObjState[] 1.7 MB, CondList 1.2 MB,
VarValue 1.0 MB, EventDef 0.8 MB), particle emitters, event sequences (EventSeq.entries 0.3 MB).

## Viewer note

The earlier WIP also taught `src/port/viewer_graph.c` the World root node and enum/flag names
(`x-enum`, `x-flags`); it was left out of this branch (the viewer's owner's file). The diff is
in commit 311ea4a for whoever picks it up.
