# Autopilot (test switch)

`HWVX_AUTOPILOT=1` lets the game's own AI drive player 1, for tests that need to cover a whole level.

**What it is not:** a check that a level is beatable. The game's AI is not good enough to judge that: a PASSED shows one route worked for the AI, a FAILED doesn't show a human couldn't win. Use it for coverage (crossing many quadrants), for logic-identical comparisons (the same run under different settings) and as a smoke test (loads, starts, doesn't break). Beatability is judged by a person in Test drive. It is test-only (no launcher option), off by default and deterministic under `HWVX_DETERMINISTIC`. With it off nothing changes (attract demo frame-identical).

* **Objective mode** (`HWVX_AUTOPILOT=1`). The destination is the target the HUD arrow points at. The route there is an A* search over the world's nav graph, which the car follows with the game's own AI path-following. It drives straight at the target only on the last stretch, and plans again when the objective changes or the car drifts off the route.
* **Fights.** Mission fight objectives use the game's combat AI, with the enemy as its target. Battle uses combat AI throughout. Drag Race is driving only.
* **Tour mode.** `HWVX_AUTOPILOT=tour` covers the world by driving to its nav points, or to quadrant centres where a world has none. `HWVX_AUTOPILOT_ROUTE=x,z;x,z;…` gives the points explicitly.
* **Result.** A run ends with `autopilot: PASSED` or `FAILED` (lost, stuck, long reverse, fall, reset, timeout), then quits.
* **Tool.** `tools/autopilot_drive.sh BUILD DATA OUT MODE WORLD SECONDS [VAR=value…]` writes the car log, the list of quadrants visited and a contact sheet. `tools/autopilot_drive.sh compare OUT A B` compares two runs. `HWVX_QUADLOG=1` logs quadrant changes.

Status (GameCube data):

| Level | Result |
|---|---|
| Challenge 1 | PASSED in 50.6 s (re-run at merge) |
| Mission 1 | PASSED in 156.6 s, including its fight |
| Drag Race | won in 72.2 s |
| Battle | **unfinished**: no result yet |
| Joyride | **unfinished**: "stuck" at the start |

The autopilot's work was paused by the user, then merged as it was. Still to do: the off-route summary, and a raycast check before the final straight run at the target.
