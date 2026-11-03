//! End-to-end check of the `port-audio` conversions against the original
//! archives (skipped when `orig/` is not present): for every `HWB_Mxx`
//! audio archive pair (M01..M14) the prototype and GameCube packs pair by
//! TOC id, every prototype pack with a GameCube partner converts to a
//! clean GameCube audio pack - same object graph (walk counts, pointer and
//! reloc tables unchanged), root ids carrying the audio number, and a
//! rebuilt big-endian PCM16 sound region.
//!
//! Run with `cargo test --release`.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use hwvx_xfile::archive::Archive;
use hwvx_xfile::blob::Blob;
use hwvx_xfile::convert::audio;
use hwvx_xfile::format::{Endian, PackType, Platform, TYPE_AUDIO};
use hwvx_xfile::pack;

const BE: Endian = Endian::Big;

fn split_all(path: &Path, platform: Platform) -> Vec<(u32, pack::SplitPack)> {
    let bytes = fs::read(path).unwrap_or_else(|_| panic!("missing {}", path.display()));
    let ar = Archive::parse(&bytes, platform).unwrap_or_else(|err| panic!("{}: {err:#}", path.display()));
    ar.entries
        .iter()
        .filter(|e| e.kind.0 == TYPE_AUDIO)
        .map(|e| {
            let sp = pack::split(platform, e.kind, &e.data).unwrap_or_else(|err| panic!("{} id {}: {err:#}", path.display(), e.id));
            (e.id, sp)
        })
        .collect()
}

#[test]
fn beta_audio_packs_convert_end_to_end() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("orig");
    if !root.join("gc/files/audio_/HWB_M01.xgc").exists() || !root.join("ps2-beta/XPS/ENGLISH/AUDIO_/HWB_M01.XPS").exists() {
        eprintln!("orig/ audio files not found, skipped");
        return;
    }

    let mut paired = 0usize;
    let mut beta_only = 0usize;
    for m in 1..=14 {
        let gc_path = root.join(format!("gc/files/audio_/HWB_M{m:02}.xgc"));
        let beta_path = root.join(format!("ps2-beta/XPS/ENGLISH/AUDIO_/HWB_M{m:02}.XPS"));
        let gc = split_all(&gc_path, Platform::Gc);
        let beta = split_all(&beta_path, Platform::Ps2Beta);
        for (name, list) in [("gc", &gc), ("beta", &beta)] {
            let ids: BTreeSet<u32> = list.iter().map(|(id, _)| *id).collect();
            assert_eq!(ids.len(), list.len(), "HWB_M{m:02} {name}: duplicate TOC ids");
        }
        for (id, bsp) in &beta {
            let Some((_, gsp)) = gc.iter().find(|(gid, _)| gid == id) else {
                beta_only += 1;
                continue;
            };
            paired += 1;
            let (out, rep) = audio::convert(bsp, &audio::Options { donor: gsp, platform: Platform::Gc, seq_id: *id })
                .unwrap_or_else(|err| panic!("HWB_M{m:02} audio_{id}: {err:#}"));
            assert_eq!(rep.pointers_added, 0, "HWB_M{m:02} audio_{id}: prototype pointer table was incomplete");
            assert_eq!(rep.sub_ids_fixed, 0, "HWB_M{m:02} audio_{id}: root id was not the audio number");

            // Re-split the joined pack with the GameCube reader: the layout
            // must be retail-clean, and walking it must find the same graph.
            let bytes = pack::join(Platform::Gc, &out).unwrap();
            let sp = pack::split(Platform::Gc, PackType(TYPE_AUDIO), &bytes).unwrap();
            let blob = Blob::from_split(BE, &sp);
            let w = audio::walk(&blob, BE).unwrap();
            assert_eq!((w.sequences, w.keys, w.sounds), (rep.sequences, rep.keys, rep.sounds), "HWB_M{m:02} audio_{id}: graph counts");
            for slot in w.pointers.iter().chain(w.root_ids.iter()) {
                assert!(blob.data.len() >= *slot as usize + 4, "HWB_M{m:02} audio_{id}: slot {slot:#x} past the blob");
            }
            for &slot in &w.pointers {
                assert!(blob.pointers.contains(&slot), "HWB_M{m:02} audio_{id}: pointer slot {slot:#x} not in pointers.bin");
            }
            assert_eq!(w.reloc_slots.len(), blob.sound_relocs.len(), "HWB_M{m:02} audio_{id}: reloc table size");
            for r in &blob.sound_relocs {
                assert!(w.reloc_slots.contains(&r.slot), "HWB_M{m:02} audio_{id}: reloc slot {:#x} is not a sound id slot", r.slot);
                assert_eq!(r.pack(), 0, "HWB_M{m:02} audio_{id}: reloc {} names shared pack {}", r.index(), r.pack());
                assert!(!rep.notes.iter().any(|n| n.starts_with("the walk found")), "HWB_M{m:02} audio_{id}: {}", rep.notes.join("; "));
            }
            for &slot in &w.root_ids {
                assert_eq!(blob.u32(BE, slot), Some(*id), "HWB_M{m:02} audio_{id}: root id at {slot:#x}");
            }

            // The references came from the GameCube donor, names cleared.
            let pack::PackHeader::Standard(h) = &sp.json.header else { panic!("HWB_M{m:02} audio_{id}: standard header") };
            let gh = match &gsp.json.header {
                pack::PackHeader::Standard(h) => h,
                _ => panic!("GameCube audio pack header"),
            };
            assert_eq!(h.refs, gh.refs, "HWB_M{m:02} audio_{id}: references");
            for r in h.refs.iter().filter(|r| !r.name_raw.0.is_empty()) {
                panic!("HWB_M{m:02} audio_{id}: raw reference name {:?}", r.name_raw);
            }
            assert_eq!((h.num_textures, h.num_palettes, h.num_shaders, h.num_triggers, h.song_offset), (0, 0, 0, 0, 0), "HWB_M{m:02} audio_{id}: foreign fields");

            // Sound region: one GC descriptor per prototype sound, samples
            // totaling exactly what the decode produced.
            assert_eq!(h.num_sounds as usize, count_sounds(bsp), "HWB_M{m:02} audio_{id}: sound count");
            assert_eq!(h.sfx_offset, 0, "HWB_M{m:02} audio_{id}: sfx_offset");
            let rd = sp.sections.get("sound_data").cloned().unwrap_or_default();
            let pts = sp.sections.get("sound_pointers").cloned().unwrap_or_default();
            assert_eq!(pts.len(), h.num_sounds as usize * 4, "HWB_M{m:02} audio_{id}: sound pointer table");
            let mut stored = 0usize;
            for i in 0..h.num_sounds as usize {
                let d = i * 0x20;
                assert_eq!(BE.u32_at(&pts, i * 4).unwrap(), d as u32, "HWB_M{m:02} audio_{id}: sound pointer {i}");
                let (ptr, len, lp, rate) = (
                    BE.u32_at(&rd, d).unwrap() as usize,
                    BE.u32_at(&rd, d + 4).unwrap() as usize,
                    BE.u32_at(&rd, d + 8).unwrap(),
                    BE.u32_at(&rd, d + 0xC).unwrap(),
                );
                assert!(ptr + len <= rd.len(), "HWB_M{m:02} audio_{id}: sound {i} samples past the region");
                assert_eq!(rd[d + 0x11], 16, "HWB_M{m:02} audio_{id}: sound {i}: not PCM16");
                assert!(rate > 0, "HWB_M{m:02} audio_{id}: sound {i}: rate");
                assert!(lp == 0xFFFF_FFFF || ((lp as usize) % 2 == 0 && (lp as usize) < len), "HWB_M{m:02} audio_{id}: sound {i}: loop {lp:#x} / len {len:#x}");
                stored += len;
            }
            assert_eq!(stored, rep.samples * 2, "HWB_M{m:02} audio_{id}: region samples vs decoded");
        }
    }
    // TOC survey of the 14 archive pairs: 149 ids appear in both builds,
    // 25 are prototype-only (extra audio_14..18 tracks), 16 are GC-only
    // (audio_0 hosts and the audio_16..18 / audio_113..118 additions).
    assert_eq!(paired, 149, "paired packs (survey: docs/beta-audio)");
    assert_eq!(beta_only, 25, "prototype-only ids");
}

/// The prototype pack's sound count, straight from its header.
fn count_sounds(sp: &pack::SplitPack) -> usize {
    match &sp.json.header {
        pack::PackHeader::Standard(h) => h.num_sounds as usize,
        _ => 0,
    }
}
