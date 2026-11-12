# Online: 8 players, and mods


Sessions take up to 8 players (NET_PROTOCOL 3) in Battle and Joyride; Drag Race takes 4 at
most (the user's decision: its worlds have the retail grid of 4). The host's mode screen
refuses Drag Race with more than 4 in the lobby and says "Drag Race: 4 players at most";
players past 4 wait in the lobby. The retail game has 4 player ports; online:

- Ports 5-8 are human players from extra pool slots (PlayerManager.c), with their car, item and
  round results kept past the code variables' 4 (GameSettings.c, online only).
- Spawns, in this order: a retail player spawn nobody in the game has (so 4 players or fewer
  always start exactly where the retail game puts them, whatever their ports), a Battle
  world's AI spawns (AI is off online), then a spot of ours near one of the world's 4, found
  from the world's collision for the largest car, Surfin' School Bus (collision box 9 x 27):
  ground under all four corners as high as the spawn's, no wall within its half-diagonal plus
  a margin, its box 2-8 units clear of every other spawn's, in plain reach of the spawn.
  Joyride's single spawn gets 7 such spots around it. Offline nothing changes.
- Scripts: a trigger or action whose car list names all of the world's 4 player spawns
  (airboxes, control on/off, teleports) includes players 5-8.
- The round summary has four rows; with players 5-8 in the game it shows P1-P4 and P5-P8 in
  turns (3 s each). No auto-advance online in Battle and Drag Race (unchanged).
- Mods: each machine sends a fingerprint of its enabled mods (FNV-1a of "id@version" in load
  order, plus names) in MSG_HELLO on joining. A client whose mods differ from the host's
  leaves with "Mods differ. Host: ... Here: ..."; the host leaves it out of games and shows
  "Player N has other mods" in the lobby. No mods on both sides: they match.
- The PS2 prototype counts as a mod: with an import mode other than `off`, the mode and the
  prototype's identity (FNV-1a of its files' paths and sizes, `Port_ProtoIdentity`, proto.c) go
  into the fingerprint, and the summary reads "..., PS2 prototype (<mode>, <6 hex>)". A
  different mode or prototype is refused like other mods ("Mods or prototype differ"). It
  hashes names and sizes, not contents.
- A registered track goes in MSG_SETUP by id (levels past a script's own are numbered by each
  ids.lock). A client without that track leaves the session ("The host's track isn't here:
  <id>") rather than loading the level it was made from.
- Players 5-8 that start at a free spawn of ports 1-4 keep their own port
  (`Port_OnlineTilePort`). In Drag Race the script renumbers players by lane
  (ActionSetPlayerOrder), so a round goes to the lane's number, as on retail.
- Test: `tools/online8_test.sh BUILD DATA OUT MODE SECONDS PLAYERS`; `GAP=P` leaves port P
  empty (its bot joins and leaves in the lobby, held open with HWVX_ONLINE_TEST `lobby=SECONDS`),
  and every other port must sync at the host.
- The round time travels as read (ms): the menus set it in 5-minute steps, and setting the
  read value again multiplied it (a bug before 8 players).
