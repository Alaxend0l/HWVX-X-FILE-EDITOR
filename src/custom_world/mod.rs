//! A custom world from a modder's geometry (`docs/plans/custom-worlds.md`):
//! one hub quadrant pack (render geometry, no region) and one collision pack
//! (terrain grid, material indices only) in an archive of their own, plus a
//! `world.jsonc` the game reads (HWVX-SDL `src/game/Port_World.c`). Nothing
//! here is game data: textures are indices into the base level's world
//! texture pack, surfaces indices into its collision materials, and the game
//! composes the world over a base level as it loads.

pub mod gltf_in;
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
    let col = |ts: &[Tri], object: u32| -> Vec<ColTri> {
        ts.iter().map(|t| ColTri { p: t.p.map(|v| v.map(|x| x as f64)), material: 0, object }).collect()
    };
    let mut collision = col(&ground, 0);
    collision.extend(col(&ramp, 1));
    collision.extend(col(&wall, 2));
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

#[cfg(test)]
mod gltf_tests {
    use super::*;

    /// The demo written as a plain glTF and read back through a project gives
    /// the same archive as the demo built directly.
    #[test]
    fn demo_through_gltf() {
        let src = demo();
        let dir = std::env::temp_dir().join(format!("hwvx-world-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("demo.gltf"), serde_json::to_vec(&gltf_in::write_gltf(&src)).unwrap()).unwrap();
        std::fs::write(dir.join("world.jsonc"), demo_project()).unwrap();
        let p = gltf_in::Project::read(&dir.join("world.jsonc")).unwrap();
        let (back, rep) = gltf_in::import(&p).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(back.meshes, src.meshes);
        assert_eq!(rep.degenerate, 0);
        assert_eq!(back.start, src.start);
        let opt = terrain::Options::default();
        assert_eq!(build(&back, &opt).unwrap().archive, build(&src, &opt).unwrap().archive);
    }

    /// Scale, turn and mirroring: a node scaled by -1 keeps its triangles
    /// facing out (the winding is turned over).
    #[test]
    fn placement() {
        let src = demo();
        let mut gl = gltf_in::write_gltf(&src);
        gl["nodes"][1]["scale"] = serde_json::json!([-1.0, 1.0, 1.0]);
        let g = gltf_in::Gltf::parse(&serde_json::to_vec(&gl).unwrap(), None).unwrap();
        let nodes = g.mesh_nodes(&|n| n == "wall");
        assert_eq!(nodes.len(), 2);
        let dir = std::env::temp_dir().join(format!("hwvx-world-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("demo.gltf"), serde_json::to_vec(&gl).unwrap()).unwrap();
        std::fs::write(dir.join("world.jsonc"), "{ \"model\": \"demo.gltf\", \"scale\": 2, \"rotateY\": 90, \"leaveOut\": [\"wall\"], \"start\": [0, 10, -600, 0] }").unwrap();
        let p = gltf_in::Project::read(&dir.join("world.jsonc")).unwrap();
        let (back, _) = gltf_in::import(&p).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(back.meshes.len(), 2);
        // rotateY 90 takes +z to +x; scale 2
        let s = back.start;
        assert!((s[0] - -1200.0).abs() < 1e-3 && (s[1] - 20.0).abs() < 1e-3 && s[2].abs() < 1e-3 && s[3] == 90.0, "{s:?}");
        // the mirrored ramp's slope still faces up
        let up = back.meshes[1].parts[0].tris.iter().filter(|t| normal(&t.p)[1] > 0.5).count();
        assert_eq!(up, 2);
    }
}

/// The project file of the demo's glTF (`hwvx-x world demo --gltf`): the
/// sample a modder starts from.
pub fn demo_project() -> String {
    format!(
        r#"// A custom world from a glTF (hwvx-x world build world.jsonc OUT; docs/plans/custom-worlds.md).
// Positions are model units times "scale" (game units), turned "rotateY" degrees about the up axis.
// Textures are indices into the base level's world texture pack (Joyride world1's w1_tex here),
// surfaces indices into its collision materials; null: vertex colour only.
{{
  "id": "uid://hwvx-world-sample-ramp",
  "name": "Sample Ramp",
  "base": 0,
  "model": "demo.gltf",
  "scale": 1.0,
  "rotateY": 0,
  "leaveOut": [],
  "renderOnly": [],
  "collisionOnly": [],
  "materials": {{
    "ground": {{ "texture": {DEMO_TEX_GROUND}, "surface": 0 }},
    "ramp": {{ "texture": {DEMO_TEX_RAMP}, "surface": 0 }},
    "wall": {{ "texture": {DEMO_TEX_WALL}, "surface": 0 }}
  }},
  "start": [0, 20, -1200, 0]
}}
"#
    )
}

#[cfg(test)]
mod env_tests {
    use super::*;

    /// A real modeller's world, given by HWVX_TEST_LAVA_CAGE=<project .jsonc>
    /// (the user's own level, never in the repository; skipped without it):
    /// it imports and builds, its start has ground under it, and the built
    /// collision matches its solid surfaces from above almost everywhere.
    #[test]
    fn lava_cage_from_env() {
        let Ok(path) = std::env::var("HWVX_TEST_LAVA_CAGE") else {
            eprintln!("HWVX_TEST_LAVA_CAGE not set: skipped");
            return;
        };
        let p = gltf_in::Project::read(std::path::Path::new(&path)).unwrap();
        let (src, rep) = gltf_in::import(&p).unwrap();
        assert!(rep.nodes.len() > 1);
        let b = build(&src, &terrain::Options::default()).unwrap();
        let s = src.start.map(|x| x as f64);
        assert!(!b.grid.solid([s[0], s[1], s[2]]), "the start is inside the solid");
        assert!(b.grid.ground_below([s[0], s[1], s[2]], 1.0e5, 1.0).is_some(), "no ground under the start");
        let (have, ok, bad) = terrain::surface_check(&src.collision, &b.grid, 200, 0.5);
        let share = ok as f64 / have as f64;
        eprintln!("surfaces: {ok} of {have} ({:.2}%), worst {:?}", 100.0 * share, bad.first());
        assert!(share > 0.995, "{:.2}% of the surfaces match", 100.0 * share);
    }
}

#[cfg(test)]
mod cap_tests {
    use super::*;

    /// A mesa (an open-bottomed block 450 high) on the ground with a sheet through it at 100
    /// (a grate a modeller left running under the rock): the cells inside the mass whose top
    /// is in another cell must read solid, not air (the hex pillars' and the ramp's trouble).
    #[test]
    fn mass_without_its_top_in_the_cell() {
        let col = |ts: Vec<Tri>, object: u32| -> Vec<ColTri> {
            ts.iter().map(|t| ColTri { p: t.p.map(|v| v.map(|x| x as f64)), material: 0, object }).collect()
        };
        let uv = [[0.0, 0.0]; 4];
        let ground = quad([[-1000.0, 0.0, -1000.0], [-1000.0, 0.0, 1000.0], [1000.0, 0.0, 1000.0], [1000.0, 0.0, -1000.0]], uv, [9; 3]).to_vec();
        let mut mesa = box_faces([-300.0, 0.0, -300.0], [300.0, 450.0, 300.0], [9; 3]);
        mesa.retain(|t| normal(&t.p)[1] > -0.5); // no bottom
        let sheet = quad([[-600.0, 100.0, -600.0], [-600.0, 100.0, 600.0], [600.0, 100.0, 600.0], [600.0, 100.0, -600.0]], uv, [9; 3]).to_vec();
        let mut tris = col(ground, 0);
        tris.extend(col(mesa, 1));
        tris.extend(col(sheet, 2));
        let g = terrain::build_grid(&tris, &terrain::Options::default()).unwrap();
        for y in [20.0, 99.0, 101.0, 150.0, 250.0, 380.0, 449.0] {
            for (x, z) in [(0.0, 0.0), (-250.0, 200.0), (280.0, -280.0)] {
                assert!(g.solid([x, y, z]), "air inside the mesa at {x} {y} {z}");
            }
        }
        assert!(!g.solid([0.0, 451.0, 0.0]) && !g.solid([450.0, 101.0, 0.0]) && g.solid([450.0, 99.0, 0.0]));
        let (_, phantom, hole) = terrain::volume_check(&tris, &g, 7);
        assert_eq!((phantom, hole), (0, 0));
    }
}

/// Synthetic meshes with the weaknesses modellers' worlds have, and the collision measured on
/// them with each repair off and on (`cargo test --release synthetic -- --nocapture` prints the
/// table). Each is general, none is one world's.
#[cfg(test)]
mod synthetic {
    use super::*;
    use terrain::{build_grid, face_check, volume_check, ColTri, Options, V3};

    fn tri(p: [V3; 3], object: u32) -> ColTri {
        ColTri { p, material: 0, object }
    }
    /// A quad a b c d (counter-clockwise from its front) as two triangles.
    fn quad(q: [V3; 4], object: u32) -> Vec<ColTri> {
        vec![tri([q[0], q[1], q[2]], object), tri([q[0], q[2], q[3]], object)]
    }
    /// The ground: an open plane at y 0, `n` x `n` quads of `s`.
    fn ground(n: usize, s: f64, object: u32) -> Vec<ColTri> {
        let h = n as f64 * s / 2.0;
        let mut out = vec![];
        for i in 0..n {
            for j in 0..n {
                let (x, z) = (-h + i as f64 * s, -h + j as f64 * s);
                out.extend(quad([[x, 0.0, z], [x, 0.0, z + s], [x + s, 0.0, z + s], [x + s, 0.0, z]], object));
            }
        }
        out
    }
    /// An open-bottomed hexagonal pillar (walls and a top, no bottom) from y0 to y1.
    fn hex_pillar(cx: f64, cz: f64, r: f64, y0: f64, y1: f64, object: u32) -> Vec<ColTri> {
        let p: Vec<(f64, f64)> = (0..6).map(|k| {
            let a = std::f64::consts::PI / 3.0 * k as f64;
            (cx + r * a.cos(), cz + r * a.sin())
        }).collect();
        let mut out = vec![];
        for k in 0..6 {
            let (a, b) = (p[k], p[(k + 1) % 6]);
            // outward: counter-clockwise from outside
            out.extend(quad([[a.0, y0, a.1], [a.0, y1, a.1], [b.0, y1, b.1], [b.0, y0, b.1]], object));
            out.push(tri([[cx, y1, cz], [b.0, y1, b.1], [a.0, y1, a.1]], object));
        }
        // make each face point out, whatever the winding above came to
        for t in out.iter_mut() {
            let pl = terrain::plane_of(&t.p).unwrap();
            let c: V3 = std::array::from_fn(|k| (t.p[0][k] + t.p[1][k] + t.p[2][k]) / 3.0);
            let out_dir = [c[0] - cx, if pl[1].abs() > 0.5 { 1.0 } else { 0.0 }, c[2] - cz];
            if pl[0] * out_dir[0] + pl[1] * out_dir[1] + pl[2] * out_dir[2] < 0.0 {
                t.p.swap(1, 2);
            }
        }
        out
    }

    fn measure(name: &str, tris: &[ColTri]) -> Vec<(String, usize, usize, usize)> {
        let mut rows = vec![];
        for (label, opt) in [
            ("none", Options { decks: false, skirts: false, caps: false, ..Default::default() }),
            ("decks", Options { skirts: false, caps: false, ..Default::default() }),
            ("+skirts", Options { caps: false, ..Default::default() }),
            ("+caps (all)", Options::default()),
        ] {
            let g = build_grid(tris, &opt).unwrap();
            let (n, ph, ho) = volume_check(tris, &g, 7);
            let f = face_check(tris, &g);
            let (fs, fp, fh) = f.values().fold((0, 0, 0), |a, s| (a.0 + s.samples, a.1 + s.phantom, a.2 + s.hole));
            eprintln!("{name:22} {label:12} volume {n:6}: phantom {ph:5} hole {ho:5} | faces {fs:5}: phantom {fp:4} hole {fh:4}");
            rows.push((label.to_string(), ph + ho, fp + fh, n));
        }
        rows
    }

    fn all_clean(rows: &[(String, usize, usize, usize)]) {
        let last = rows.last().unwrap();
        assert_eq!((last.1, last.2), (0, 0), "{} still has errors", last.0);
    }

    /// An open-bottomed hex pillar sunk into the ground, taller than a square.
    #[test]
    fn synthetic_hex_pillar() {
        let mut t = ground(8, 100.0, 0);
        t.extend(hex_pillar(30.0, -20.0, 60.0, -15.0, 330.0, 1));
        let rows = measure("hex pillar", &t);
        all_clean(&rows);
        let g = build_grid(&t, &Options::default()).unwrap();
        for y in [1.0, 100.0, 199.0, 201.0, 329.0] {
            assert!(g.solid([30.0, y, -20.0]), "inside the pillar at {y}");
            assert!(!g.solid([150.0, y, -20.0]), "beside the pillar at {y}");
        }
        assert!((g.ground_below([30.0, 400.0, -20.0], 500.0, 0.25).unwrap() - 330.0).abs() < 0.3);
    }

    /// A one-sided ramp (a single sloped sheet) from the ground up to the tops of two pillars.
    #[test]
    fn synthetic_ramp_between_pillars() {
        let mut t = ground(8, 100.0, 0);
        t.extend(hex_pillar(-150.0, 0.0, 60.0, -10.0, 150.0, 1));
        t.extend(hex_pillar(150.0, 0.0, 60.0, -10.0, 150.0, 2));
        // from z -300 at the ground up to z 0 at 150, between the pillars, open all round
        t.extend(quad([[-90.0, 0.0, -300.0], [-90.0, 150.0, 0.0], [90.0, 150.0, 0.0], [90.0, 0.0, -300.0]], 3));
        let rows = measure("ramp between pillars", &t);
        all_clean(&rows);
        let g = build_grid(&t, &Options::default()).unwrap();
        for z in [-280.0, -200.0, -100.0, -20.0] {
            let want = 150.0 * (z + 300.0) / 300.0;
            let got = g.ground_below([0.0, want + 30.0, z], 300.0, 0.1).unwrap();
            assert!((got - want).abs() < 0.3, "the ramp at z {z}: {got} not {want}");
        }
        // the high end of a one-sided ramp is a slab: room under it
        assert!(!g.solid([0.0, 60.0, -20.0]) && g.solid([0.0, 148.0, -2.0]));
    }

    /// Two halves of a floor that were never welded: an overlap of 0.5 and a gap of 0.01.
    #[test]
    fn synthetic_unwelded_seams() {
        let mut t = vec![];
        t.extend(quad([[-400.0, 0.0, -400.0], [-400.0, 0.0, 400.0], [0.25, 0.0, 400.0], [0.25, 0.0, -400.0]], 0));
        t.extend(quad([[-0.25, 0.0, -400.0], [-0.25, 0.0, 0.0], [400.0, 0.0, 0.0], [400.0, 0.0, -400.0]], 1));
        t.extend(quad([[0.0, 0.0, 0.01], [0.0, 0.0, 400.0], [400.0, 0.0, 400.0], [400.0, 0.0, 0.01]], 2));
        // and a box standing on the seam
        t.extend(hex_pillar(0.0, 0.0, 40.0, 0.0, 60.0, 3));
        let rows = measure("unwelded seams", &t);
        all_clean(&rows);
        let g = build_grid(&t, &Options::default()).unwrap();
        for (x, z) in [(0.0, -200.0), (0.2, 200.0), (200.0, 0.005), (-200.0, 0.0), (0.1, 0.1 + 41.0)] {
            assert!(g.ground_below([x, 20.0, z], 50.0, 0.1).is_some_and(|y| y.abs() < 0.2), "a hole in the floor at {x} {z}");
        }
    }

    /// A ring of touching hex pillars round a pool (a lower floor sheet inside), a grate sheet
    /// laid over the ring at the pillars' top, and a sheet running under it all (what the
    /// modeller left of a walkway): masses whose tops are in the squares above.
    #[test]
    fn synthetic_pillar_ring_and_grate() {
        let mut t = ground(10, 100.0, 0);
        let r = 50.0;
        for k in 0..6 {
            let a = std::f64::consts::PI / 3.0 * k as f64;
            t.extend(hex_pillar(150.0 * a.cos(), 150.0 * a.sin(), r, -10.0, 300.0, 1 + k));
        }
        t.extend(quad([[-120.0, 200.0, -120.0], [-120.0, 200.0, 120.0], [120.0, 200.0, 120.0], [120.0, 200.0, -120.0]], 10));
        t.extend(quad([[-220.0, 300.0, -220.0], [-220.0, 300.0, 220.0], [220.0, 300.0, 220.0], [220.0, 300.0, -220.0]], 11));
        t.extend(quad([[-400.0, 40.0, -400.0], [-400.0, 40.0, 400.0], [400.0, 40.0, 400.0], [400.0, 40.0, -400.0]], 12));
        let rows = measure("pillar ring + grate", &t);
        all_clean(&rows);
        let g = build_grid(&t, &Options::default()).unwrap();
        for y in [50.0, 150.0, 250.0, 297.0] {
            assert!(g.solid([150.0, y, 0.0]), "inside a pillar at {y}");
        }
        assert!(!g.solid([0.0, 250.0, 0.0]) && !g.solid([0.0, 320.0, 0.0]) && g.solid([0.0, 298.0, 0.0]));
    }

    /// A thin one-sided deck floating over the ground: a car fits under it.
    #[test]
    fn synthetic_floating_deck() {
        let mut t = ground(6, 100.0, 0);
        t.extend(quad([[-100.0, 60.0, -150.0], [-100.0, 60.0, 150.0], [100.0, 60.0, 150.0], [100.0, 60.0, -150.0]], 1));
        let rows = measure("floating deck", &t);
        all_clean(&rows);
        let g = build_grid(&t, &Options::default()).unwrap();
        assert!(!g.solid([0.0, 30.0, 0.0]), "no room under the deck");
        assert!(g.solid([0.0, 58.0, 0.0]) && !g.solid([0.0, 61.0, 0.0]) && !g.solid([150.0, 30.0, 0.0]));
        assert!((g.ground_below([0.0, 50.0, 0.0], 100.0, 0.1).unwrap()).abs() < 0.2);
    }
}
