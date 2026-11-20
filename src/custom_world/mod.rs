//! A custom world from a modder's geometry (`docs/plans/custom-worlds.md`):
//! one hub quadrant pack (render geometry, no region) and one collision pack
//! (terrain grid, material indices only) in an archive of their own, plus a
//! `world.jsonc` the game reads (HWVX-SDL `src/game/Port_World.c`). Nothing
//! here is game data: textures are indices into the base level's world
//! texture pack, surfaces indices into its collision materials, and the game
//! composes the world over a base level as it loads.

pub mod terrain;

use std::collections::BTreeMap;

use anyhow::{ensure, Result};

use crate::archive::{Archive, Entry};
use crate::blob::{Blob, Reloc};
use crate::convert::geometry::{build_gc, colour_attr, Group, Lod, Model, Vertex};
use crate::format::{Endian, PackType, Platform, TYPE_COLLISION, TYPE_GEOMETRY, VERSION_RELEASE};
use crate::pack::{self, PackHeader, PackJson, PackRef, QuadHeader, SplitPack, StdHeader};

pub use terrain::ColTri;

/// The world id the archive's TOC gives the quadrant packs (the game sets the
/// composed world's id to it).
pub const WORLD_ID: u32 = 0;
/// The hub quadrant's pack id.
pub const HUB_PACK: u32 = 0xFFFF;
/// The collision pack's id in the archive.
pub const COLLISION_ID: u32 = 0;
/// First geometry id of the hub's geometries.
pub const FIRST_GEOM_ID: u32 = 0x7C00_0000;
/// Every retail world geometry's single LOD distance.
const FAR_LOD: f32 = 1.0e8;

/// A render triangle: positions, UVs, RGBA colours (0xFF full).
#[derive(Clone, Debug, PartialEq)]
pub struct Tri {
    pub p: [[f32; 3]; 3],
    pub uv: [[f32; 2]; 3],
    pub col: [[u8; 4]; 3],
}

/// A group (one draw): a base world texture (its index in the base level's
/// world texture pack) or none (vertex colour only).
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    pub texture: Option<u32>,
    pub tris: Vec<Tri>,
}

/// One geometry of the hub.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub name: String,
    pub parts: Vec<Part>,
}

/// What a custom world is built from.
#[derive(Clone, Debug)]
pub struct Source {
    pub meshes: Vec<Mesh>,
    pub collision: Vec<ColTri>,
    /// Player 1's start: x, y, z and the heading in degrees.
    pub start: [f32; 4],
}

/// The built archive and what went into it.
pub struct Built {
    pub archive: Vec<u8>,
    pub geometries: usize,
    pub triangles: usize,
    pub quad_pack_bytes: usize,
    pub grid: terrain::Grid,
    pub collision_bytes: usize,
}

impl Built {
    pub fn report(&self) -> String {
        let g = &self.grid;
        let nodes: Vec<usize> = g.cells.iter().map(|c| c.tree.len()).collect();
        format!(
            "hub: {} geometries, {} triangles, {} bytes; collision: grid {} x {} x {} of {:.0} x {:.0} x {:.0}, {} cells, nodes {} (max {}), {} crease edges, {} open edges, {} edges of 3+ faces, {} bytes",
            self.geometries,
            self.triangles,
            self.quad_pack_bytes,
            g.n[0],
            g.n[1],
            g.n[2],
            g.size[0],
            g.size[1],
            g.size[2],
            g.cells.len(),
            nodes.iter().sum::<usize>(),
            nodes.iter().max().unwrap_or(&0),
            g.edges.len(),
            g.open_edges,
            g.shared_edges,
            self.collision_bytes
        )
    }
}

fn model_of(m: &Mesh) -> Result<Model> {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    let mut groups = vec![];
    let mut names = vec![];
    for part in &m.parts {
        let mut tris = vec![];
        for t in &part.tris {
            for p in &t.p {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            tris.push(std::array::from_fn(|i| Vertex::new(t.p[i], t.uv[i], colour_attr(t.col[i]))));
        }
        ensure!(tris.len() * 3 <= 0xFFFF, "{}: a group of {} triangles (at most 21845 in one draw): split the mesh", m.name, tris.len());
        groups.push(Group { flags: 0, colour: [0; 4], blend: 0, sort: 0, texture: part.texture, shader: None, tris });
        let mut n: Vec<u8> = part.name.bytes().filter(|b| b.is_ascii_graphic()).take(31).collect();
        if n.is_empty() {
            n = b"tex".to_vec();
        }
        names.extend(n);
        names.push(0);
    }
    names.push(0);
    ensure!(lo[0] <= hi[0], "{}: no triangles", m.name);
    let d = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f32>().sqrt() / 2.0;
    Ok(Model {
        kind: 0,
        lod_distances: vec![FAR_LOD],
        bbox_min: lo,
        radius_1c: d * 1.0001,
        bbox_max: hi,
        lods: vec![Lod { names, groups }],
    })
}

fn quad_refs() -> [PackRef; 2] {
    [
        PackRef { name: "HW_share".into(), id: 1, name_raw: Default::default(), runtime_slot: 0 },
        PackRef { name: String::new(), id: 0, name_raw: Default::default(), runtime_slot: 0 },
    ]
}

/// A quadrant pack (type 7, GameCube): header (word 0: the geometry table),
/// the table (id, 0, geometry; the third word a local ref relocation), the
/// geometries 32-aligned with their pointers and texture relocations.
pub fn quad_pack(meshes: &[Mesh]) -> Result<(SplitPack, usize)> {
    let e = Endian::Big;
    let n = meshes.len() as u32;
    let mut b = Blob { data: vec![0; geometry_align(0x20 + 12 * n as usize)], ..Default::default() };
    b.set_ptr(e, 0, 0x20);
    let mut tris = 0;
    for (k, m) in meshes.iter().enumerate() {
        let model = model_of(m)?;
        tris += model.lods[0].groups.iter().map(|g| g.tris.len()).sum::<usize>();
        let g = build_gc(&model, &mut |v| Some(v))?;
        let at = b.append(&g.data, 32);
        for &s in &g.pointers {
            let v = g.u32(e, s).unwrap();
            b.set_ptr(e, at + s, at + v);
        }
        for r in &g.texture_relocs {
            b.texture_relocs.push(Reloc { slot: at + r.slot, value: r.value });
            // the pre-baked slot word: the id high, the slot's own offset low
            let w = ((r.value & 0xFFFF) << 16) | ((at + r.slot) & 0xFFFF);
            b.put_u32(e, at + r.slot, w);
        }
        let entry = 0x20 + 12 * k as u32;
        b.put_u32(e, entry, FIRST_GEOM_ID + k as u32);
        b.put_u32(e, entry + 8, at);
        b.ref_relocs.push(Reloc { slot: entry + 8, value: k as u32 });
    }
    let mut sp = SplitPack {
        json: PackJson {
            header: PackHeader::Quad(QuadHeader { unused: [0; 5], refs: quad_refs() }),
            padding: BTreeMap::new(),
            sections: vec![],
        },
        sections: BTreeMap::new(),
    };
    b.store(e, &mut sp);
    Ok((sp, tris))
}

fn geometry_align(n: usize) -> usize {
    crate::format::align(n, 32)
}

/// The collision pack (type 5, GameCube) of a grid.
pub fn collision_pack(g: &terrain::Grid) -> Result<SplitPack> {
    let e = Endian::Big;
    let b = terrain::write_grid(g, e)?;
    let mut sp = SplitPack {
        json: PackJson {
            header: PackHeader::Standard(StdHeader {
                num_textures: 0,
                texture_data_offset: 0,
                num_sounds: 0,
                sfx_offset: 0,
                num_palettes: 0,
                palette_offset: 0,
                num_shaders: 0,
                shader_offset: 0,
                num_triggers: 0,
                trigger_offset: 0,
                song_offset: 0,
                song_bank_offset: 0,
                refs: quad_refs(),
            }),
            padding: BTreeMap::new(),
            sections: vec![],
        },
        sections: BTreeMap::new(),
    };
    b.store(e, &mut sp);
    Ok(sp)
}

/// The world archive (GameCube): the hub quadrant pack and the collision pack.
pub fn build(src: &Source, opt: &terrain::Options) -> Result<Built> {
    ensure!(!src.meshes.is_empty(), "no render meshes");
    ensure!(src.meshes.len() < 0x10000, "{} geometries: at most 65535", src.meshes.len());
    let p = Platform::Gc;
    let (quad, triangles) = quad_pack(&src.meshes)?;
    let quad_bytes = pack::join(p, &quad)?;
    let grid = terrain::build_grid(&src.collision, opt)?;
    let col = collision_pack(&grid)?;
    let col_bytes = pack::join(p, &col)?;
    let n = src.meshes.len() as u32;
    let entries = vec![
        Entry { version: VERSION_RELEASE, kind: PackType(TYPE_GEOMETRY), id: WORLD_ID, sub_id: (n << 16) | HUB_PACK, data: quad_bytes.clone(), gap: vec![] },
        Entry { version: VERSION_RELEASE, kind: PackType(TYPE_COLLISION), id: COLLISION_ID, sub_id: 0, data: col_bytes.clone(), gap: vec![] },
    ];
    let archive = Archive { platform: p, version: VERSION_RELEASE, entries, tail: vec![] }.build()?;
    Ok(Built {
        archive,
        geometries: src.meshes.len(),
        triangles,
        quad_pack_bytes: quad_bytes.len(),
        grid,
        collision_bytes: col_bytes.len(),
    })
}

/// The `world.jsonc` the game reads (`HWVX_WORLD=<it>`).
pub fn world_jsonc(id: &str, name: &str, base: u32, archive: &str, geometries: usize, start: [f32; 4], comment: &str) -> String {
    let j = serde_json::json!({
        "format": 1,
        "id": id,
        "name": name,
        "base": base,
        "archive": archive,
        "hubGeometries": geometries,
        "worldId": WORLD_ID,
        "collisionId": COLLISION_ID,
        "start": start,
    });
    format!("// {comment}\n// base: the track (number) whose level this world is composed over; archive: <archive>.xgc next to this file\n{}\n", serde_json::to_string_pretty(&j).unwrap())
}

// ---- the generated demo (stage 1) --------------------------------------------

fn shade(n: [f32; 3], base: [u8; 3]) -> [u8; 4] {
    let s = 0.55 + 0.35 * n[1].max(0.0) + 0.1 * n[0];
    let c = |v: u8| ((v as f32 * s).round().clamp(0.0, 255.0)) as u8;
    [c(base[0]), c(base[1]), c(base[2]), 0xFF]
}

fn normal(p: &[[f32; 3]; 3]) -> [f32; 3] {
    let a = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let b = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    let n = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
    [n[0] / l, n[1] / l, n[2] / l]
}

/// A quad (counter-clockwise from its front) as two triangles; `uvs` per corner.
fn quad(q: [[f32; 3]; 4], uv: [[f32; 2]; 4], base: [u8; 3]) -> [Tri; 2] {
    let t = |a: usize, b: usize, c: usize| {
        let p = [q[a], q[b], q[c]];
        let col = shade(normal(&p), base);
        Tri { p, uv: [uv[a], uv[b], uv[c]], col: [col; 3] }
    };
    [t(0, 1, 2), t(0, 2, 3)]
}

/// The axis-aligned box lo..hi's six faces, facing out.
fn box_faces(lo: [f32; 3], hi: [f32; 3], base: [u8; 3]) -> Vec<Tri> {
    let (a, b) = (lo, hi);
    let uv = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
    let faces = [
        [[a[0], a[1], a[2]], [a[0], a[1], b[2]], [a[0], b[1], b[2]], [a[0], b[1], a[2]]], // -x
        [[b[0], a[1], a[2]], [b[0], b[1], a[2]], [b[0], b[1], b[2]], [b[0], a[1], b[2]]], // +x
        [[a[0], a[1], a[2]], [b[0], a[1], a[2]], [b[0], a[1], b[2]], [a[0], a[1], b[2]]], // -y
        [[a[0], b[1], a[2]], [a[0], b[1], b[2]], [b[0], b[1], b[2]], [b[0], b[1], a[2]]], // +y
        [[a[0], a[1], a[2]], [a[0], b[1], a[2]], [b[0], b[1], a[2]], [b[0], a[1], a[2]]], // -z
        [[a[0], a[1], b[2]], [b[0], a[1], b[2]], [b[0], b[1], b[2]], [a[0], b[1], b[2]]], // +z
    ];
    faces.iter().flat_map(|q| quad(*q, uv, base)).collect()
}

/// A wedge along +z: from (x0..x1, y 0, z0) rising to height `h` at z1, then a
/// vertical back face; no bottom (it stands on the ground).
fn ramp_faces(x0: f32, x1: f32, z0: f32, z1: f32, h: f32, base: [u8; 3]) -> Vec<Tri> {
    let uv = [[0.0, 0.0], [0.0, 2.0], [2.0, 2.0], [2.0, 0.0]];
    let mut out = vec![];
    // the slope, facing up and back (-z)
    out.extend(quad([[x0, 0.0, z0], [x0, h, z1], [x1, h, z1], [x1, 0.0, z0]], uv, base));
    // the back face, facing +z
    out.extend(quad([[x0, 0.0, z1], [x1, 0.0, z1], [x1, h, z1], [x0, h, z1]], uv, base));
    // the sides
    let tri = |p: [[f32; 3]; 3]| {
        let col = shade(normal(&p), base);
        Tri { p, uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], col: [col; 3] }
    };
    out.push(tri([[x0, 0.0, z0], [x0, 0.0, z1], [x0, h, z1]]));
    out.push(tri([[x1, 0.0, z0], [x1, h, z1], [x1, 0.0, z1]]));
    out
}

/// Texture indices in world1's world texture pack (`w1_tex`) the demo uses:
/// the chunker's arena's (`tools/xdl_chunk.py arena` defaults).
pub const DEMO_TEX_GROUND: u32 = 4;
pub const DEMO_TEX_WALL: u32 = 9;
pub const DEMO_TEX_RAMP: u32 = 32;

/// The demo world: a 3,000 x 3,000 ground (12 x 12 tiles), a ramp (200 wide,
/// 400 long, 80 high) ahead of the start and walls (80 high, 40 thick) round the edge
/// and a pillar (160 high) at x 300, z -700; the start 600 before the ramp.
pub fn demo() -> Source {
    let h = 1500.0f32;
    let tile = 250.0f32;
    let mut ground = vec![];
    let n = (2.0 * h / tile) as usize;
    for i in 0..n {
        for j in 0..n {
            let (x0, z0) = (-h + i as f32 * tile, -h + j as f32 * tile);
            let (x1, z1) = (x0 + tile, z0 + tile);
            ground.extend(quad(
                [[x0, 0.0, z0], [x0, 0.0, z1], [x1, 0.0, z1], [x1, 0.0, z0]],
                [[0.0, 0.0], [0.0, 4.0], [4.0, 4.0], [4.0, 0.0]],
                [150, 160, 140],
            ));
        }
    }
    let ramp = ramp_faces(-100.0, 100.0, -600.0, -200.0, 80.0, [230, 200, 120]);
    // walls round the edge (touching, not overlapping: inside one past a face of
    // another would read as empty), and a pillar right of the way (+x) to tell
    // the handedness by
    let t = 40.0f32;
    let mut wall = box_faces([-h, 0.0, h - t], [h, 80.0, h], [200, 120, 110]);
    wall.extend(box_faces([-h, 0.0, -h], [h, 80.0, -h + t], [200, 120, 110]));
    wall.extend(box_faces([-h, 0.0, -h + t], [-h + t, 80.0, h - t], [200, 120, 110]));
    wall.extend(box_faces([h - t, 0.0, -h + t], [h, 80.0, h - t], [200, 120, 110]));
    wall.extend(box_faces([280.0, 0.0, -720.0], [320.0, 160.0, -680.0], [120, 120, 220]));
    let meshes = vec![
        Mesh { name: "ground".into(), parts: vec![Part { name: "ground".into(), texture: Some(DEMO_TEX_GROUND), tris: ground.clone() }] },
        Mesh { name: "ramp".into(), parts: vec![Part { name: "ramp".into(), texture: Some(DEMO_TEX_RAMP), tris: ramp.clone() }] },
        Mesh { name: "wall".into(), parts: vec![Part { name: "wall".into(), texture: Some(DEMO_TEX_WALL), tris: wall.clone() }] },
    ];
    // collision: the ground first (the first polygon of a cell is its first
    // splitter), then the ramp and the wall; material 0 everywhere
    let col = |ts: &[Tri]| -> Vec<ColTri> {
        ts.iter().map(|t| ColTri { p: t.p.map(|v| v.map(|x| x as f64)), material: 0 }).collect()
    };
    let mut collision = col(&ground);
    collision.extend(col(&ramp));
    collision.extend(col(&wall));
    Source { meshes, collision, start: [0.0, 20.0, -1200.0, 0.0] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::geometry::read_gc;

    #[test]
    fn demo_builds_and_reads_back() {
        let src = demo();
        let b = build(&src, &terrain::Options::default()).unwrap();
        let ar = Archive::parse(&b.archive, Platform::Gc).unwrap();
        assert_eq!(ar.entries.len(), 2);
        assert_eq!(ar.entries[0].sub_id, (3 << 16) | HUB_PACK);
        // the quadrant pack: each geometry decodes to the input triangles
        let sp = pack::split(Platform::Gc, ar.entries[0].kind, &ar.entries[0].data).unwrap();
        let blob = Blob::from_split(Endian::Big, &sp);
        for (k, m) in src.meshes.iter().enumerate() {
            let at = blob.u32(Endian::Big, 0x20 + 12 * k as u32 + 8).unwrap();
            let model = read_gc(&blob, at).unwrap();
            let got: Vec<[[f32; 3]; 3]> = model.lods[0].groups.iter().flat_map(|g| g.tris.iter().map(|t| t.map(|v| v.pos))).collect();
            let want: Vec<[[f32; 3]; 3]> = m.parts.iter().flat_map(|p| p.tris.iter().map(|t| t.p)).collect();
            assert_eq!(got, want, "geometry {k}");
            assert_eq!(model.lods[0].groups[0].texture, m.parts[0].texture);
        }
        // the collision pack walks as the game's swap walks it, and reads back
        let sp = pack::split(Platform::Gc, ar.entries[1].kind, &ar.entries[1].data).unwrap();
        let blob = Blob::from_split(Endian::Big, &sp);
        let w = crate::convert::collision::walk(&blob, Endian::Big).unwrap();
        let g = terrain::read_grid(&blob, Endian::Big).unwrap();
        assert_eq!(w.cells, g.cells.len() + 1);
        assert_eq!(w.nodes, g.cells.iter().map(|c| c.tree.len()).sum::<usize>());
        assert_eq!(g.cells.len(), b.grid.cells.len());
        // down rays: the ground, the ramp's top, its middle, the wall's top
        let at = |x: f64, z: f64| g.ground_below([x, 300.0, z], 400.0, 0.5).unwrap();
        assert!(at(500.0, 0.0).abs() < 0.01, "ground {}", at(500.0, 0.0));
        assert!((at(0.0, -201.0) - 79.8).abs() < 0.1, "ramp top {}", at(0.0, -201.0));
        assert!((at(0.0, -400.0) - 40.0).abs() < 0.1, "ramp middle {}", at(0.0, -400.0));
        assert!((at(0.0, 1480.0) - 80.0).abs() < 0.01, "wall top {}", at(0.0, 1480.0));
        // just above the ground is open, just below solid, everywhere
        for x in [-1400.0, -700.0, 0.0, 333.0, 1450.0] {
            for z in [-1400.0, -900.0, 0.0, 700.0, 1100.0, 1450.0] {
                assert!(!g.solid([x, 0.5, z]), "open above {x} {z}");
                assert!(g.solid([x, -0.5, z]), "solid below {x} {z}");
            }
        }
        // inside the ramp and the wall: solid
        assert!(g.solid([0.0, 20.0, -250.0]));
        assert!(g.solid([0.0, 40.0, 1480.0]) && g.solid([-1480.0, 40.0, 0.0]) && g.solid([300.0, 100.0, -700.0]));
        assert!(!g.solid([0.0, 40.0, 1450.0]));
        assert!(!g.solid([150.0, 20.0, -250.0]));
        // creases: the ramp's and the wall's edges (the ground is flat)
        assert!(!b.grid.edges.is_empty());
    }
}
