// Scratch smoke test for the prototype -> GameCube collision converter:
// convert the beta w1_col extraction against the GameCube donor and report.
use std::path::Path;

use hwvx_xfile::convert::collision;
use hwvx_xfile::format::{Platform, PackType};
use hwvx_xfile::pack;

fn main() -> anyhow::Result<()> {
    let tmp = Path::new("/private/var/folders/y9/r_r1dhkx2mx903mtkj6y3b5c0000gn/T/opencode");
    let beta = read(&tmp.join("beta-w/00_collision_0"))?;
    let donor = read(&tmp.join("col-gc/00_collision_0"))?;
    let (out, rep) = collision::convert(
        &beta,
        &collision::Options {
            donor: &donor,
            platform: Platform::Gc,
        },
    )?;
    println!(
        "cells {} nodes {} edges {} materials {} textures {} sounds {} sound_fixes {}",
        rep.cells, rep.nodes, rep.edges, rep.materials, rep.textures, rep.sounds, rep.sound_fixes
    );
    for n in &rep.notes {
        println!("note: {n}");
    }
    let bytes = pack::join(Platform::Gc, &out)?;
    let back = pack::split(Platform::Gc, PackType(5), &bytes)?;
    // roundtrip the converted pack itself
    let again = pack::join(Platform::Gc, &back)?;
    anyhow::ensure!(again == bytes, "converted pack does not roundtrip");
    // Walking the converted pack in big-endian is the real test: a u16 or a
    // pointer that swapped the wrong way sends the BSP walk off the graph.
    let out_blob = hwvx_xfile::blob::Blob::from_split(hwvx_xfile::format::Endian::Big, &back);
    let w2 = collision::walk(&out_blob, hwvx_xfile::format::Endian::Big)?;
    println!(
        "re-walk: cells {} nodes {} edges {} materials {}",
        w2.cells, w2.nodes, w2.edges, w2.materials
    );
    anyhow::ensure!(
        (w2.cells, w2.nodes, w2.edges, w2.materials) == (rep.cells, rep.nodes, rep.edges, rep.materials),
        "the converted pack walks differently"
    );
    println!(
        "joined {} bytes; sections {:?}\n",
        bytes.len(),
        out.sections.keys().collect::<Vec<_>>()
    );
    // art that was rebuilt: old ranges are zero, new defs read back, sound
    // relocations still name pack-local ids
    let h = match &back.json.header {
        hwvx_xfile::pack::PackHeader::Standard(h) => h,
        _ => anyhow::bail!("expected a standard header"),
    };
    let old = hwvx_xfile::blob::Blob::from_split(hwvx_xfile::format::Endian::Little, &beta);
    let new = hwvx_xfile::blob::Blob::from_split(hwvx_xfile::format::Endian::Big, &back);
    let bh = match &beta.json.header {
        hwvx_xfile::pack::PackHeader::Standard(h) => h,
        _ => anyhow::bail!("expected a standard header"),
    };
    let mut ranges = vec![(bh.texture_data_offset, bh.texture_data_offset + bh.num_textures * hwvx_xfile::convert::texture::BETA_HEADER_LEN as u32)];
    for i in 0..bh.num_textures as usize {
        let th = hwvx_xfile::convert::texture::BetaTexHeader::read(
            &old.data,
            bh.texture_data_offset as usize + i * hwvx_xfile::convert::texture::BETA_HEADER_LEN,
        )?;
        ranges.push((th.pixels, th.pixels + th.stored_len() as u32));
    }
    ranges.push((bh.palette_offset, bh.palette_offset + bh.num_palettes * hwvx_xfile::convert::texture::PALETTE_LEN as u32));
    for &(a, b) in &ranges {
        anyhow::ensure!(
            new.data[a as usize..b as usize].iter().all(|&x| x == 0),
            "prototype bytes {a:#x}..{b:#x} survived the conversion"
        );
    }
    println!("prototype art ranges zeroed: {} ranges", ranges.len());
    for i in 0..h.num_textures as usize {
        let o = h.texture_data_offset as usize + i * hwvx_xfile::convert::texture::GC_HEADER_SLOT;
        let d = &new.data;
        let le = |i: usize| u16::from_le_bytes([d[o + i], d[o + i + 1]]) as u32;
        println!(
            "  tex {i}: {}x{} fmt {:#x} mips {} pixels {:#x} pointer-slot {}",
            le(0),
            le(2),
            le(4),
            le(6),
            new.u32(hwvx_xfile::format::Endian::Big, (o + 8) as u32).unwrap(),
            new.pointers.contains(&(o as u32 + 8))
        );
    }
    let sr: Vec<u32> = new.sound_relocs.iter().map(|r| r.index()).collect();
    println!("sound reloc ids {sr:?} packs {:?}\n", new.sound_relocs.iter().map(|r| r.pack()).collect::<Vec<_>>());
    println!("header: {:?}", out.json.header);
    Ok(())
}

fn read(dir: &Path) -> anyhow::Result<hwvx_xfile::pack::SplitPack> {
    let json_path = dir.join(hwvx_xfile::PACK_FILE);
    let json: hwvx_xfile::pack::PackJson = serde_json::from_slice(&std::fs::read(&json_path)?)?;
    let mut sections = std::collections::BTreeMap::new();
    for entry in fs_read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "bin") {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            sections.insert(name, std::fs::read(&path)?);
        }
    }
    Ok(hwvx_xfile::pack::SplitPack { json, sections })
}

fn fs_read_dir(p: &Path) -> anyhow::Result<std::fs::ReadDir> {
    std::fs::read_dir(p).map_err(Into::into)
}
