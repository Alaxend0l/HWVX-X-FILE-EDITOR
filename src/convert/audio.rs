//! Convert a PS2-prototype audio pack (type 9) into a GameCube one.
//!
//! An audio pack holds briefing-dialogue sequences
//! (`InterfaceManager_MoveDialogSystem` -> `SoundSequence_Play`): a root
//! table `{ u32 count; PTR entries }` at the base of `data.bin` whose
//! entries are `[PTR SoundSeqInfo, u32 id]`, each sequence a key track of
//! keys that each play a list of `SoundSeqSound` records
//! (`SoundSequence.h`):
//!
//! ```text
//! SoundSeqInfo  0x00 u32 duration, 0x04 u8 allPlayers, 0x05 u8 maxInstances,
//!               0x06 u16 durationRange, 0x08 u32 numKeys, 0x0C PTR keys
//! SoundSeqKey   0x00 f32 duration, 0x04 u8 loopStart + 3 pad,
//!               0x08 u32 numSounds, 0x0C PTR { PTR SoundSeqSound }
//! SoundSeqSound 0x00 u8 type + 7 pad, 0x08 f32 volume, 0x0C f32 pitch,
//!               0x10 f32 pitchRange, 0x14 u32 sample id (a sound reloc),
//!               0x18 PTR(f32) { near, far }, 0x1C pad word
//! ```
//!
//! The object graph is byte-for-byte the same shape in the prototype and
//! retail GameCube builds (verified on `HWB_M01`/`HWB_M04` extracts: same
//! root/sequence/key/sound layout, the same six pointer slots per sequence,
//! `Sound + 0x14` reloc slots with pack-local sound ids, and the range
//! pointer at `Sound + 0x18` listed in `pointers.bin`), so the graph
//! converts by swapping each field with the exact claim map from
//! `Claim_SoundSeq` in `Port_Swap.c`: `SoundSeqInfo + 4` is two raw bytes
//! followed by a u16, `SoundSeqKey + 4` four raw bytes, and `SoundSeqSound`
//! starts with eight raw bytes; every other byte is a 32-bit word.
//!
//! What actually changes:
//! * the sound region. The prototype stores headerless VAD/VAG ADPCM; the
//!   GameCube stores big-endian PCM16. Every sound is decoded and re-stored
//!   through `sound::build_gc_region`, exactly like the collision converter
//!   rebuilds its inline ground sounds. The `sound_pointers` table is
//!   rebuilt from the new descriptor slots.
//! * the root entry `id` is normalized to the GameCube pack id. Both builds
//!   already store the audio number there (verified: `audio_2` roots carry
//!   id 2, `HWB_M04/audio_16` carries 16), so this only fixes strays.
//! * the pack references come from the GameCube donor: the prototype's
//!   stored names are garbage ('cccc...').
//!
//! The sound relocs are kept as they are: both builds use pack-local
//! values (high 16 bits zero) that the loader resolves through the pack's
//! own sound table, and the rebuilt table preserves the order.

use std::collections::BTreeSet;

use anyhow::{bail, ensure, Result};

use crate::blob::{Blob, SwapMap};
use crate::convert::sound;
use crate::format::{align, Endian, Platform};
use crate::pack::{PackHeader, SplitPack, StdHeader};

const LE: Endian = Endian::Little;

/// Everything the converter needs besides the prototype pack.
pub struct Options<'a> {
    /// GameCube retail audio pack with the same id: supplies the references.
    pub donor: &'a SplitPack,
    /// Target build (GameCube).
    pub platform: Platform,
    /// The audio number the root entries carry (the paired pack's id).
    pub seq_id: u32,
}

/// What happened, for the user.
#[derive(Default, Debug)]
pub struct Report {
    pub sequences: usize,
    pub keys: usize,
    pub sounds: usize,
    pub samples: usize,
    pub pointers_added: usize,
    pub sub_ids_fixed: usize,
    pub notes: Vec<String>,
}

fn std_header(sp: &SplitPack) -> Result<&StdHeader> {
    match &sp.json.header {
        PackHeader::Standard(h) => Ok(h),
        PackHeader::Quad(_) => bail!("expected an audio pack, found a geometry pack"),
    }
}

/// The layout map plus what the walk found, mirroring `Claim_SoundSeq` and
/// `Claim_Roots` (packType 9) in `Port_Swap.c`.
pub struct Walk {
    swap: SwapMap,
    pub sequences: usize,
    pub keys: usize,
    pub sounds: usize,
    /// Root entry id slots (`entry + 4`).
    pub root_ids: Vec<u32>,
    /// Pointer slots the graph uses: `0x4`, root entry + 0, sequence + 0xC,
    /// key + 0xC, every list entry, and `Sound + 0x18` when it is set.
    pub pointers: Vec<u32>,
    /// Sound sample-id slots (`Sound + 0x14`), the sound reloc slots.
    pub reloc_slots: Vec<u32>,
}

/// Walk the roots and every sequence under them, claiming the raw byte
/// fields; everything the claim map does not mention swaps as a word.
pub fn walk(blob: &Blob, e: Endian) -> Result<Walk> {
    let data = &blob.data;
    let mut w = Walk {
        swap: SwapMap::new(data.len()),
        sequences: 0,
        keys: 0,
        sounds: 0,
        root_ids: vec![],
        pointers: vec![],
        reloc_slots: vec![],
    };
    if data.len() < 8 {
        return Ok(w);
    }
    let get = |off: u32| blob.u32(e, off);
    let fits = |off: u32, len: u32| (off as usize) + (len as usize) <= data.len();

    w.pointers.push(0x4);
    let (n, table) = (get(0).unwrap_or(0), get(4).unwrap_or(!0));
    for i in 0..n.min(0x1000) {
        let entry = match table.checked_add(i * 8) {
            Some(e) if fits(e, 8) => e,
            _ => break,
        };
        w.pointers.push(entry);
        w.root_ids.push(entry + 4);
        w.swap.word(entry + 4);
        let Some(seq) = get(entry) else { break };
        if seq == !0 {
            continue;
        }
        claim_seq(blob, e, &mut w, fits, seq);
    }
    Ok(w)
}

/// One sequence: params, key track, sound lists. `fits` bounds-checks like
/// `InBlob`; the C walker's iteration caps (0x1000 keys, 0x100 sounds)
/// guard against corrupt counts.
fn claim_seq(
    blob: &Blob,
    e: Endian,
    w: &mut Walk,
    fits: impl Fn(u32, u32) -> bool,
    seq: u32,
) {
    if !fits(seq, 0x10) {
        return;
    }
    let get = |off: u32| blob.u32(e, off);
    w.swap.bytes(seq + 4, 2); // allPlayers, maxInstances
    w.swap.half(seq + 6); // durationRange
    w.sequences += 1;

    let keys = get(seq + 0xC).unwrap_or(!0);
    w.pointers.push(seq + 0xC);
    let nk = get(seq + 8).unwrap_or(0);
    for i in 0..nk.min(0x1000) {
        let key = match keys.checked_add(i * 0x10) {
            Some(k) if keys != !0 && fits(k, 0x10) => k,
            _ => break,
        };
        w.keys += 1;
        w.swap.bytes(key + 4, 4); // loopStart + pad
        w.pointers.push(key + 0xC);
        let list = get(key + 0xC).unwrap_or(!0);
        let ns = get(key + 8).unwrap_or(0);
        for j in 0..ns.min(0x100) {
            let slot = match list.checked_add(j * 4) {
                Some(s) if list != !0 && fits(s, 4) => s,
                _ => break,
            };
            w.pointers.push(slot);
            let snd = match get(slot) {
                Some(s) if s != !0 => s,
                _ => continue,
            };
            if !fits(snd, 0x1C) {
                continue;
            }
            w.sounds += 1;
            w.swap.bytes(snd, 8); // type, owner, pad
            w.reloc_slots.push(snd + 0x14); // sample id: a sound reloc
            if get(snd + 0x18).unwrap_or(!0) != !0 {
                w.pointers.push(snd + 0x18); // range { near, far }
            }
        }
    }
}

/// Convert one prototype audio pack into a GameCube one.
pub fn convert(beta: &SplitPack, opt: &Options) -> Result<(SplitPack, Report)> {
    let mut report = Report::default();
    let target = opt.platform;
    ensure!(target == Platform::Gc, "audio packs convert to GameCube only");
    let te = target.endian();

    let h = std_header(beta)?.clone();
    for (what, n, off) in [
        ("texture", h.num_textures, h.texture_data_offset),
        ("palette", h.num_palettes, h.palette_offset),
        ("shader", h.num_shaders, h.shader_offset),
        ("trigger", h.num_triggers, h.trigger_offset),
    ] {
        ensure!(
            n == 0 && off == 0,
            "prototype audio packs with {what}s are not supported ({n} at {off:#x})"
        );
    }
    ensure!(
        h.song_offset == 0 && h.song_bank_offset == 0,
        "prototype audio packs with inline songs are not supported"
    );
    ensure!(!beta.sections.contains_key("song_data"), "prototype audio packs with a song section are not supported");

    let mut blob = Blob::from_split(LE, beta);
    ensure!(!blob.data.is_empty(), "audio pack has no object data");

    // ---- 1. map the layout, read the sounds that get rebuilt -----------------
    let w = walk(&blob, LE)?;
    report.sequences = w.sequences;
    report.keys = w.keys;
    report.sounds = w.sounds;
    if w.sequences == 0 {
        report.notes.push("the root walk found no sequences".into());
    }

    let region = beta.sections.get("sound_data").cloned().unwrap_or_default();
    let sounds = if h.num_sounds > 0 {
        sound::read_beta_sounds(&region, h.sfx_offset as usize, h.num_sounds as usize)?
    } else {
        vec![]
    };
    report.samples = sounds.iter().map(|s| s.samples.len()).sum();
    ensure!(sounds.len() <= u16::MAX as usize, "too many sounds for a reloc id");

    // The sample ids are pack-local in both builds; keep them, but make
    // sure they address this pack's own table, which the rebuild preserves.
    for r in &blob.sound_relocs {
        ensure!(r.index() < h.num_sounds, "sound reloc {} is past the pack's {num} sounds", r.value, num = h.num_sounds);
        if r.pack() != 0 {
            report.notes.push(format!("sound reloc {} names shared pack {}", r.index(), r.pack()));
        }
    }
    let walked: BTreeSet<u32> = w.reloc_slots.iter().copied().collect();
    let table: BTreeSet<u32> = blob.sound_relocs.iter().map(|r| r.slot).collect();
    if walked != table {
        report.notes.push(format!(
            "the walk found {} sound id slots, the reloc table lists {}",
            walked.len(),
            table.len()
        ));
    }

    // ---- 2. byte-swap the object graph ---------------------------------------
    if te != LE {
        w.swap.swap(&mut blob.data);
    }

    // ---- 3. normalize the root ids, keep the pointer table complete ----------
    for &slot in &w.root_ids {
        if blob.u32(te, slot) != Some(opt.seq_id) {
            blob.put_u32(te, slot, opt.seq_id);
            report.sub_ids_fixed += 1;
        }
    }
    for &slot in &w.pointers {
        if blob.pointers.insert(slot) {
            report.pointers_added += 1;
        }
    }

    // ---- 4. header ------------------------------------------------------------
    blob.data.resize(align(blob.data.len(), 32), 0);
    blob.validate(te)?;

    let mut out = beta.clone();
    out.json.padding.clear();
    out.sections.clear();
    blob.store(te, &mut out);
    if !sounds.is_empty() {
        let (snd_region, snd_ptrs) = sound::build_gc_region(&sounds)?;
        out.sections.insert("sound_data".into(), snd_region);
        let mut p = vec![];
        for v in snd_ptrs {
            te.push(&mut p, v);
        }
        out.sections.insert("sound_pointers".into(), p);
    }
    if let PackHeader::Standard(hh) = &mut out.json.header {
        hh.num_sounds = sounds.len() as u32;
        hh.sfx_offset = 0;
        hh.num_textures = 0;
        hh.texture_data_offset = 0;
        hh.num_palettes = 0;
        hh.palette_offset = 0;
        hh.num_shaders = 0;
        hh.shader_offset = 0;
        hh.num_triggers = 0;
        hh.trigger_offset = 0;
        hh.song_offset = 0;
        hh.song_bank_offset = 0;
        hh.refs.clone_from(&std_header(opt.donor)?.refs);
        for r in hh.refs.iter_mut() {
            r.name_raw = Default::default();
            r.runtime_slot = 0;
        }
    }
    out.describe(target);
    // must survive the target's pack layout
    let bytes = crate::pack::join(target, &out)?;
    crate::pack::split(target, crate::format::PackType(9), &bytes)?;
    Ok((out, report))
}
