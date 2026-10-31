# PC `Hot Wheels.exe`: the 33-car limit and how to lift it

Exe: `orig/pc/Hot Wheels.exe`, sha1 `91af8432d6903371f724c4c2924c4ba51cc9eb73`. PE32, image base 0x400000, DllCharacteristics 0, so it always loads at its preferred base. `.text` VA 0x401000 = file 0x1000, so **file offset = VA - 0x400000** in `.text`.
Tools: `research/pc/disasm.py` (capstone linear dump, output `orig.asm`). Function boundaries were cross-checked against `HWVX-PC-Decomp/config/hotwheels/functions.json`.

## 1. Places that limit or index by car count

| VA | file | what |
|---|---|---|
| 0x429FA0 | 0x29FA0 | **GameSettings_SetCodeVariable** (Ghidra `_func_00429FA0`, size 0x6E0). `cmp eax,0xC8` / `add eax,-0x64` / jump table at 0x42A4F8 |
| 0x42A00C | 0x2A00C | set, case 0x64, type 0: `83f821 cmp eax,0x21` / `0f83df040000 jae 0x42a4f4` (pop esi; ret) / `mov ecx,[esp+0x10]; test ecx,ecx; seta dl; mov [eax+0x64d300],dl` (0x42A015..0x42A025). **Car limit (set)** |
| 0x42A680 | 0x2A680 | **GameSettings_GetCodeVariable** (`_func_0042A680`, size 0x910). Pushes ebx,ebp,esi,edi; arg = `[esp+0x18]`; jump table at 0x42ADD8 |
| 0x42A6C1 | 0x2A6C1 | get 0x64: `push 1; call 0x42afb0` (IsCheatEnabled(1)); if true, goes to 0x42A6CF `pop edi; pop esi; pop ebp; mov eax,1; pop ebx; ret` |
| 0x42A704 | 0x2A704 | get 0x64, type 0: `83f821 cmp eax,0x21` / `0f831c010000 jae 0x42a829` (xor eax,eax; return 0) / `movsx eax, byte [eax+0x64d300]`. **Car limit (get)** |
| 0x42A6EB / 0x429FEE | | type 1 (DD Rescue) branch, `cmp eax,0x14`, `[0x64dd40]`. Not used by Hot Wheels |
| 0x42C06B..0x42C076 | 0x2C06B | save-slot summary (`_func_0042BF60`): `cmp byte [eax+ecx],0; inc ebx; ... cmp eax,0x21; jb` counts unlocked cars over 33, then `sub ebx,6`. Same as the GC `for (i = 0; i < 33; i++)` ("Available Cars %d"). Display only. Unchanged |
| 0x429D0E..0x429DC4 | | default game data: zeroes the struct at 0x64D300 and unlocks cars 0..5, tracks, items |
| 0x4472AA | 0x472AA | ProgressionManager AI car pick: `movzx eax, byte [edi+0x37]` (tile car maxVehicle), `lea ebp,[eax+1]`, `div ebp`. **Data-driven** (world tile car +0x37), not in the exe |
| 0x420ACA..0x420AEE | | pack TOC cache: copies `count*0x18` bytes into a 50-entry cache (`count` at +0x4B0 = 50*0x18, as GC `FileTOCEntry entries[50]`). The header read is 0x800 bytes. **Not bounds-checked.** HW_Cars.xpc now has 0x22 = 34 entries (type-8 entry + 33 cars). With 3 more cars it has 37, still under 50 |
| 0x45656A | | `cmp eax,0x21` frame-time clamp (33 ms). Not related to cars |

**HotWheelsGameData is at 0x64D300** (0xF4 bytes, same layout as GC):
- cars[0x21] at 0x64D300
- tracks[0x39] at 0x64D321
- **pad5A at 0x64D35A**: no code reads or writes it except the zero-fill
- unk93 at 0x64D393
- unk9F at 0x64D39F
- unkA9 at 0x64D3A9
- unkAA at 0x64D3AA
- unkB0 at 0x64D3B0
- unkEC at 0x64D3EC
- padF0 at 0x64D3F0

No other code indexes cars by a fixed count. Car select, names, AI and results all take their ranges from data (see §3). The only references to `0x64d300`+index are the get and set above.

## 2. Patch

Two caves, both 15-byte NOP alignment runs that follow a `ret`:
- **A** at VA 0x429E51 (file 0x29E51), at the end of `_func_00429DD0`
- **B** at VA 0x42B8F1 (file 0x2B8F1), at the end of `_func_0042B880`

No code jumps into either cave, and neither the caves nor the two patched `jae` displacements have base relocations. All new code is rel32, so no relocation entries are needed.

| file off | original | new | meaning |
|---|---|---|---|
| 0x02A707 | `0f 83 1c 01 00 00` | `0f 83 44 f7 ff ff` | get: `jae 0x429e51` (cave A) |
| 0x029E51 | `90`x14 | `83 f8 24 0f 82 75 08 00 00 e9 ca 09 00 00` | `cmp eax,0x24; jb 0x42a6cf` (return 1); `jmp 0x42a829` (return 0) |
| 0x02A00F | `0f 83 df 04 00 00` | `0f 83 dc 18 00 00` | set: `jae 0x42b8f1` (cave B) |
| 0x02B8F1 | `90`x15 | `83 f8 24 73 08 83 c0 39 e9 17 e7 ff ff 5e c3` | `cmp eax,0x24; jae ->pop esi;ret; add eax,0x39; jmp 0x42a015` (stores at 0x64d300+id+0x39 = pad5A[id-0x21]) |

Result:
- `get(0x64, 33..35)` returns 1.
- `get(0x64, >=36)` returns 0, as before.
- `set(0x64, 33..35, v)` stores into pad5A[0..2], matching the GC/SDL `carsExt`.

The set half is optional. Without it, set already ignores ids of 0x21 and above, so it is harmless either way. `--no-set` applies only the get half (2 edits, 20 bytes).

Script: `research/pc/patch_exe.py IN OUT [--no-set]`. It checks the sha1 and every original byte, refuses to overwrite the input, and prints the patched disassembly. The output `research/pc/patched.exe` (sha1 `5485ea3f…`) differs from the original in 35 bytes.

**Wine check.** I ran it twice, 30 s each, from `research/pc/run/` (symlinks plus the patched exe), with no input. The game loads and runs its init without any SEH errors, then stops at the first missing intro-movie message box ("FMV_\hw_intro could not be opened"). The `orig/pc` copy has no `FMV_` folder. No input was allowed, so I did **not** reach the title or car-select screen. The patched branches themselves were not exercised at runtime.

## 3. PC data vs GameCube

Dumped with `research/pc/carlists.py` (a report-only port of `src/roster.rs` with a selectable byte order). Outputs: `research/pc/pc_carlists.txt` and `gc_carlists.txt`.

**Same structures, little-endian:**
- IMHeader id 0x2 = car select
- IMVariable 0x34 bytes: u8 type at +0, s32 min at +4, s32 max at +8
- text list: count, texts, u16 index variable at +0xA
- names end in `*`

**Car-select screens: HW_IPack 02_interface_3, 05_interface_6, 06_interface_7**

| | PC |
|---|---|
| carousel variables | #1..#7, type 0, min 0, **max 32** |
| name lists | objects 75/76/77, **36 entries**, indexed by var #4. Ids 33..35 = "HW Prototype 12*", "Power Rocket*", "Jet Threat 3*" (placeholders, same as GC) |
| unlock-hint list | object 82, 36 entries, ids 33..35 = " " |

Example PC offsets:
- if3 header 0x14C300, vars at 0x14C4F4.., names 0x15AAD0
- if6 header 0x3A7AB0, vars 0x3A7CA4.., names 0x3B62D0
- if7 header 0x3A7B50

**Difference:** GC if6 and if7 each have a **second** car-select header (0x7C9A00 / 0x7C9AC0, name objects 61/62, hint 67). PC has only one per pack.

**World** (`worlds_/sp_/drag_/race1l2.xpc`): a results header (found by shape, at 0x2275E0) has 4 name lists (objects 24..27, 36 entries) indexed by vars #1..#4, **type 1**, min 0, max 32. This matches GC (0x29FA80).

**Thumbnails:** car-select objects 5-9, 20-24 and 38-43 have type-0 objects whose sub type 3 animation has u8 +0xC = 1 and 36 keys. Key k is a texture-reloc slot (local texture).

| pack | car 0 | car 32 | **cars 33, 34, 35** |
|---|---|---|---|
| PC if3 | 0x19 | 0x39 | **0x3A, 0x3B, 0x3C** |
| PC if6 / if7 | 0x40 | 0x60 | **0x61, 0x62, 0x63** |
| GC if3 | 0x19 | 0x39 | |
| GC if6 | 0x3C | 0x5C | |

PC if6/if7 are offset by 4 from GC.

**PC texture header:** 0x10-byte slots at `texture_data_offset`, laid out as `u16 format, u16 w, u16 h, u16 mips, u32 pixel offset (pointer slot +8), u32 0`.
- Thumbnails are 128x128 with 8 mips.
- Real cars use **format 5 = DXT5** (21888 B with mips).
- Placeholders 33..35 use **format 4 = DXT1 RGB, no alpha** (10944 B).

The exe's upload code (0x467B68) uses `fmt==4 ? 0x83F0 (COMPRESSED_RGB_S3TC_DXT1) : 0x83F3 (DXT5)` with `glCompressedTexImage2D`. Replacement thumbnails should be written as format 5 (DXT5) with the full mip chain, or as format 4 if no alpha is needed.

## 4. Uncertainties

- The patch was not verified at runtime past the FMV message box.
- The AI only drives the new cars if a world tile car's maxVehicle (+0x37) is raised to 35. This is data, not exe.
- Whether a world tile car or the "random" path can produce an id up to maxVehicle, given the HW_Cars TOC.
- Persisting pad5A depends on the PC save writing the whole 0xF4-byte struct. This is assumed from the layout match and was not traced.
- The 50-entry TOC cache is unchecked, so there are at most 50 entries per pack.
- I only mapped the formats 4 and 5 that are used here. Other PC texture formats exist (the upload function has more branches).
