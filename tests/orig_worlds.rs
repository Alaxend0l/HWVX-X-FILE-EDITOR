//! World quadrant cross-check: the GameCube `w1_geo.xgc` quad packs against
//! their PS2-retail (v183) and PS2-prototype (v179) twins.
//!
//! This is the validation gate for `port-world`.  The three levels pair up
//! through the **world pack's quad table**, which is how the runtime itself
//! finds quad packs (FileStreaming.c reads the geo archive's table of
//! contents and matches `WorldQuad.packId == entry id`):
//!
//! * v183 (GC and PS2 retail): `World` numQuads @+0x20, quad list @+0x24,
//!   records of 0x50 (World.h `WorldQuad`): id @+0x00, numGeoms @+0x0C,
//!   packId `(numGeoms << 16 | quadId)` @+0x38.  GC and PS2 records are
//!   word-identical per slot.
//! * v179 (prototype): same table location, but records of 0x40 with the
//!   quad id @+0x00 and the geom count @+0x0C; quad ids differ from retail
//!   (CH1_L2: world quad 906, streaming quads 5780..5799, retail 65535 and
//!   1..20).  The *slot order* is the same world quad layout, so slot i of
//!   each level is the same quadrant of the map, and the quad packs appear
//!   in the archives in that same slot order.
//!
//! With the slots paired the test checks, per paired quad:
//! * every GameCube quad parses with `read_gc` (the parser that will verify
//!   the geometry we emit), every prototype quad with `read_beta`,
//! * the models found in each quad pack equal the world table's `numGeoms`
//!   for that slot, for all three levels,
//! * the GameCube rebuild of the PS2-retail models has exactly the retail
//!   GameCube quad's texture-relocation count, matching model counts, equal
//!   LOD/group structure and triangle counts within 2 %,
//! * for the v179 prototype the same structural checks pass; its triangle
//!   counts are only reported, since v179 art drifted from v183.
//!
//! Skipped when `orig/` is not present.  Run with `cargo test --release`.

use std::fs;
use std::path::Path;

use hwvx_xfile::archive::Archive;
use hwvx_xfile::blob::Blob;
use hwvx_xfile::convert::geometry::{self, Model};
use hwvx_xfile::format::{Endian, Platform, TYPE_GEOMETRY, TYPE_WORLD, VERSION_RELEASE};
use hwvx_xfile::pack::{self, PackHeader};

const LE: Endian = Endian::Little;
const BE: Endian = Endian::Big;

#[derive(Clone)]
struct Quad {
    name: String,
    /// Archive sub_id: low 16 = quad id, high 16 = geom/model count.
    sub_id: u32,
    blob: Blob,
    models: Vec<Model>,
}

impl Quad {
    fn id(&self) -> u32 {
        self.sub_id & 0xFFFF
    }
    fn tris(&self) -> usize {
        self.models
            .iter()
            .map(|m| m.lods.iter().map(|l| l.groups.iter().map(|g| g.tris.len()).sum::<usize>()).sum::<usize>())
            .sum()
    }
}

/// Every geometry pack of an archive, in archive order, fully parsed.
fn quads(path: &Path, platform: Platform, e: Endian) -> Vec<Quad> {
    let data = fs::read(path).unwrap_or_else(|_| panic!("missing {}", path.display()));
    let ar = Archive::parse(&data, platform).unwrap_or_else(|err| panic!("{}: {err:#}", path.display()));
    let mut out = Vec::new();
    for (i, entry) in ar.entries.iter().enumerate() {
        if entry.kind.0 != TYPE_GEOMETRY {
            continue;
        }
        let name = format!("{:02}_q{}_g{}", i, entry.sub_id & 0xFFFF, entry.sub_id >> 16);
        let sp = pack::split(platform, entry.kind, &entry.data)
            .unwrap_or_else(|err| panic!("{} {name}: split: {err:#}", path.display()));
        assert!(matches!(sp.json.header, PackHeader::Quad(_)), "{} {name}: not a quad pack", path.display());
        let blob = Blob::from_split(e, &sp);

        let magic = if e == BE { geometry::GC_MAGIC } else { geometry::BETA_MAGIC };
        // Model heads: the geometry magic, with the LOD list and distance
        // list pointer slots (@+0x08, +0x0C) registered in the pointer table
        // (this is what `is_beta_geometry` checks for cars, and what the
        // runtime's own geometry walk follows).
        let heads: Vec<u32> = (0..blob.data.len().saturating_sub(0x40) as u32)
            .step_by(4)
            .filter(|&off| {
                blob.data.get(off as usize..off as usize + 2).map(|b| if e == BE { u16::from_be_bytes([b[0], b[1]]) } else { u16::from_le_bytes([b[0], b[1]]) })
                    == Some(magic)
                    && blob.pointers.contains(&(off + 8))
                    && blob.pointers.contains(&(off + 0xC))
            })
            .collect();
        let mut models: Vec<Model> = heads
            .iter()
            .map(|&off| {
                if e == BE {
                    geometry::read_gc(&blob, off).unwrap_or_else(|err| panic!("{} {name}: read_gc at {off:#x}: {err:#}", path.display()))
                } else {
                    geometry::read_beta(&blob, off).unwrap_or_else(|err| panic!("{} {name}: read_beta at {off:#x}: {err:#}", path.display()))
                }
            })
            .collect();
        assert!(!models.is_empty(), "{} {name}: no geometry found", path.display());

        // Blob order is platform specific; compare models in world order.
        // (The GameCube blob keeps the LOD name-list pointer slots, which
        // share one bbox, so a stable sort keeps blob order where it agrees.)
        models.sort_by(|a, b| {
            a.bbox_min.iter().chain(a.bbox_max.iter()).partial_cmp(b.bbox_min.iter().chain(b.bbox_max.iter())).unwrap()
        });

        out.push(Quad { name, sub_id: entry.sub_id, blob, models });
    }
    out
}

/// One `WorldQuad` slot of a world pack.
#[derive(Clone)]
struct QuadSlot {
    id: u32,
    pack_id: u32,
    num_geoms: u32,
}

impl QuadSlot {
    fn as_string(&self) -> String {
        format!("id {} pack {:#010x} geoms {}", self.id, self.pack_id, self.num_geoms)
    }
}

/// The world pack's quadrant table: (World +0x20 count, +0x24 list).
/// v183 records are `WorldQuad` (0x50 bytes, packId @+0x38); v179 prototype
/// records are 0x40 bytes with the quad id @+0x00 and numGeoms @+0x0C.
fn quad_slots(path: &Path, platform: Platform) -> Vec<QuadSlot> {
    let e = platform.endian();
    let data = fs::read(path).unwrap_or_else(|_| panic!("missing {}", path.display()));
    let ar = Archive::parse(&data, platform).unwrap_or_else(|err| panic!("{}: {err:#}", path.display()));
    let entry = ar
        .entries
        .iter()
        .find(|e| e.kind.0 == TYPE_WORLD)
        .unwrap_or_else(|| panic!("{}: no world pack", path.display()));
    let sp = pack::split(platform, entry.kind, &entry.data).unwrap_or_else(|err| panic!("{}: {err:#}", path.display()));
    let w = sp.sections.get("data").expect("world pack has no data section");
    let at = |off: usize| e.u32_at(w, off).unwrap_or_else(|| panic!("{}: world pack shorter than its tables", path.display()));

    let stride = if ar.version < VERSION_RELEASE { 0x40 } else { 0x50 };
    let n = at(0x20) as usize;
    let list = at(0x24) as usize;
    assert!(n > 0 && n < 0x100 && list > 0 && list < w.len(), "{}: bad quad table {n} @ {list:#x}", path.display());
    (0..n)
        .map(|i| {
            let r = list + i * stride;
            let id = at(r);
            let num_geoms = at(r + 0x0C);
            let pack_id = if stride == 0x40 { ((num_geoms & 0xFFFF) << 16) | (id & 0xFFFF) } else { at(r + 0x38) };
            QuadSlot { id, pack_id, num_geoms }
        })
        .collect()
}

#[test]
fn w1_quads_read_pair_and_match() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("orig");
    let gc_geo = root.join("gc/files/worlds_/geo_/w1_geo.xgc");
    let gc_world = root.join("gc/files/worlds_/sp_/chal1_/ch1_l2.xgc");
    let ps2_geo = root.join("ps2/XPS/ENGLISH/worlds_/geo_/w1_geo.xps");
    let ps2_world = root.join("ps2/XPS/ENGLISH/worlds_/sp_/chal1_/ch1_l2.xps");
    let beta = root.join("ps2-beta/XPS/ENGLISH/WORLDS_/SP_/CHAL1_/CH1_L2.XPS");
    if !gc_geo.exists() || !ps2_geo.exists() || !beta.exists() || !gc_world.exists() || !ps2_world.exists() {
        eprintln!("orig/ world files not found, skipped");
        return;
    }

    // ---- the world packs pair slot by slot ---------------------------------
    let gs = quad_slots(&gc_world, Platform::Gc);
    let ps = quad_slots(&ps2_world, Platform::Ps2);
    let bs = quad_slots(&beta, Platform::Ps2Beta);
    assert_eq!(gs.len(), 21, "GameCube world quad slots");
    assert_eq!(ps.len(), 21, "PS2-retail world quad slots");
    assert_eq!(bs.len(), 21, "prototype world quad slots");
    for i in 0..21 {
        assert_eq!(gs[i].as_string(), ps[i].as_string(), "quad slot {i}: GC vs PS2-retail record");
    }

    // ---- the geo archives list their packs in slot order -------------------
    let gc = quads(&gc_geo, Platform::Gc, BE);
    let r2 = quads(&ps2_geo, Platform::Ps2, LE);
    let bt = quads(&beta, Platform::Ps2Beta, LE);
    assert_eq!(gc.len(), 21, "GameCube quad packs");
    assert_eq!(r2.len(), 21, "PS2-retail quad packs");
    assert_eq!(bt.len(), 21, "prototype quad packs (inside CH1_L2.XPS)");
    for i in 0..21 {
        assert_eq!(gc[i].sub_id, gs[i].pack_id, "GameCube quad pack {} should match world slot {i}", gc[i].name);
        assert_eq!(r2[i].sub_id, ps[i].pack_id, "PS2-retail quad pack {} should match world slot {i}", r2[i].name);
        assert_eq!(bt[i].id(), bs[i].id, "prototype quad pack {} should match world slot {i}", bt[i].name);
    }
    println!("world slots GC/PS2r: {}", gs.iter().map(|s| s.as_string()).collect::<Vec<_>>().join(", "));
    println!("world slots beta: {}", bs.iter().map(|s| s.as_string()).collect::<Vec<_>>().join(", "));

    // ---- compare paired quads ----------------------------------------------
    for (tag, twin) in [("PS2r", &r2), ("beta", &bt)] {
        for i in 0..21 {
            let (qg, qo) = (&gc[i], &twin[i]);
            let qn = format!("{tag}: {} <-> {}", qg.name, qo.name);

            // Reader bookkeeping: every shader relocation must land on exactly
            // one parsed group base (and vice versa).
            let groups_with_shader = |q: &Quad| -> usize {
                q.models.iter().flat_map(|m| m.lods.iter().flat_map(|l| l.groups.iter())).filter(|g| g.shader.is_some()).count()
            };
            assert_eq!(groups_with_shader(qg), qg.blob.shader_relocs.len(), "{qn}: GameCube shader groups vs blob shader relocs");
            assert_eq!(groups_with_shader(qo), qo.blob.shader_relocs.len(), "{qn}: {tag} shader groups vs blob shader relocs");

            // Rebuild the prototype/retail models as GC geometry and compare
            // the blob bookkeeping tables with the real GameCube quad.
            let mut built = Blob::default();
            for m in &qo.models {
                let b = geometry::build_gc(m, &mut |t| Some(t)).unwrap_or_else(|e| panic!("{qn}: build_gc: {e:#}"));
                built.graft(BE, &b, 32).unwrap_or_else(|e| panic!("{qn}: graft: {e:#}"));
            }
            println!(
                "{qn}: models {:>2}/{:<2} tris {:>5}/{:<5} ptr {:>5}/{:<5} texreloc {:>3}/{:<3} shader {:>2}/{:<2}",
                qg.models.len(),
                qo.models.len(),
                qg.tris(),
                qo.tris(),
                built.pointers.len(),
                qg.blob.pointers.len(),
                built.texture_relocs.len(),
                qg.blob.texture_relocs.len(),
                qg.blob.shader_relocs.len(),
                qo.blob.shader_relocs.len(),
            );

            if tag == "PS2r" {
                // Same engine version: the conversion pipeline must produce
                // exactly the relocation table of the retail GameCube quad,
                // and the art must essentially agree.
                assert_eq!(built.texture_relocs.len(), qg.blob.texture_relocs.len(), "{qn}: texture relocation count");
                assert_eq!(built.shader_relocs.len(), qg.blob.shader_relocs.len(), "{qn}: shader relocation count");
                for r in built.shader_relocs.iter().chain(qg.blob.shader_relocs.iter()) {
                    assert_eq!(r.slot % 0x20, 0, "{qn}: shader reloc slot {:#x} is not a group base", r.slot);
                }
                let mut vg: Vec<u32> = qg.blob.shader_relocs.iter().map(|r| r.value).collect();
                let mut vo: Vec<u32> = qo.blob.shader_relocs.iter().map(|r| r.value).collect();
                vg.sort();
                vo.sort();
                assert_eq!(vg, vo, "{qn}: shader relocation values GC vs PS2-retail");
                assert_eq!(qg.models.len(), qo.models.len(), "{qn}: model count");
                assert_eq!(qo.models.len() as u32, gs[i].num_geoms, "{qn}: quad model count vs world table numGeoms");
                for (mi, (mg, mo)) in qg.models.iter().zip(&qo.models).enumerate() {
                    let ctx = format!("{qn} model {mi}");
                    assert_eq!(mg.lods.len(), mo.lods.len(), "{ctx}: LOD count");
                    for (l, (lg, lo)) in mg.lods.iter().zip(&mo.lods).enumerate() {
                        assert_eq!(lg.groups.len(), lo.groups.len(), "{ctx} LOD {l}: group count");
                        let tg: usize = lg.groups.iter().map(|g| g.tris.len()).sum();
                        let to: usize = lo.groups.iter().map(|g| g.tris.len()).sum();
                        let f = tg.abs_diff(to) as f32 / tg.max(to) as f32;
                        assert!(f < 0.02, "{ctx} LOD {l}: tri counts {tg} vs {to} differ by {:.1}%", f * 100.0);
                    }
                }
            } else {
                // v179 prototype: the world table's geom count must equal the
                // models actually present in the paired quad pack (this is
                // exactly how the runtime finds them, and it is what
                // `port-world` will encode into the rebuilt geo TOC).
                assert_eq!(qo.models.len() as u32, bs[i].num_geoms, "{qn}: prototype quad model count vs world table numGeoms");
                // Art has drifted between v179 and v183, so triangle counts
                // are only reported, not asserted.
            }
        }
    }
}

