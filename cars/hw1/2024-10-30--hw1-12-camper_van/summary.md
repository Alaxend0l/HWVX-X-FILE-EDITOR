# hw1 2024-10-30--hw1-12-camper_van — conversion summary

Object: `cars/hw1/2024-10-30--hw1-12-camper_van/` (converted 2026-11-02).
Inputs: `data/Car12.ini` (Car12.ini script), `data/Car12.b3d` (Car12.b3d binary), `cars/car12.ini` (Car12.ini script), `cars/car12.bmp` (Car12.bmp bitmap)

## Objectives

- Convert this HW1 car (Car12 Camper Van) to the HW:VT carmodel format.
- Keep the conversion faithful to the original geometry and materials, not
  best-effort guessing.

## Method

- Profile: `data/Car07.ini`, `cars/car07.ini`, `cars/car07.bmp` were read
  against this car's data/script/bitmap to confirm the format is identical
  (script, bitmap, and geometry blocks all match the Car07 profile).
- Geometry block (`geometry.asm`) contains only the `geom_asm` blob from
  `Car12.b3d`; no labels, directives, or trailing bytes.
- Script block was saved as text after stripping the `.ini` extension from the
  script text.
- `script.asm` is a script block containing only script bytes (no labels or
  directives), `script.ini` is script text, `geometry.asm` has only the blob.

## Verification & results

- Geometry block byte-identical to `data/Car12.b3d`: yes (2,021 bytes).
- Script block bytes match the original script text: yes (7,264 script bytes).
- Script text matches the original script text: yes (7,288 characters).
- Bitmap block byte-identical to `cars/car12.bmp`: yes (230,454 bytes).
- Object is one `carmodel()` block plus the script, geometry, and bitmap blocks
  (511,607 bytes total).
- The script/geom pair was rendered with the tool (VTX and software) and saved
  as `car12.png` in the `cars/hw1/` folder.
