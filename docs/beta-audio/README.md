# Porting PS2-prototype audio to the GameCube game

The leaked PS2 prototype ships fourteen briefing-dialogue audio archives,
`AUDIO_/HWB_M01.XPS` … `HWB_M14.XPS`, that pair one-to-one with the shipped
GameCube archives `audio_/HWB_M01.xgc` … `HWB_M14.xgc`. Each archive holds
*sound sequence* packs (pack type 9): a `SoundSeqInfo` key track that plays
lists of `SoundSeqSound` records against one streamed sound. `hwvx-x
port-audio` converts the prototype packs and installs them into the
GameCube extracts so the shipped engine replays the prototype's dialogue
and sound effects with prototype timing and content.

The approach is **content replacement inside the retail host**: every
GameCube audio pack keeps its TOC id and its references, so the interface
packs that point at the sequences keep finding them; only the sequence
graph's *data* (durations, sample ids, volumes) and the sound region (the
actual audio) change.

* `PROGRESS.md` – the work log: what was found, what was decided, and why.
* The container format (XPS/XGC archives: header, table of contents, how
  `extract`/`pack` build, access and store them) is documented in the
  [root README](../../README.md#format-notes); this file documents the
  audio-specific contract on top of it.

## Recipe

```sh
cargo build --release
B=target/release/hwvx-x

# 1. unpack all fourteen archive pairs (each extract goes in its own folder)
mkdir -p work/audio/gc work/audio/beta
for m in 01 02 03 04 05 06 07 08 09 10 11 12 13 14; do
  $B extract orig/gc/files/audio_/HWB_M$m.xgc                 work/audio/gc/HWB_M$m
  $B extract --platform ps2-beta orig/ps2-beta/XPS/ENGLISH/AUDIO_/HWB_M$m.XPS work/audio/beta/HWB_M$m
done

# 2. convert every pair (rewrites the GameCube extracts in place; the
#    original pack folders are kept next to them as <name>.bak)
for m in 01 02 03 04 05 06 07 08 09 10 11 12 13 14; do
  $B port-audio work/audio/gc/HWB_M$m --beta work/audio/beta/HWB_M$m
done

# 3. rebuild the archives into the game's data tree (backup first)
D=HWVX-SDL/orig/files
for m in 01 02 03 04 05 06 07 08 09 10 11 12 13 14; do
  cp -n $D/audio_/HWB_M$m.xgc $D/audio_/HWB_M$m.xgc.bak
  $B pack --force work/audio/gc/HWB_M$m $D/audio_/HWB_M$m.xgc
done
```

Then run HWVX-SDL with `--data HWVX-SDL/orig/files` and watch a briefing or
race intro: the dialogue speaks with the prototype's sounds. Undo with the
`.bak` copies: `cp` them back over the `.xgc` files.

The whole pair set converts in one pass: **149 prototype packs** (one
sequence, one key, one sound each) are installed over their GameCube
partners, and 25 prototype-only ids are reported and skipped (see *The
matching contract*).

## What `port-audio` does

For each GameCube audio pack in the extract it finds the prototype pack, in
the `--beta` extracts, with the same TOC id (the audio number), and calls
`convert::audio`:

1. **Byte-swaps the sequence graph** exactly as the game's own load-time
   swapper does (`Claim_SoundSeq` in `Port_Swap.c`): the object layout is
   identical in both builds — root table `{ u32 count; PTR entries }`,
   entries `[PTR SoundSeqInfo, u32 id]`, `SoundSeqInfo` (duration, all-
   players/max-instance bytes, duration-range u16, key track), keys
   (duration f32, loop-start byte, sound list), sounds (type/owner bytes,
   volume/pitch/pitch-range f32, sample id, range pointer) — so the graph
   converts by swapping every word and claiming the raw byte fields. The
   `pointers.bin`, `sound_relocs.bin` and reference tables are carried over.
2. **Rebuilds the sound region** from prototype VAG ADPCM to GameCube
   big-endian PCM16 (`convert::sound`, the same path the car and collision
   converters use): 32-byte descriptors at `sfx_offset = 0`, samples 32-byte
   aligned, loop start as a byte offset, rate and gain from the prototype's
   VAG headers. The `sound_pointers` table is rebuilt from the new slots.
3. **Normalizes the root entry ids** to the GameCube pack id (both builds
   already store the audio number there, so this only fixes strays) and
   **takes the pack references from the GameCube donor** — the prototype's
   stored reference names are garbage ('cccc...').
4. **Re-validates** the result by joining and re-splitting it as a GameCube
   pack before writing.

Nothing else moves: TOC ids, pack order and the archive layout are exactly
the retail ones, so `pack` rebuilds a byte-shape the loader already accepts.

## The matching contract

Audio packs are found by **TOC id** — the audio number in the pack name
(`audio_7` has id 7). Interface content refers to sequences by pointer into
the streamed audio archive (`InterfaceManager.c`, `Action.c` →
`SoundSequence_Play`), and the pointer fixup resolves against the pack the
sequence was loaded from, so an installed pack must keep its partner's id.
`port-audio` pairs strictly by id and never renumbers.

The TOC survey of all fourteen pairs (also asserted by
`tests/orig_audio_port.rs`):

| Archive | GC packs | Prototype packs | Paired | Prototype-only ids | GameCube-only ids |
|---------|---------:|----------------:|-------:|--------------------|-------------------|
| HWB_M01 | 8  | 7  | 7  | — | 0 |
| HWB_M02 | 5  | 5  | 5  | — | — |
| HWB_M03 | 13 | 16 | 13 | 14–16 | — |
| HWB_M04 | 8  | 6  | 6  | — | 0, 16 |
| HWB_M05 | 10 | 10 | 10 | — | — |
| HWB_M06 | 11 | 18 | 10 | 11–18 | 0 |
| HWB_M07 | 20 | 14 | 13 | 14 | 0, 113–118 |
| HWB_M08 | 12 | 12 | 12 | — | — |
| HWB_M09 | 14 | 18 | 14 | 15–18 | — |
| HWB_M10 | 13 | 11 | 11 | — | 17, 18 |
| HWB_M11 | 12 | 18 | 12 | 13–18 | — |
| HWB_M12 | 17 | 18 | 17 | 14 | — |
| HWB_M13 | 8  | 5  | 5  | — | 16–18 |
| HWB_M14 | 14 | 16 | 14 | 15, 16 | — |
| **Total** | **165** | **174** | **149** | **25** | **16** |

* The 25 **prototype-only** tracks are extra dialogue in the prototype's
  briefing sets. They are not installed: a sequence is played through a
  pointer stored in the *interface* packs, so a track nothing references
  would never play, and installing new ids would mean editing TOCs and
  re-relativising every pointer in the archive. They come back with the
  interface port (the prototype's `HW_IPACK` content references them).
* The 16 **GameCube-only** packs (`audio_0` archive hosts, the `audio_16..18`
  and `audio_113..118` additions) keep their retail content untouched.

## Useful while working

* `hwvx-x info <archive or folder>` — pack list with TOC ids (`audio_N`).
* `hwvx-x verify orig/` — proves unmodified archives still rebuild byte for
  byte (only run against untouched extracts).
* `tests/orig_audio_port.rs` — pairs all 14 archives, converts all 149
  packs, re-splits each as GameCube-clean, re-walks the big-endian graph
  (same sequence/key/sound counts, complete pointer and reloc tables, root
  ids correct) and checks every rebuilt descriptor (PCM16, rate, loop,
  region bounds).

## Restoring

The retail GameCube archives are unchanged under `orig/`; the game's copies
under `HWVX-SDL/orig/files/audio_/` have `.bak` twins. To go back: restore
the `.bak` files — the archives *are* the game data, nothing else needs
rebuilding.
