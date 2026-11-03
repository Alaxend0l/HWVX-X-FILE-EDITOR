// Scratch probe: across every world-quad pack of the GC, PS2-retail and
// prototype geo archives, report (a) group flag stats vs texture relocs —
// do F_ENV groups carry texture relocs? — and (b) shader relocs: count,
// whether each slot is a group base, and the on-file +0x18 pattern there.
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use hwvx_xfile::archive::Archive;
use hwvx_xfile::blob::Blob;
use hwvx_xfile::convert::geometry::{BETA_MAGIC, GC_MAGIC};
use hwvx_xfile::format::{Endian, Platform, TYPE_GEOMETRY};
use hwvx_xfile::pack;

fn main() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("orig");
    probe(
        &root.join("gc/files/worlds_/geo_/w1_geo.xgc"),
        Platform::Gc,
        Endian::Big,
        "GC",
    )?;
    probe(
        &root.join("ps2/XPS/ENGLISH/worlds_/geo_/w1_geo.xps"),
        Platform::Ps2,
        Endian::Little,
        "PS2r",
    )?;
    probe(
        &root.join("ps2-beta/XPS/ENGLISH/WORLDS_/SP_/CHAL1_/CH1_L2.XPS"),
        Platform::Ps2Beta,
        Endian::Little,
        "BETA",
    )?;
    Ok(())
}

fn probe(path: &Path, platform: Platform, e: Endian, tag: &str) -> anyhow::Result<()> {
    let data = fs::read(path)?;
    let ar = Archive::parse(&data, platform)?;
    let magic = if e == Endian::Big { GC_MAGIC } else { BETA_MAGIC };
    let mut quads = 0usize;
    let mut env_groups = 0usize;
    let mut env_with_tex = 0usize;
    let mut env_with_shader = 0usize;
    let mut shader_groups = 0usize;
    let mut shader_with_tex = 0usize;
    let mut shader_at_base = 0usize;
    let mut patterns: BTreeMap<[u8; 4], usize> = BTreeMap::new();
    for entry in ar.entries.iter() {
        if entry.kind.0 != TYPE_GEOMETRY {
            continue;
        }
        quads += 1;
        let sp = pack::split(platform, entry.kind, &entry.data).expect("split");
        let blob = Blob::from_split(e, &sp);
        let tmap: BTreeMap<u32, u32> = blob.texture_relocs.iter().map(|r| (r.slot, r.value)).collect();
        let smap: BTreeMap<u32, u32> = blob.shader_relocs.iter().map(|r| (r.slot, r.value)).collect();

        // Find geometry heads (magic + pointer slots at +0x08/+0x0C).
        let heads: Vec<u32> = (0..blob.data.len().saturating_sub(0x40) as u32)
            .step_by(4)
            .filter(|&off| {
                let b = &blob.data;
                let m = if e == Endian::Big {
                    u16::from_be_bytes([b[off as usize], b[off as usize + 1]])
                } else {
                    u16::from_le_bytes([b[off as usize], b[off as usize + 1]])
                };
                m == magic && blob.pointers.contains(&(off + 8)) && blob.pointers.contains(&(off + 0xC))
            })
            .collect();

        let mut group_bases: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for &g in &heads {
            let rd16 = |off: usize| -> u32 {
                let b = &blob.data;
                if e == Endian::Big {
                    u16::from_be_bytes([b[off], b[off + 1]]) as u32
                } else {
                    u16::from_le_bytes([b[off], b[off + 1]]) as u32
                }
            };
            let rptr = |off: usize| -> u32 {
                let b = &blob.data;
                if e == Endian::Big {
                    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
                } else {
                    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
                }
            };
            let num_lods = rd16(g as usize + 2) as usize;
            let lods_at = rptr(g as usize + 8);
            for l in 0..num_lods {
                let lod = lods_at as usize + if e == Endian::Big { 0x18 * l } else { 24 * l };
                let num_groups = rd16(lod + 2) as usize;
                let groups_at = match blob.pointers.contains(&((lod + 8) as u32)) {
                    true => rptr(lod + 8),
                    false => continue,
                };
                for k in 0..num_groups {
                    let at = groups_at as usize + 0x20 * k;
                    group_bases.insert(at as u32);
                    let flags = rd16(at + 2);
                    let has_t = tmap.contains_key(&(at as u32 + 0x18));
                    let shader = smap.get(&(at as u32));
                    if shader.is_some() {
                        shader_at_base += 1;
                    }
                    if flags & 0x0002 != 0 {
                        env_groups += 1;
                        if has_t {
                            env_with_tex += 1;
                        }
                        if shader.is_some() {
                            env_with_shader += 1;
                        }
                    }
                    if shader.is_some() {
                        shader_groups += 1;
                        if has_t {
                            shader_with_tex += 1;
                        }
                        let p = [blob.data[at + 0x18], blob.data[at + 0x19], blob.data[at + 0x1A], blob.data[at + 0x1B]];
                        *patterns.entry(p).or_default() += 1;
                    }
                }
            }
        }
        for (&slot, _) in smap.iter() {
            if !group_bases.contains(&slot) {
                println!("{tag}: quad {quads}: shader slot {slot:#x} is NOT a group base");
            }
        }
    }
    println!(
        "{tag}: quads={quads} env_groups={env_groups} env_with_tex={env_with_tex} env_with_shader={env_with_shader} \
         shader_groups={shader_groups} shader_slots_at_base={shader_at_base} shader_with_tex={shader_with_tex}"
    );
    println!("{tag}: shader-group +0x18 raw bytes: {patterns:?}");
    Ok(())
}
