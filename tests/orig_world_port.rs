//! End-to-end check of the `port-world` conversions against the original
//! archives (skipped when `orig/` is not present): every PS2-prototype
//! CH1_L2 quad pack converts to a GameCube quad pack, the prototype world
//! texture pack converts against its GameCube donor, and the GameCube world
//! pack's quadrant table is patched to point at the converted packs exactly
//! the way the streaming loader matches them (`packId == (geoms << 16 |
//! quadId)`, mirrored in the archive TOC by the manifest `sub_id`).
//!
//! Run with `cargo test --release`.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use hwvx_xfile::archive::Archive;
use hwvx_xfile::blob::Blob;
use hwvx_xfile::convert::{geometry, world};
use hwvx_xfile::format::{Endian, PackType, Platform, TYPE_GEOMETRY, TYPE_TEXTURE, TYPE_WORLD};
use hwvx_xfile::pack;

const BE: Endian = Endian::Big;

fn split_all(path: &Path, platform: Platform, kind: u32) -> Vec<(u32, pack::SplitPack)> {
    let bytes = fs::read(path).unwrap_or_else(|_| panic!("missing {}", path.display()));
    let ar = Archive::parse(&bytes, platform).unwrap_or_else(|err| panic!("{}: {err:#}", path.display()));
    ar.entries
        .iter()
        .filter(|e| e.kind.0 == kind)
        .map(|e| {
            let sp = pack::split(platform, e.kind, &e.data).unwrap_or_else(|err| panic!("{} sub_id {:#x}: {err:#}", path.display(), e.sub_id));
            (e.sub_id, sp)
        })
        .collect()
}

#[test]
fn beta_ch1_l2_world_converts_end_to_end() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("orig");
    let gc_geo = root.join("gc/files/worlds_/geo_/w1_geo.xgc");
    let gc_tex = root.join("gc/files/worlds_/tex_/w1_tex.xgc");
    let gc_world = root.join("gc/files/worlds_/sp_/chal1_/ch1_l2.xgc");
    let beta = root.join("ps2-beta/XPS/ENGLISH/WORLDS_/SP_/CHAL1_/CH1_L2.XPS");
    if !gc_geo.exists() || !gc_tex.exists() || !gc_world.exists() || !beta.exists() {
        eprintln!("orig/ world files not found, skipped");
        return;
    }

    // The archives list quad packs in world-slot order (asserted by
    // tests/orig_worlds.rs), so positional pairing is the slot pairing.
    let gc = split_all(&gc_geo, Platform::Gc, TYPE_GEOMETRY);
    let bt = split_all(&beta, Platform::Ps2Beta, TYPE_GEOMETRY);
    assert_eq!(gc.len(), 21, "GameCube quad packs");
    assert_eq!(bt.len(), 21, "prototype quad packs");

    // Geometry id space: donor tables keep their ids positionally; extra
    // prototype models get fresh ids above every GameCube id.
    let donors: Vec<Vec<u32>> = gc.iter().map(|(_, sp)| world::donor_geom_ids(sp).unwrap()).collect();
    let all: Vec<u32> = donors.iter().flatten().copied().collect();
    let base = world::new_geom_id_base(&all);
    assert!(base >= 0x2D4C, "fresh geometry ids start at {base:#x}, expected above 0x2d4b");

    // Rebuilt model counts, per slot (the counts the prototype scan +
    // converter produce; mirrors the probe recorded in docs/beta-worlds).
    let counts = [17, 13, 15, 26, 41, 15, 38, 50, 33, 9, 4, 78, 14, 20, 8, 92, 49, 23, 44, 12, 23];

    let mut patches = Vec::new();
    let mut next_id = base;
    let mut used_ids: BTreeSet<u32> = BTreeSet::new();
    for i in 0..21 {
        let mut ids = Vec::new();
        for k in 0..counts[i] {
            let id = if k < donors[i].len() { donors[i][k] } else { let id = next_id; next_id += 1; id };
            assert!(used_ids.insert(id), "geometry id {id:#x} used twice");
            ids.push(id);
        }
        let (out, rep) = world::convert_quad(&bt[i].1, &gc[i].1, &ids)
            .unwrap_or_else(|err| panic!("quad slot {i}: {err:#}"));
        assert_eq!(rep.models, counts[i], "quad slot {i}: model count");

        // Re-split the joined pack and parse every model with the GameCube
        // reader: the layout must be retail-clean.
        let bytes = pack::join(Platform::Gc, &out).unwrap();
        let sp = pack::split(Platform::Gc, PackType(TYPE_GEOMETRY), &bytes).unwrap();
        let blob = Blob::from_split(BE, &sp);
        let rr = sp.sections.get("ref_relocs").map(Vec::len).unwrap_or(0);
        assert_eq!(rr / 8, counts[i], "quad slot {i}: table entries");
        for k in 0..counts[i] {
            let entry = 0x20 + 12 * k;
            assert_eq!(BE.u32_at(blob.data.as_slice(), entry).unwrap(), ids[k], "quad slot {i} geom {k}: id");
            let off = BE.u32_at(blob.data.as_slice(), entry + 8).unwrap();
            geometry::read_gc(&blob, off).unwrap_or_else(|err| panic!("quad slot {i} geom {k} at {off:#x}: {err:#}"));
        }

        let pack_id = (counts[i] as u32) << 16 | gc[i].0 & 0xFFFF;
        patches.push(world::QuadPatch { num_geoms: counts[i] as u32, pack_id });
    }
    assert_eq!(used_ids.len(), counts.iter().sum::<usize>(), "unique geometry ids");

    // ---- world texture pack -------------------------------------------------
    let bt_tex = split_all(&beta, Platform::Ps2Beta, TYPE_TEXTURE);
    let gc_tex = split_all(&gc_tex, Platform::Gc, TYPE_TEXTURE);
    assert_eq!(bt_tex.len(), 1);
    assert_eq!(gc_tex.len(), 1);
    let (tex, trep) = world::convert_textures(&bt_tex[0].1, &gc_tex[0].1).unwrap();
    assert_eq!(trep.textures, 153, "prototype world textures");

    // Every GameCube texture def's pixel slot is a registered pointer that
    // lands inside the blob, and the shader tail relocations moved with it.
    let bytes = pack::join(Platform::Gc, &tex).unwrap();
    let sp = pack::split(Platform::Gc, PackType(TYPE_TEXTURE), &bytes).unwrap();
    let blob = Blob::from_split(BE, &sp);
    let hwvx_xfile::pack::PackHeader::Standard(h) = &sp.json.header else { panic!("texture pack header") };
    assert_eq!(h.num_textures, 153);
    assert_eq!(h.num_palettes, 0);
    for i in 0..153 {
        let slot = h.texture_data_offset as usize + i * 0x20 + 8;
        assert!(blob.pointers.contains(&(slot as u32)), "texture {i} pixel slot {slot:#x} not a pointer");
        let t = blob.u32(BE, slot as u32).unwrap();
        assert!((t as usize) < blob.data.len(), "texture {i} pixels at {t:#x} past blob end");
    }
    assert_eq!(blob.texture_relocs.len(), 7, "shader-entry texture relocations kept");
    for r in &blob.texture_relocs {
        assert!((r.slot as usize) + 4 <= blob.data.len(), "texture reloc slot {:#x} past end", r.slot);
    }

    // ---- world pack quadrant table ------------------------------------------
    let bt_world = split_all(&beta, Platform::Ps2Beta, TYPE_WORLD);
    let gc_world = split_all(&gc_world, Platform::Gc, TYPE_WORLD);
    assert_eq!(bt_world.len(), 1);
    assert_eq!(gc_world.len(), 1);
    let wout = world::patch_world(&gc_world[0].1, &patches).unwrap();
    let bytes = pack::join(Platform::Gc, &wout).unwrap();
    let sp = pack::split(Platform::Gc, PackType(TYPE_WORLD), &bytes).unwrap();
    let w = sp.sections.get("data").expect("world data.bin");
    let n = BE.u32_at(w, 0x20).unwrap() as usize;
    let list = BE.u32_at(w, 0x24).unwrap() as usize;
    assert_eq!(n, 21);
    for i in 0..21 {
        let r = list + i * 0x50;
        assert_eq!(BE.u32_at(w, r + 0x0C).unwrap(), patches[i].num_geoms, "world slot {i}: numGeoms");
        assert_eq!(BE.u32_at(w, r + 0x38).unwrap(), patches[i].pack_id, "world slot {i}: packId");
        assert_eq!(patches[i].pack_id >> 16, patches[i].num_geoms, "world slot {i}: TOC match contract");
    }
    // Everything else in the world pack is untouched.
    let wold = gc_world[0].1.sections.get("data").unwrap();
    assert_eq!(wold.len(), w.len());
    for b in 0..w.len() {
        let in_patch = (0..21).any(|i| {
            let r = list + i * 0x50;
            (r + 0x0C..r + 0x10).contains(&b) || (r + 0x38..r + 0x3C).contains(&b)
        });
        if !in_patch {
            assert_eq!(wold[b], w[b], "world pack byte {b:#x} changed outside the quad table");
        }
    }
}
