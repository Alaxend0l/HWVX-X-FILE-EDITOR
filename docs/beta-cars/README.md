# Adding PS2-prototype cars to the GameCube game

This folder documents how the three cars that only exist in the leaked PS2
prototype — **Fandango**, **Sweet 16 II** and **Jester** — were added to the
GameCube release as new cars (ids 33, 34, 35), and how to do it yourself.

* `PROGRESS.md` – the work log: what was found, what was decided, and why.
* `research/` – detailed format specs (geometry, textures & sounds, object
  structures & shared-pack references, car roster) with the Python probe
  scripts used to work them out. Paths written `SCRATCH/...` refer to the
  folders the scripts were run on (archives unpacked with `hwvx-x extract`).
* `../../data/beta-imports.json` – how references into the shared packs
  (`HW_share`, the `HW_Cars` shared pack) map from the prototype to GameCube.

## Recipe

```sh
cargo build --release
B=target/release/hwvx-x

# 1. unpack the prototype cars and the GameCube cars / interface packs
$B extract orig/ps2-beta/XPS/ENGLISH/HW_CARS.XPS work/beta_cars
$B extract orig/gc/files/HW_Cars.xgc  work/gc_cars
$B extract orig/gc/files/HW_IPack.xgc work/gc_ipack

# 2. convert the three cars and append them to HW_Cars as ids 33-35
$B port-car work/beta_cars/02_car_1  --into work/gc_cars --id 33   # Fandango
$B port-car work/beta_cars/13_car_12 --into work/gc_cars --id 34   # Sweet 16 II
$B port-car work/beta_cars/18_car_17 --into work/gc_cars --id 35   # Jester

# 3. car lists: car-select screens (names, carousel range, thumbnails rendered
#    from the cars) and the race-results screens inside the drag/battle worlds
$B roster work/gc_ipack --name '33=Fandango*' --name '34=Sweet 16 II*' \
    --name '35=Jester*' --cars work/gc_cars
$B extract orig/gc/files/worlds_ work/gc_worlds
$B roster work/gc_worlds --name '33=Fandango*' --name '34=Sweet 16 II*' \
    --name '35=Jester*'          # patches the 20 drag/battle worlds

# 4. rebuild the archives
$B pack work/gc_cars   HWVX-SDL/orig/files/HW_Cars.xgc  --force
$B pack work/gc_ipack  HWVX-SDL/orig/files/HW_IPack.xgc --force
$B pack work/gc_worlds HWVX-SDL/orig/files/worlds_      --force
```

Then build HWVX-SDL (it needs the small `GameSettings.c` change described
below) and run it with `--data HWVX-SDL/orig/files`.

Useful while working:

* `hwvx-x render-car <car pack folder> out.tga [--yaw D --pitch D]` – a quick
  textured preview of a GameCube car pack.
* `hwvx-x textures <pack folder> <out folder>` – every texture of a pack as TGA.
* `hwvx-x info <archive>` – what each pack contains.
* `port-car --view-offset Y` – how high the car sits on the car-select
  turntable (GameCube cars use 0.5–3.5; default 2.0).
* `port-car --donor <GC car pack>` – which GameCube car the HUD and race
  cameras are copied from (default: car 0).
* `roster --thumb 33=picture.tga` – use your own thumbnail instead of a render.

## What `port-car` does

1. Reads the prototype pack (little-endian) and finds every object type in
   the car with a walker that mirrors HWVX-SDL's `Port_Swap.c`, so each field
   is byte-swapped to big-endian correctly (u32/f32 vs u16 vs bytes).
2. Rebuilds the models: PS2 VIF triangle strips → GX indexed triangle lists
   with vertex buffers; the shadow hull → a GameCube shadow-volume edge list.
3. Rebuilds the textures: 8-bit palettised (PS2 CLUT order) → CMPR, or RGBA8
   for translucent ones (glass), with mip-maps.
4. Rebuilds the sounds: PS2 VAG ADPCM → 16-bit PCM big-endian.
5. Remaps references into the shared packs using `data/beta-imports.json`.
   One shared texture has no GameCube counterpart; a white texture is added
   to the car instead.
6. Copies the HUD and the race-camera setup from a GameCube car (the
   prototype's camera type 1 does not exist on GameCube), keeping the
   prototype's stat bars and bone camera.
7. Uses GameCube tuning for damage (20 hits / 75 %, the prototype breaks parts
   on every hit) and sets the turntable height.
8. Gives the car its new pack id and adds it to the manifest.

## The code change (HWVX-SDL)

The game keeps one "unlocked" flag per car in the save data, sized for 33
cars. `src/game/GameSettings.c` / `include/common.h`: car ids 33–35 use 3
bytes of previously unused padding in `HotWheelsGameData` (old saves still
load), and are reported as unlocked, since nothing in the game unlocks them.

On a real GameCube the same rule would have to be patched into the DOL's
`GameSettings_GetCodeVariable` (0x8002FAC8) / `SetCodeVariable` (0x8002F28C);
that has not been done or tested.
