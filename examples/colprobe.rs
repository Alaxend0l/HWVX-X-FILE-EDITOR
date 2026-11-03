// Scratch probe: walk the collision pack of the GC, PS2-retail and prototype
// world-1 archives with the Claim_Collision map, report structure counts, and
// dump the bytes around texture/sound reloc slots.
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use hwvx_xfile::archive::Archive;
use hwvx_xfile::blob::Blob;
use hwvx_xfile::format::{Endian, Platform, PackType, TYPE_COLLISION};

fn main() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("orig");
    probe(&root.join("gc/files/worlds_/col_/w1_col.xgc"), Platform::Gc, Endian::Big, "GC")?;
    probe(
        &root.join("ps2/XPS/ENGLISH/worlds_/col_/w1_col.xps"),
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
    println!("==== {tag}: {} ====", path.display());
    for entry in ar.entries.iter() {
        if entry.kind.0 != TYPE_COLLISION {
            continue;
        }
        let sp = hwvx_xfile::pack::split(platform, entry.kind, &entry.data).expect("split");
        let blob = Blob::from_split(e, &sp);
        let b = &blob.data;
        let w = |o: usize| -> u32 {
            b.get(o..o + 4)
                .map(|s| match e {
                    Endian::Big => u32::from_be_bytes([s[0], s[1], s[2], s[3]]),
                    Endian::Little => u32::from_le_bytes([s[0], s[1], s[2], s[3]]),
                })
                .unwrap_or(0xFFFF_FFFF)
        };
        let w16 = |o: usize| -> u32 {
            b.get(o..o + 2)
                .map(|s| match e {
                    Endian::Big => u16::from_be_bytes([s[0], s[1]]) as u32,
                    Endian::Little => u16::from_le_bytes([s[0], s[1]]) as u32,
                })
                .unwrap_or(0xFFFF)
        };
        let f = |o: usize| -> f32 { f32::from_bits(w(o)) };

        println!(
            "{tag} col: len={:#x} version={:#x} mode={} numX={} numY={} numZ={} cellSize={:.2}/{:.2}/{:.2}",
            b.len(),
            w(0x24),
            w(0x58),
            w(0x34) as i32,
            w(0x38) as i32,
            w(0x3C) as i32,
            f(0x28),
            f(0x2C),
            f(0x30)
        );
        println!(
            "  cells={:#x} cellIndex={:#x} numPolys={} polys={:#x} numEdges={} edges={:#x} materials={:#x}",
            w(0x20),
            w(0x40),
            w(0x48),
            w(0x4C),
            w(0x50),
            w(0x54),
            w(0x5C)
        );

        let total = (w(0x34) * w(0x38) * w(0x3C)) as usize;
        let ci = w(0x40) as usize;
        let (mut num_cells, mut nz, mut mx) = (0usize, 0usize, 0u32);
        for i in 0..total.min(0x40000) {
            let c = w(ci + i * 4);
            if c != 0xFFFF_FFFF {
                nz += 1;
                if c + 1 > num_cells as u32 {
                    num_cells = (c + 1) as usize;
                }
                if c > mx {
                    mx = c;
                }
            }
        }
        println!("  cellIndex entries={total} used={nz} numCells={num_cells} max={mx}");

        let cells_at = w(0x20) as usize;
        let (mut tnodes, mut tpolyidx, mut tedgeidx, mut cell_polys, mut with_bsp) =
            (0usize, 0usize, 0usize, 0usize, 0usize);
        let mut node_span = (usize::MAX, 0usize);
        let mut polyidx_span = (usize::MAX, 0usize);
        let mut edgeidx_span = (usize::MAX, 0usize);
        for i in 0..num_cells {
            let c = cells_at + i * 0x50;
            if c + 0x50 > b.len() {
                println!("  cell {i} runs past the end at {c:#x}");
                break;
            }
            let (nodes, cp, edges) = (w(c + 0x30) as usize, w(c + 0x38) as usize, w(c + 0x44) as usize);
            let (np, ne) = (w(c + 0x3C), w(c + 0x40));
            if nodes != 0xFFFF_FFFF && nodes + 0x20 <= b.len() {
                with_bsp += 1;
                let mut stack = vec![0u32];
                let mut seen = 0usize;
                while let Some(ni) = stack.pop() {
                    let n = nodes + ni as usize * 0x20;
                    if n + 0x20 > b.len() || seen > 0x80000 {
                        break;
                    }
                    seen += 1;
                    node_span.0 = node_span.0.min(n);
                    node_span.1 = node_span.1.max(n + 0x20);
                    for k in 0..2 {
                        let ch = w16(n + 0x14 + k * 2);
                        if ch != 0xFFFF {
                            stack.push(ch);
                        }
                    }
                }
                tnodes += seen;
            }
            if cp != 0xFFFF_FFFF && np != 0 {
                cell_polys += 1;
                tpolyidx += np as usize;
                polyidx_span.0 = polyidx_span.0.min(cp as usize);
                polyidx_span.1 = polyidx_span.1.max(cp as usize + np as usize * 4);
            }
            if edges != 0xFFFF_FFFF && ne != 0 {
                tedgeidx += ne as usize;
                edgeidx_span.0 = edgeidx_span.0.min(edges as usize);
                edgeidx_span.1 = edgeidx_span.1.max(edges as usize + ne as usize * 2);
            }
        }
        println!(
            "  cells={num_cells} withBSP={with_bsp} nodes={tnodes} nodes@{node_span:?} cellsWithPolyIdx={cell_polys} polyIdx={tpolyidx} {polyidx_span:?} edgeIdx={tedgeidx} {edgeidx_span:?}"
        );

        let edges_at = w(0x54) as usize;
        let mats = w(0x5C) as usize;
        let polys_at = w(0x4C) as usize;
        let num_polys = w(0x48) as usize;
        let in_struct = |off: usize| -> String {
            if off < 0x60 {
                "header".into()
            } else if (ci..ci + total * 4).contains(&off) {
                format!("cellIndex+{:#x}", (off - ci) % 4)
            } else if (cells_at..cells_at + num_cells * 0x50).contains(&off) {
                format!("cell+{:#x}", (off - cells_at) % 0x50)
            } else if (edges_at..edges_at + (w(0x50) as usize) * 0x30).contains(&off) {
                format!("edge+{:#x}", (off - edges_at) % 0x30)
            } else if (polys_at..polys_at + num_polys * 0x20).contains(&off) {
                format!("poly+{:#x}", (off - polys_at) % 0x20)
            } else if (node_span.0..node_span.1).contains(&off) {
                format!("node+{:#x}", (off - node_span.0) % 0x20)
            } else if (mats..mats + 0x8000).contains(&off) {
                "material".into()
            } else {
                format!("free={off:#x}")
            }
        };
        let mut cls: BTreeMap<String, usize> = BTreeMap::new();
        for &p in &blob.pointers {
            *cls.entry(in_struct(p as usize)).or_default() += 1;
        }
        println!("  {} pointers by struct: {cls:?}", blob.pointers.len());

        let mut nsurf = 0usize;
        for i in 0..0x2000 {
            let m = w(mats + i * 4);
            if m == 0xFFFF_FFFF || m as usize + 0x20 > b.len() {
                break;
            }
            nsurf += 1;
        }
        println!("  material table scan: {nsurf} surface pointers @0x{mats:x}");
        for i in 0..nsurf.min(4) {
            let s = w(mats + i * 4) as usize;
            println!("  surf {i}: {:02x?}", &b[s..s + 0x20]);
        }
        if edges_at + 0x30 <= b.len() {
            println!("  edge0: {:02x?}", &b[edges_at..edges_at + 0x30]);
        }
        if num_polys > 0 && polys_at + 0x20 <= b.len() {
            println!("  poly0: {:02x?}", &b[polys_at..polys_at + 0x20]);
        }
        if node_span.0 < node_span.1 {
            println!("  node0: {:02x?}", &b[node_span.0..node_span.0 + 0x20]);
        }
        if cell_polys > 0 {
            println!("  cell0 raw: {:02x?}", &b[cells_at..cells_at + 0x50]);
        }

        for (k, r) in blob.texture_relocs.iter().enumerate() {
            let s = r.slot as usize;
            println!(
                "  texreloc {k}: slot {s:#x} in [{}] value {:#x} bytes@-8..+8 {:02x?}",
                in_struct(s),
                r.value,
                &b[s.saturating_sub(8)..(s + 8).min(b.len())]
            );
        }
        for (k, r) in blob.sound_relocs.iter().enumerate() {
            let s = r.slot as usize;
            let lo = s.saturating_sub(0x18);
            println!(
                "  sndreloc {k}: slot {s:#x} in [{}] value {:#x} bytes@-0x18..+4 {:02x?}",
                in_struct(s),
                r.value,
                &b[lo..(s + 4).min(b.len())]
            );
        }
        for (k, r) in blob.ref_relocs.iter().enumerate() {
            println!(
                "  refreloc {k}: slot {:#x} value {:#x} in [{}]",
                r.slot,
                r.value,
                in_struct(r.slot as usize)
            );
        }

        if let hwvx_xfile::pack::PackHeader::Standard(hh) = &sp.json.header {
            println!(
                "  wire: textures={} tex_off={:#x} palettes={} pal_off={:#x} sounds={} sfx_off={:#x} song={} refs={:?}",
                hh.num_textures,
                hh.texture_data_offset,
                hh.num_palettes,
                hh.palette_offset,
                hh.num_sounds,
                hh.sfx_offset,
                hh.song_offset,
                hh.refs.iter().map(|r| r.name.as_str()).collect::<Vec<_>>()
            );
        }
        println!("  sound_relocs={:?} texture_relocs={} ref_relocs={} shader_relocs={}",
            blob.sound_relocs.iter().map(|r| (r.slot, r.value)).collect::<Vec<_>>(),
            blob.texture_relocs.len(),
            blob.ref_relocs.len(),
            blob.shader_relocs.len());
        let _ = PackType(0);
    }
    Ok(())
}
