# Porting PS2-prototype audio to the GameCube build — progress log

Goal: get the PS2 prototype's briefing-dialogue audio into the shipped
GameCube game so the retail engine plays the prototype's takes. Target: the
fourteen `AUDIO_/HWB_M01..M14` archives, which exist in both builds.
Test bed: `HWVX-SDL` (build, then run with `--data HWVX-SDL/orig/files`).

The decision that shapes everything: **the object graph is already the
same in both builds; only the audio encoding differs.** An audio pack is a
`SoundSeqInfo` key track — the format has no textures, no models, no
platform-specific data layout beyond byte order — so the port is the same
two steps the collision converter uses for its inline sounds: field-swap
the graph with the game's own claim map, rebuild the sound region
(VAG ADPCM → big-endian PCM16). Everything the loader keys on (TOC ids,
references, pack order) survives untouched.

## 1. What the archives hold (2026-11-03)

Fourteen pairs, same names in both builds. TOC ids are the audio number
from the pack name (`audio_7` → id 7), and the ids are *not* sequential:
GameCube `HWB_M04` slot 7 is `audio_16` with id 16, so pairing must go by
id, never by slot. The full survey (149 paired, 25 prototype-only,
16 GameCube-only) is the table in `README.md`; `tests/orig_audio_port.rs`
asserts it against the real archives.

Every paired pack turns out to be the same shape in both builds: one root
entry, one sequence, one key, one sound — a single line of dialogue or one
stinger. The prototype's extra 25 tracks are additional dialogue lines the
shipped game's interface packs never reference.

## 2. The audio pack layout (2026-11-03)

Decoded against `SoundSequence.h` (the structs with their static asserts)
and `Claim_SoundSeq` in `Port_Swap.c` (the retail load-time swapper's own
map for packType 9), then verified field by field on a `HWB_M01` pair
(`audio_2`: GC 240 B graph vs prototype 140 B — the difference is heap
padding, zero-filled, not reproduced):

```
data.bin base:   u32 count; PTR entries;        entry = [PTR SoundSeqInfo, u32 id]
SoundSeqInfo:    0x00 u32 duration              0x04 u8 allPlayers, u8 maxInstances
                 0x06 u16 durationRange         0x08 u32 numKeys   0x0C PTR keys
SoundSeqKey:     0x00 f32 duration              0x04 u8 loopStart + 3 pad
                 0x08 u32 numSounds             0x0C PTR { PTR SoundSeqSound }
SoundSeqSound:   0x00 u8 type + 3 pad, u8 owner + 3 pad
                 0x08 f32 volume  0x0C f32 pitch  0x10 f32 pitchRange
                 0x14 u32 sample id (sound reloc) 0x18 PTR(f32) { near, far }
```

Both builds use exactly the same pointer slots (`0x4`, entry+0,
sequence+0xC, key+0xC, each list entry, and `Sound+0x18` when the range is
present) and the same sound reloc slot (`Sound+0x14`) with pack-local
values, so `pointers.bin` and `sound_relocs.bin` carry over unchanged —
the converter asserts both tables match its own walk instead of trusting
that. The claim map only needs the byte fields: two bytes at `+4` and a
u16 at `+6` in the sequence, four raw bytes at `+4` in the key (loopStart
is a u8, the comment in `Port_Swap.c` calls it *active*), and the raw
first eight bytes of every sound. Everything else swaps as a word.

Faithful to the claim map, the raw fields are **not value-rewritten** even
where the builds disagree in *content* (the GC donor has allPlayers=1, the
prototype 0): `SoundSequence.c:163` reads `!seq->allPlayers` only to skip
playback in split-screen, so the prototype's value is harmless — and
guessing at value semantics is how ports go wrong.

## 3. The sound region (2026-11-03)

The 32-byte descriptor table + samples region is the one format the
`convert::sound` module already owns from the car and collision work: the
prototype stores headerless VAG blocks (loop start = byte offset of a
block), GameCube stores big-endian PCM16 32-byte aligned (`sound_pointers`
holding the descriptor slots). Rebuilding the region with
`build_gc_region` is a single call; the region-relative `sfx_offset` stays
0 and the `sound_data`/`sound_pointers` extract sections are rewritten.

## 4. What the converter rejects (2026-11-03)

None of it occurs in the 149 paired packs, but the converter refuses
rather than silently dropping: textures, palettes, shaders, triggers or
inline songs in an audio pack header; relocs that address a shared pack or
run past the pack's own sound table; an empty object blob. The join →
split round-trip through the GameCube pack layout runs before anything is
written.

## 5. Decisions taken (2026-11-03)

* **Pair strictly by TOC id.** The game reaches sequences through pointer
  refs stored in interface packs, relative to the pack they live in; an
  installed sequence must sit at its partner's id in its partner's archive.
* **Skip the 25 prototype-only ids** (reported by the CLI, asserted as a
  count by the test). Playing them needs the prototype's interface packs,
  which reference them by pointer — that is the interface milestone's
  problem, not the audio converter's. Installing new TOC ids would also
  re-relativise every pack offset in the archive for content nothing in
  the retail UI would trigger.
* **Keep the 16 GameCube-only packs** (`audio_0` hosts, `audio_16..18`,
  `audio_113..118`) untouched — they carry retail-only content
  (`audio_113..118` accompany the retail briefing/movie content, which is
  out of scope).
* **References from the GameCube donor** with `name_raw`/`runtime_slot`
  cleared, as in the collision converter: the prototype's stored reference
  names are garbage ('cccc...').
* **Root entry ids normalized to the paired pack's id.** Verified: both
  builds already store the audio number there, so the fix-up fires zero
  times on real data and only guards strays.

## 6. Results (2026-11-03)

`convert::audio` + `tests/orig_audio_port.rs`: all 149 paired packs
convert, re-split GameCube-clean, and re-walk with identical
sequence/key/sound counts, complete pointer and reloc tables, correct root
ids, donor references, and rebuilt PCM16 descriptors (rate > 0, loop byte
offsets, samples inside the region, stored bytes = decoded samples × 2).
End-to-end CLI run: all 28 archives extracted, 14 `port-audio` runs —
149 packs, 149 sequences, 23 988 860 samples (~48 MB of PCM16) installed —
14 archives repacked and `hwvx-x verify --platform gc` clean on every one,
then repacked over the fourteen game-tree archives (retail originals kept
as `HWB_Mxx.xgc.bak`; the installed files are MD5-identical to the recipe
output).

## 7. Boot check (2026-11-03)

With the converted archives installed, the game boots through the three
logo movies into the title menu, and scripted `HWVX_AUTOINPUT` input takes
it from there into story mode, through a race on the converted CH1_L2
world (which loads the ported world packs mid-run), past the win, and into
a second race — no crash, no new errors.

The `Audio :: FreeA_ - Invalid pointer: 7xxxxx` messages that print at
audio init are **pre-existing**, not caused by the converted archives:
booting with the retail `.bak` archives swapped back in prints the same
seven-message pattern at the same point.

The audible check — *hearing* the prototype dialogue — is deferred, and
honestly reported as such: a type-9 data-pack load (`data pack HWB_Mxx:
type 9`, logged by `DataPack_LoadFile`) never fires in any boot observed,
with or without the port. Those loads are only requested from
`InterfaceManager.c:771`, i.e. from sound lists inside *interface* pack
headers, and the retail menu/race flow never touches one. The only game
file referencing the `HWB_Mxx` names is `HW_brief.xgc` (14 interface
packs — the briefing screens), which the retail build never loads by name
either; getting the briefing interfaces up is the interface milestone, and
that is where this audio becomes audible.
