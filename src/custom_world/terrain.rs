//! A world's collision pack (type 5) from triangles: the terrain grid of
//! `docs/worlds/README.md` ("Collision packs", "BSP build rules"), ported from
//! HWVX-SDL's `tools/xdl_chunk.py` (`build_collision`, `write_collision`) and
//! `tools/bsp_build.py` (`build`).
//!
//! * The grid: squares of about 250 x 200 x 250 over the triangles' bounds
//!   (grown by 1, and by a square's height above), each a whole fraction of
//!   the span.
//! * Per square: the triangles clipped to its box; where several solids meet
//!   there (each connected piece of a mesh node is one), their union's
//!   boundary ([`union_polys`]), closed by the parts of the cell's walls
//!   inside the solid ([`box_caps`]); built into a solid-leaf tree in the order that
//!   best matches "under a surface" ([`Objects::inside`]; the first polygon is
//!   the splitter, the source order first): coplanar
//!   polygons facing the same way go in front, facing the other way are
//!   dropped (two solids touching there, a box standing on the ground: an
//!   inside face), tolerance 1e-4, pre-order. A cell's solid only matters in
//!   its own box (`RayHitsBSP` drops hits outside it).
//! * Before that, floating one-sided decks become slabs ([`solidify_decks`]) and the open
//!   edges of floors get skirts down to the bottom ([`skirts`]); [`volume_check`] and
//!   [`face_check`] measure the result.
//! * Crease edges: where two faces of the welded mesh meet at more than 0.5
//!   degrees, listed (u16) in every cell whose box grown by 10 the edge
//!   crosses.
//! * Materials are indices into the *base* collision pack's surface table: the
//!   pack written here has no table of its own (`materials` 0); the game
//!   (`src/game/Port_World.c`) puts the base pack's in.

use std::collections::HashMap;

use anyhow::{bail, ensure, Result};

use crate::blob::Blob;
use crate::format::{align, Endian};

pub type V3 = [f64; 3];
pub type Plane = [f64; 4];

/// A collision triangle (counter-clockwise seen from the open side) and its
/// surface (an index into the base collision pack's materials).
#[derive(Clone, Debug)]
pub struct ColTri {
    pub p: [V3; 3],
    pub material: u32,
    /// The solid it bounds (a mesh node): where two overlap in a cell, each
    /// one's faces inside the other are taken out first (a union).
    pub object: u32,
}

/// Build settings.
#[derive(Clone, Debug)]
pub struct Options {
    /// Target square size across (x, z) and up (y).
    pub cell_xz: f64,
    pub cell_y: f64,
    /// Faces meeting at more than this many degrees make a crease edge.
    pub crease_deg: f64,
    /// The repairs, each on by default (off only to measure what it buys): floating decks
    /// made slabs ([`solidify_decks`]), skirts under open floor edges ([`skirts`]), the cell
    /// walls inside the solid ([`box_caps`]) and the solid squares beside the surfaced ones.
    pub decks: bool,
    pub skirts: bool,
    pub caps: bool,
    pub fill: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options { cell_xz: 250.0, cell_y: 200.0, crease_deg: 0.5, decks: true, skirts: true, caps: true, fill: true }
    }
}

const EPS: f64 = 1e-4;
/// Planes this close (normals within 0.26 degrees, the polygon within 0.01 units of the
/// splitter) are one plane in a tree ([`build_tree`]).
pub const COPLANAR_COS: f64 = 0.99999;
pub const COPLANAR_DIST: f64 = 0.01;
/// Keep near-coplanar polygons as nodes of their own (the old way), to measure what dropping
/// them buys (`hwvx-x world build --without coplanar`).
pub static KEEP_COPLANAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const EDGE_REACH: f64 = 10.0;
const LEAF: u16 = 0xFFFF;
pub const VERSION: u32 = 0xD;

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn len(a: V3) -> f64 {
    dot(a, a).sqrt()
}

/// The plane of a counter-clockwise polygon (Newell), normal towards the
/// viewer, rounded to f32 as the pack stores it.
pub fn plane_of(v: &[V3]) -> Option<Plane> {
    let mut n = [0.0f64; 3];
    for i in 0..v.len() {
        let c = cross(v[i], v[(i + 1) % v.len()]);
        n = [n[0] + c[0], n[1] + c[1], n[2] + c[2]];
    }
    let ln = len(n);
    if ln < 1e-12 {
        return None;
    }
    let n = [n[0] / ln, n[1] / ln, n[2] / ln];
    let k = v.len() as f64;
    let mut m = [0.0f64; 3];
    for x in v {
        for j in 0..3 {
            m[j] += x[j] / k;
        }
    }
    Some([n[0], n[1], n[2], -dot(n, m)].map(|x| x as f32 as f64))
}

#[derive(Clone, Debug)]
pub struct Poly {
    pub plane: Plane,
    pub verts: Vec<V3>,
    pub material: u32,
}

enum Side {
    On,
    Front,
    Back,
    Span(Option<Vec<V3>>, Option<Vec<V3>>),
}

fn dist(p: V3, plane: Plane) -> f64 {
    p[0] * plane[0] + p[1] * plane[1] + p[2] * plane[2] + plane[3]
}

fn split_poly(verts: &[V3], plane: Plane, eps: f64) -> Side {
    let d: Vec<f64> = verts.iter().map(|v| dist(*v, plane)).collect();
    let pos = d.iter().any(|&x| x > eps);
    let neg = d.iter().any(|&x| x < -eps);
    if !pos && !neg {
        return Side::On;
    }
    if !neg {
        return Side::Front;
    }
    if !pos {
        return Side::Back;
    }
    let (mut fr, mut bk) = (vec![], vec![]);
    let m = verts.len();
    for i in 0..m {
        let (a, b, da, db) = (verts[i], verts[(i + 1) % m], d[i], d[(i + 1) % m]);
        if da >= -eps {
            fr.push(a);
        }
        if da <= eps {
            bk.push(a);
        }
        if (da > eps && db < -eps) || (da < -eps && db > eps) {
            let t = da / (da - db);
            let x = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
            fr.push(x);
            bk.push(x);
        }
    }
    Side::Span((fr.len() >= 3).then_some(fr), (bk.len() >= 3).then_some(bk))
}

/// A terrain node: plane, children (indices, pre-order), material.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub plane: Plane,
    pub front: Option<usize>,
    pub back: Option<usize>,
    pub material: u32,
}

/// The solid-leaf tree of `polys` (bsp_build.build, opposite = "drop").
pub fn build_tree(polys: Vec<Poly>, limit: usize) -> Result<Vec<Node>> {
    fn rec(items: Vec<Poly>, tree: &mut Vec<Option<Node>>, limit: usize) -> Result<usize> {
        let k = tree.len();
        ensure!(k < limit, "a collision cell of more than {limit} nodes: use smaller squares");
        let mut it = items.into_iter();
        let first = it.next().unwrap();
        let plane = first.plane;
        tree.push(None);
        let n = [plane[0], plane[1], plane[2]];
        let (mut front, mut back) = (vec![], vec![]);
        for p in it {
            // a polygon in (or within a hair of) the splitter's plane facing the same way says
            // nothing the splitter does not: dropped (one exactly in it facing the other way
            // goes too, below: two solids touch there; one a hair off it facing the other way
            // stays, it may be the other side of a thin sheet). Kept as a node of its own (the exporter's rule) a nearly
            // coplanar one leaves a solid leaf as thin as the float error behind its plane,
            // which the game's ray (RayHitsBSP2) hits wherever it ends a piece of a ray on
            // the parent's plane: a phantom face anywhere along that plane in the cell. Many
            // small triangles of one gentle surface in a cell (a fine mesh, a small scale)
            // made many of them: the car's box corners struck them and its nose lifted.
            let pn = [p.plane[0], p.plane[1], p.plane[2]];
            if !KEEP_COPLANAR.load(std::sync::atomic::Ordering::Relaxed) && dot(n, pn) > COPLANAR_COS && p.verts.iter().all(|v| dist(*v, plane).abs() < COPLANAR_DIST) {
                continue;
            }
            match split_poly(&p.verts, plane, EPS) {
                Side::On => {
                    if KEEP_COPLANAR.load(std::sync::atomic::Ordering::Relaxed) && dot(n, pn) > 0.0 {
                        front.push(p);
                    }
                }
                Side::Front => front.push(p),
                Side::Back => back.push(p),
                Side::Span(f, b) => {
                    if let Some(f) = f {
                        front.push(Poly { plane: p.plane, verts: f, material: p.material });
                    }
                    if let Some(b) = b {
                        back.push(Poly { plane: p.plane, verts: b, material: p.material });
                    }
                }
            }
        }
        let fi = if front.is_empty() { None } else { Some(rec(front, tree, limit)?) };
        let bi = if back.is_empty() { None } else { Some(rec(back, tree, limit)?) };
        tree[k] = Some(Node { plane, front: fi, back: bi, material: first.material });
        Ok(k)
    }
    let mut tree = vec![];
    if !polys.is_empty() {
        rec(polys, &mut tree, limit)?;
    }
    Ok(tree.into_iter().map(|n| n.unwrap()).collect())
}

/// Point in the solid (the terrain's rule: distance >= 0 is in front).
pub fn inside(tree: &[Node], p: V3) -> bool {
    let mut k = 0;
    loop {
        let n = &tree[k];
        if dist(p, n.plane) >= 0.0 {
            match n.front {
                Some(f) => k = f,
                None => return false,
            }
        } else {
            match n.back {
                Some(b) => k = b,
                None => return true,
            }
        }
    }
}

fn clip_to_box(mut v: Vec<V3>, lo: V3, hi: V3) -> Option<Vec<V3>> {
    for axis in 0..3 {
        for (sign, lim) in [(1.0, hi[axis]), (-1.0, lo[axis])] {
            let mut pl = [0.0, 0.0, 0.0, -sign * lim];
            pl[axis] = sign;
            match split_poly(&v, pl, 1e-6) {
                Side::Front => return None,
                Side::Span(_, b) => v = b?,
                _ => {}
            }
        }
    }
    Some(v)
}

fn area(v: &[V3]) -> f64 {
    let mut s = [0.0; 3];
    for i in 0..v.len() {
        let c = cross(v[i], v[(i + 1) % v.len()]);
        s = [s[0] + c[0], s[1] + c[1], s[2] + c[2]];
    }
    len(s) / 2.0
}

/// Twice the area over the longest edge: a sliver cut off at a wall is ~0.
fn thick(v: &[V3]) -> f64 {
    let e = (0..v.len()).map(|i| len(sub(v[(i + 1) % v.len()], v[i]))).fold(0.0, f64::max);
    if e > 0.0 {
        2.0 * area(v) / e
    } else {
        0.0
    }
}

/// A polygon lying in a side of the box and facing into it (a neighbour's
/// surface on the shared wall): in a solid-leaf tree it would declare the
/// whole box empty.
fn faces_in(v: &[V3], plane: Plane, lo: V3, hi: V3) -> bool {
    for axis in 0..3 {
        for (lim, inward) in [(lo[axis], 1.0), (hi[axis], -1.0)] {
            if v.iter().all(|p| (p[axis] - lim).abs() < 1e-3) && plane[axis] * inward > 0.5 {
                return true;
            }
        }
    }
    false
}

/// Vertex ids of the triangles, welded at `eps`.
fn weld(tris: &[ColTri], eps: f64) -> Vec<[usize; 3]> {
    let mut ids: HashMap<[i64; 3], usize> = HashMap::new();
    tris.iter()
        .map(|t| {
            t.p.map(|p| {
                let key = p.map(|x| (x / eps).round() as i64);
                let n = ids.len();
                *ids.entry(key).or_insert(n)
            })
        })
        .collect()
}

/// A crease edge: two points and a material.
pub type Edge = (V3, V3, u32);

/// Edges where two faces meet at more than `deg` degrees, and how many edges
/// of the welded mesh are open (one face) or shared by more than two.
pub fn crease_edges(tris: &[ColTri], deg: f64, buried: &dyn Fn(V3) -> bool) -> (Vec<Edge>, usize, usize) {
    let ids = weld(tris, 1e-3);
    let normals: Vec<Option<V3>> = tris.iter().map(|t| plane_of(&t.p).map(|p| [p[0], p[1], p[2]])).collect();
    let mut faces: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    let mut order = vec![];
    for (i, t) in ids.iter().enumerate() {
        if normals[i].is_none() {
            continue;
        }
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let key = (a.min(b), a.max(b));
            let e = faces.entry(key).or_default();
            if e.is_empty() {
                order.push(key);
            }
            e.push((i, k));
        }
    }
    let cos = deg.to_radians().cos();
    let (mut out, mut open, mut many) = (vec![], 0, 0);
    for key in order {
        let fs = &faces[&key];
        match fs.len() {
            1 => open += 1,
            2 => {
                let ((i, k), (j, _)) = (fs[0], fs[1]);
                let (ni, nj) = (normals[i].unwrap(), normals[j].unwrap());
                // an edge where faces inside another solid meet (a pillar's foot under the
                // ground, a mesh running through itself) is no crease of the collision: a box
                // near it would catch on it
                let (a, b) = (tris[i].p[k], tris[i].p[(k + 1) % 3]);
                let s = [ni[0] + nj[0], ni[1] + nj[1], ni[2] + nj[2]];
                let ls = len(s).max(1e-9);
                let off = [(a[0] + b[0]) / 2.0 + s[0] / ls * 0.05, (a[1] + b[1]) / 2.0 + s[1] / ls * 0.05, (a[2] + b[2]) / 2.0 + s[2] / ls * 0.05];
                if dot(ni, nj) < cos && !buried(off) {
                    out.push((tris[i].p[k], tris[i].p[(k + 1) % 3], tris[i].material));
                }
            }
            _ => many += 1,
        }
    }
    (out, open, many)
}

fn seg_hits_box(a: V3, b: V3, lo: V3, hi: V3) -> bool {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for k in 0..3 {
        let d = b[k] - a[k];
        if d.abs() < 1e-12 {
            if a[k] < lo[k] || a[k] > hi[k] {
                return false;
            }
            continue;
        }
        let (mut u0, mut u1) = ((lo[k] - a[k]) / d, (hi[k] - a[k]) / d);
        if u0 > u1 {
            std::mem::swap(&mut u0, &mut u1);
        }
        t0 = t0.max(u0);
        t1 = t1.min(u1);
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// A built grid.
#[derive(Clone, Debug)]
pub struct Grid {
    pub lo: V3,
    pub hi: V3,
    pub n: [usize; 3],
    pub size: V3,
    /// Per square (x fastest, then z, then y): 0 or its cell's number (from 1).
    pub index: Vec<u32>,
    pub edges: Vec<Edge>,
    /// Per cell: its box, tree and edge indices.
    pub cells: Vec<Cell>,
    /// Edges of the welded mesh with one face / more than two.
    pub open_edges: usize,
    pub shared_edges: usize,
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub lo: V3,
    pub hi: V3,
    pub tree: Vec<Node>,
    pub edges: Vec<u16>,
}

impl Grid {
    /// The square holding a point, if inside the grid.
    pub fn square(&self, p: V3) -> Option<[usize; 3]> {
        let mut s = [0usize; 3];
        for k in 0..3 {
            let f = ((p[k] - self.lo[k]) / self.size[k]).floor();
            if f < 0.0 || f >= self.n[k] as f64 {
                return None;
            }
            s[k] = f as usize;
        }
        Some(s)
    }

    /// The first solid point going down from `p` (the game's ray, simplified:
    /// cell by cell, the first surface crossed into a solid), by steps of
    /// `step`: the ground height under a point, for tests.
    pub fn ground_below(&self, p: V3, depth: f64, step: f64) -> Option<f64> {
        let mut y = p[1];
        let mut was_solid = self.solid([p[0], y, p[2]]);
        if was_solid {
            return Some(y);
        }
        while y > p[1] - depth {
            let ny = y - step;
            let s = self.solid([p[0], ny, p[2]]);
            if s && !was_solid {
                // bisect the crossing
                let (mut a, mut b) = (ny, y);
                for _ in 0..40 {
                    let m = (a + b) / 2.0;
                    if self.solid([p[0], m, p[2]]) {
                        a = m;
                    } else {
                        b = m;
                    }
                }
                return Some(b);
            }
            was_solid = s;
            y = ny;
        }
        None
    }

    /// Whether a point is in the solid of its square's cell (empty squares:
    /// not solid).
    pub fn solid(&self, p: V3) -> bool {
        let Some([x, y, z]) = self.square(p) else { return false };
        let c = self.index[x + self.n[0] * (z + self.n[2] * y)];
        c != 0 && {
            let t = &self.cells[c as usize - 1].tree;
            !t.is_empty() && inside(t, p)
        }
    }
}

/// The triangles with each object split into its connected pieces (by welded
/// vertices): a modeller's mesh often holds several solids that overlap (a
/// roof slab through the walls under it), and the union needs each solid on
/// its own. Object numbers from 0, in order of first appearance.
pub fn split_components(tris: &[ColTri]) -> Vec<ColTri> {
    let ids = weld(tris, 1e-3);
    let n = ids.iter().flatten().max().map_or(0, |m| m + 1);
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for t in &ids {
        for k in 1..3 {
            let (a, b) = (find(&mut parent, t[0]), find(&mut parent, t[k]));
            parent[a] = b;
        }
    }
    let mut number: HashMap<(u32, usize), u32> = HashMap::new();
    tris.iter()
        .zip(&ids)
        .map(|(t, v)| {
            let root = find(&mut parent, v[0]);
            let next = number.len() as u32;
            let object = *number.entry((t.object, root)).or_insert(next);
            ColTri { object, ..t.clone() }
        })
        .collect()
}

/// The grid of a collision mesh.
pub fn build_grid(tris: &[ColTri], opt: &Options) -> Result<Grid> {
    ensure!(!tris.is_empty(), "no collision triangles");
    let split = if opt.decks { prepare(tris) } else { split_components(tris) };
    let tris = &split[..];
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for t in tris {
        for p in &t.p {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    // grown by 1, and by a square's height above: a car on the highest
    // surface must be inside a cell (hits outside a cell's box are dropped)
    let lo = lo.map(|x| x - 1.0);
    let hi = [hi[0] + 1.0, hi[1] + 1.0 + opt.cell_y, hi[2] + 1.0];
    let span = sub(hi, lo);
    let target = [opt.cell_xz, opt.cell_y, opt.cell_xz];
    let n: [usize; 3] = std::array::from_fn(|k| ((span[k] / target[k]).ceil() as usize).max(1));
    ensure!(n[0] * n[1] * n[2] <= 0x100000, "a collision grid of {} x {} x {} squares: too many", n[0], n[1], n[2]);
    let size: V3 = std::array::from_fn(|k| span[k] / n[k] as f64);
    // the squares' walls kept off the flat faces: a face a hair inside a square, its solid cut
    // off by the wall, is a slab as thin as that hair, and the game's ray reports a slab's far
    // side (a plain 0.14 above a square's floor pushed cars down into it)
    let (lo, n, hi) = shift_off_faces(tris, lo, n, size);
    let (nx, ny, nz) = (n[0], n[1], n[2]);

    // polygons per square, in source order, then the skirts
    let objects = Objects::new(tris, lo, size, nx, nz);
    let skirts = if opt.skirts { skirts(tris, lo[1], &objects) } else { vec![] };
    let mut per: HashMap<[usize; 3], Vec<(u32, Poly)>> = HashMap::new();
    for t in tris.iter().chain(&skirts) {
        let Some(plane) = plane_of(&t.p) else { continue };
        let a: [usize; 3] = std::array::from_fn(|k| {
            let m = t.p.iter().map(|p| p[k]).fold(f64::MAX, f64::min) - CELL_OVERLAP;
            (((m - lo[k]) / size[k]).floor().max(0.0) as usize).min(n[k] - 1)
        });
        let b: [usize; 3] = std::array::from_fn(|k| {
            let m = t.p.iter().map(|p| p[k]).fold(f64::MIN, f64::max) + CELL_OVERLAP;
            (((m - lo[k]) / size[k]).floor().max(0.0) as usize).min(n[k] - 1)
        });
        for x in a[0]..=b[0] {
            for y in a[1]..=b[1] {
                for z in a[2]..=b[2] {
                    let clo = [lo[0] + size[0] * x as f64, lo[1] + size[1] * y as f64, lo[2] + size[2] * z as f64];
                    let chi = [clo[0] + size[0], clo[1] + size[1], clo[2] + size[2]];
                    let (clo, chi) = (clo.map(|v| v - CELL_OVERLAP), chi.map(|v| v + CELL_OVERLAP));
                    let Some(piece) = clip_to_box(t.p.to_vec(), clo, chi) else { continue };
                    if piece.len() < 3 || thick(&piece) <= 1e-2 || faces_in(&piece, plane, clo, chi) {
                        continue;
                    }
                    per.entry([x, y, z]).or_default().push((t.object, Poly { plane, verts: piece, material: t.material }));
                }
            }
        }
    }

    let (edges, open_edges, shared_edges) = crease_edges(tris, opt.crease_deg, &|p| objects.inside_any(p));
    ensure!(edges.len() <= 0xFFFF, "{} crease edges: the cells' edge indices are u16", edges.len());
    // the squares with surfaces, and the solid ones next to them (a car pushed through a cap
    // meets solid, not an empty square)
    let mut keys: Vec<[usize; 3]> = per.keys().copied().collect();
    let mut solid_next: Vec<[usize; 3]> = vec![];
    for k in &keys {
        for (a, d) in [(0, -1i64), (0, 1), (1, -1), (1, 1), (2, -1), (2, 1)] {
            let mut s = *k;
            let v = s[a] as i64 + d;
            if v < 0 || v >= n[a] as i64 {
                continue;
            }
            s[a] = v as usize;
            if per.contains_key(&s) || solid_next.contains(&s) {
                continue;
            }
            let c: V3 = std::array::from_fn(|j| lo[j] + size[j] * (s[j] as f64 + 0.5));
            if objects.inside_any(c) {
                solid_next.push(s);
            }
        }
    }
    if opt.caps && opt.fill {
        keys.extend(&solid_next);
    }
    keys.sort_by_key(|k| (k[1], k[2], k[0]));
    let mut index = vec![0u32; nx * ny * nz];
    let mut cells = vec![];
    for key in keys {
        let [x, y, z] = key;
        let clo = [lo[0] + size[0] * x as f64, lo[1] + size[1] * y as f64, lo[2] + size[2] * z as f64];
        let chi = [clo[0] + size[0], clo[1] + size[1], clo[2] + size[2]];
        let mut polys = union_polys(per.remove(&key).unwrap_or_default(), &objects, 0xFFFF)?;
        if opt.caps {
            let (glo, ghi) = (clo.map(|v| v - CELL_OVERLAP), chi.map(|v| v + CELL_OVERLAP));
            polys.extend(box_caps(&polys, &objects, glo, ghi, 0xFFFF)?);
        }
        let tree = best_tree(polys, &objects, clo, chi, 0xFFFF)?;
        if tree.is_empty() {
            continue;
        }
        let blo = clo.map(|v| v - EDGE_REACH);
        let bhi = chi.map(|v| v + EDGE_REACH);
        let eids: Vec<u16> =
            edges.iter().enumerate().filter(|(_, e)| seg_hits_box(e.0, e.1, blo, bhi)).map(|(i, _)| i as u16).collect();
        cells.push(Cell { lo: clo, hi: chi, tree, edges: eids });
        index[x + nx * (z + nz * y)] = cells.len() as u32;
    }
    Ok(Grid { lo, hi, n, size, index, edges, cells, open_edges, shared_edges })
}

/// Appends with alignment; pointer slots recorded in the blob.
struct Out {
    b: Blob,
    e: Endian,
}

impl Out {
    fn alloc(&mut self, n: usize, a: usize) -> u32 {
        let at = align(self.b.data.len(), a);
        self.b.data.resize(at + n, 0);
        at as u32
    }
    fn w32(&mut self, at: u32, v: u32) {
        self.b.put_u32(self.e, at, v);
    }
    fn wf(&mut self, at: u32, v: f64) {
        self.w32(at, (v as f32).to_bits());
    }
    fn w16(&mut self, at: u32, v: u16) {
        let b = match self.e {
            Endian::Big => v.to_be_bytes(),
            Endian::Little => v.to_le_bytes(),
        };
        self.b.data[at as usize..at as usize + 2].copy_from_slice(&b);
    }
    fn ptr(&mut self, slot: u32, target: u32) {
        self.b.set_ptr(self.e, slot, target);
    }
    fn fill(&mut self, at: u32, n: usize) {
        self.b.data[at as usize..at as usize + n].fill(0xCD);
    }
}

/// The collision pack's data blob (a `TerrainGrid` at offset 0, version 0xD,
/// mode 1, no materials table).
pub fn write_grid(g: &Grid, e: Endian) -> Result<Blob> {
    let mut o = Out { b: Blob::default(), e };
    o.alloc(0x60, 32);
    let eo = if g.edges.is_empty() { 0 } else { o.alloc(0x30 * g.edges.len(), 16) };
    for (i, (a, b, m)) in g.edges.iter().enumerate() {
        let at = eo + 0x30 * i as u32;
        for k in 0..3 {
            o.wf(at + 4 * k as u32, a[k]);
            o.wf(at + 0x10 + 4 * k as u32, b[k]);
        }
        o.fill(at + 0x20, 4);
        o.w32(at + 0x24, *m);
        o.fill(at + 0x28, 8);
    }
    let ci = o.alloc(4 * g.index.len(), 16);
    for (i, &v) in g.index.iter().enumerate() {
        o.w32(ci + 4 * i as u32, v);
    }
    let co = o.alloc(0x50 * (g.cells.len() + 1), 32);
    for (k, c) in g.cells.iter().enumerate() {
        let at = co + 0x50 * (k as u32 + 1);
        ensure!(c.tree.len() < 0xFFFF, "a collision cell of {} nodes (u16 indices): use smaller squares", c.tree.len());
        for j in 0..3 {
            o.wf(at + 4 * j as u32, c.lo[j]);
            o.wf(at + 0x10 + 4 * j as u32, c.hi[j]);
        }
        let no = o.alloc(0x20 * c.tree.len(), 32);
        for (i, n) in c.tree.iter().enumerate() {
            let a = no + 0x20 * i as u32;
            for j in 0..4 {
                o.wf(a + 4 * j as u32, n.plane[j]);
            }
            o.w32(a + 0x10, n.material);
            o.w16(a + 0x14, n.front.map_or(LEAF, |f| f as u16));
            o.w16(a + 0x16, n.back.map_or(LEAF, |b| b as u16));
            o.fill(a + 0x18, 8);
        }
        o.ptr(at + 0x30, no);
        o.w32(at + 0x3C, c.tree.len() as u32);
        o.w32(at + 0x40, c.edges.len() as u32);
        if !c.edges.is_empty() {
            let n = 2 * c.edges.len();
            let lo = o.alloc(align(n, 16), 16);
            for (i, &x) in c.edges.iter().enumerate() {
                o.w16(lo + 2 * i as u32, x);
            }
            o.fill(lo + n as u32, align(n, 16) - n);
            o.ptr(at + 0x44, lo);
        }
    }
    o.alloc(0, 32);
    for k in 0..3 {
        o.wf(4 * k as u32, g.lo[k]);
        o.wf(0x10 + 4 * k as u32, g.hi[k]);
        o.wf(0x28 + 4 * k as u32, g.size[k]);
        o.w32(0x34 + 4 * k as u32, g.n[k] as u32);
    }
    o.ptr(0x20, co);
    o.w32(0x24, VERSION);
    o.ptr(0x40, ci);
    o.w32(0x44, g.cells.len() as u32 + 1);
    o.w32(0x50, g.edges.len() as u32);
    if !g.edges.is_empty() {
        o.ptr(0x54, eo);
    }
    o.w32(0x58, 1);
    Ok(o.b)
}

/// Reads a collision blob written by [`write_grid`] (or a retail one's grid)
/// back: the boxes, trees and edge lists, for tests and reports.
pub fn read_grid(b: &Blob, e: Endian) -> Result<Grid> {
    let w = |o: u32| -> Result<u32> { b.u32(e, o).ok_or_else(|| anyhow::anyhow!("read past the end at {o:#x}")) };
    let f = |o: u32| -> Result<f64> { Ok(f32::from_bits(w(o)?) as f64) };
    let h = |o: u32| -> Result<u16> {
        let s = b.data.get(o as usize..o as usize + 2).ok_or_else(|| anyhow::anyhow!("read past the end"))?;
        Ok(match e {
            Endian::Big => u16::from_be_bytes([s[0], s[1]]),
            Endian::Little => u16::from_le_bytes([s[0], s[1]]),
        })
    };
    if w(0x24)? != VERSION {
        bail!("collision version {:#x}, not 0xD", w(0x24)?);
    }
    let lo = [f(0)?, f(4)?, f(8)?];
    let hi = [f(0x10)?, f(0x14)?, f(0x18)?];
    let size = [f(0x28)?, f(0x2C)?, f(0x30)?];
    let n = [w(0x34)? as usize, w(0x38)? as usize, w(0x3C)? as usize];
    let ci = w(0x40)?;
    let index: Vec<u32> = (0..n[0] * n[1] * n[2]).map(|i| w(ci + 4 * i as u32)).collect::<Result<_>>()?;
    let ne = w(0x50)?;
    let eo = w(0x54)?;
    let mut edges = vec![];
    for i in 0..ne {
        let a = eo + 0x30 * i;
        edges.push(([f(a)?, f(a + 4)?, f(a + 8)?], [f(a + 0x10)?, f(a + 0x14)?, f(a + 0x18)?], w(a + 0x24)?));
    }
    let co = w(0x20)?;
    let count = index.iter().copied().max().unwrap_or(0);
    let mut cells = vec![];
    for k in 1..=count {
        let at = co + 0x50 * k;
        let no = w(at + 0x30)?;
        let nn = w(at + 0x3C)?;
        let mut tree = vec![];
        for i in 0..nn {
            let a = no + 0x20 * i;
            let (fr, bk) = (h(a + 0x14)?, h(a + 0x16)?);
            tree.push(Node {
                plane: [f(a)?, f(a + 4)?, f(a + 8)?, f(a + 0xC)?],
                material: w(a + 0x10)?,
                front: (fr != LEAF).then_some(fr as usize),
                back: (bk != LEAF).then_some(bk as usize),
            });
        }
        let nl = w(at + 0x40)?;
        let lo_ = w(at + 0x44)?;
        let el = (0..nl).map(|i| h(lo_ + 2 * i)).collect::<Result<_>>()?;
        cells.push(Cell { lo: [f(at)?, f(at + 4)?, f(at + 8)?], hi: [f(at + 0x10)?, f(at + 0x14)?, f(at + 0x18)?], tree, edges: el });
    }
    Ok(Grid { lo, hi, n, size, index, edges, cells, open_edges: 0, shared_edges: 0 })
}

impl Grid {
    /// The solid's top seen from above, one pixel per `px` units (x right, z
    /// down), as a binary PPM: grey by height (black: nothing), the start (x,
    /// y, z, heading in degrees: forward is sin h, 0, cos h) a red dot with a
    /// red line 400 units along its heading.
    pub fn heightmap_ppm(&self, px: f64, start: [f32; 4]) -> Vec<u8> {
        let w = ((self.hi[0] - self.lo[0]) / px).ceil() as usize;
        let h = ((self.hi[2] - self.lo[2]) / px).ceil() as usize;
        let mut top = vec![None; w * h];
        let step = 2.0;
        for j in 0..h {
            for i in 0..w {
                let (x, z) = (self.lo[0] + (i as f64 + 0.5) * px, self.lo[2] + (j as f64 + 0.5) * px);
                let mut y = self.hi[1] - 0.5;
                while y > self.lo[1] {
                    if self.solid([x, y, z]) {
                        top[j * w + i] = Some(y);
                        break;
                    }
                    y -= step;
                }
            }
        }
        let (mn, mx) = top.iter().flatten().fold((f64::MAX, f64::MIN), |(a, b), &y| (a.min(y), b.max(y)));
        let mut img = vec![0u8; w * h * 3];
        for (k, t) in top.iter().enumerate() {
            if let Some(y) = t {
                let g = (40.0 + 215.0 * (y - mn) / (mx - mn).max(1e-6)) as u8;
                img[3 * k..3 * k + 3].copy_from_slice(&[g, g, g]);
            }
        }
        let mut dot = |x: f64, z: f64, r: i64| {
            let (ci, cj) = (((x - self.lo[0]) / px) as i64, ((z - self.lo[2]) / px) as i64);
            for dj in -r..=r {
                for di in -r..=r {
                    let (i, j) = (ci + di, cj + dj);
                    if i >= 0 && j >= 0 && (i as usize) < w && (j as usize) < h {
                        let k = 3 * (j as usize * w + i as usize);
                        img[k..k + 3].copy_from_slice(&[255, 0, 0]);
                    }
                }
            }
        };
        let (s, c) = (start[3] as f64).to_radians().sin_cos();
        dot(start[0] as f64, start[2] as f64, 2);
        for t in 0..40 {
            dot(start[0] as f64 + s * 10.0 * t as f64, start[2] as f64 + c * 10.0 * t as f64, 0);
        }
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        out.extend(img);
        out
    }
}

/// Each object's triangles by grid column (x, z), for "what is above this
/// point" questions.
pub struct Objects<'a> {
    tris: &'a [ColTri],
    lo: V3,
    size: V3,
    nx: usize,
    nz: usize,
    /// (object, column) -> triangle indices whose x/z box reaches the column
    cols: HashMap<(u32, usize), Vec<usize>>,
    /// column -> the objects with triangles reaching it
    by_col: HashMap<usize, Vec<u32>>,
}

impl<'a> Objects<'a> {
    pub fn new(tris: &'a [ColTri], lo: V3, size: V3, nx: usize, nz: usize) -> Objects<'a> {
        let mut cols: HashMap<(u32, usize), Vec<usize>> = HashMap::new();
        for (i, t) in tris.iter().enumerate() {
            let col = |k: usize, f: fn(f64, f64) -> f64, n: usize| {
                let v = t.p.iter().map(|p| p[k]).fold(if f(0.0, 1.0) == 0.0 { f64::MAX } else { f64::MIN }, f);
                (((v - lo[k]) / size[k]).floor().max(0.0) as usize).min(n - 1)
            };
            let (x0, x1) = (col(0, f64::min, nx), col(0, f64::max, nx));
            let (z0, z1) = (col(2, f64::min, nz), col(2, f64::max, nz));
            for x in x0..=x1 {
                for z in z0..=z1 {
                    cols.entry((t.object, x + nx * z)).or_default().push(i);
                }
            }
        }
        let mut by_col: HashMap<usize, Vec<u32>> = HashMap::new();
        for (o, c) in cols.keys() {
            by_col.entry(*c).or_default().push(*o);
        }
        Objects { tris, lo, size, nx, nz, cols, by_col }
    }

    /// Whether `p` is inside any object ([`Objects::inside`]).
    pub fn inside_any(&self, p: V3) -> bool {
        let cx = (((p[0] - self.lo[0]) / self.size[0]).floor().max(0.0) as usize).min(self.nx - 1);
        let cz = (((p[2] - self.lo[2]) / self.size[2]).floor().max(0.0) as usize).min(self.nz - 1);
        self.by_col.get(&(cx + self.nx * cz)).is_some_and(|os| os.iter().any(|&o| self.inside(o, p)))
    }

    /// Whether `p` is inside object `q`: the first of its triangles straight
    /// above `p` faces up (we are under its surface). Exact for closed meshes;
    /// for open ones (terrain, a prop with no bottom) everything under the
    /// surface counts as inside, so a single-sided deck has no room below it.
    pub fn inside(&self, q: u32, p: V3) -> bool {
        let cx = (((p[0] - self.lo[0]) / self.size[0]).floor().max(0.0) as usize).min(self.nx - 1);
        let cz = (((p[2] - self.lo[2]) / self.size[2]).floor().max(0.0) as usize).min(self.nz - 1);
        let Some(list) = self.cols.get(&(q, cx + self.nx * cz)) else { return false };
        let mut best: Option<(f64, bool)> = None;
        for &i in list {
            let [a, b, c] = self.tris[i].p;
            let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
            if d.abs() < 1e-12 {
                continue;
            }
            let l1 = ((b[2] - c[2]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[2] - c[2])) / d;
            let l2 = ((c[2] - a[2]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[2] - c[2])) / d;
            let l3 = 1.0 - l1 - l2;
            if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 {
                continue;
            }
            let y = l1 * a[1] + l2 * b[1] + l3 * c[1];
            if y > p[1] && best.is_none_or(|(by, _)| y < by) {
                let n = cross(sub(b, a), sub(c, a));
                best = Some((y, n[1] > 0.0));
            }
        }
        best.is_some_and(|(_, up)| up)
    }
}

/// A cell's tree from the polygons of several solids (objects, in order of
/// first appearance): each object's polygons cut along every other object's
/// planes in the cell (its tree), the pieces inside another object dropped
/// ([`Objects::inside`], tested a hair off the piece on its open side, so a
/// face where two solids touch goes, and a face two share stays), and one
/// tree of what is left: the union's boundary. Without this a closed mesh
/// pushed into the ground (a tower) leaves the ground beside it empty: below
/// the ground, in front of the tower's buried walls, reads as open air; and a
/// tree's solid only holds inside its own surfaces, so a solid-leaf test of
/// one open mesh against another (the first try) dropped faces far from it.
pub fn union_polys(items: Vec<(u32, Poly)>, objects: &Objects, limit: usize) -> Result<Vec<Poly>> {
    let mut order: Vec<u32> = vec![];
    let mut groups: HashMap<u32, Vec<Poly>> = HashMap::new();
    for (o, p) in items {
        if !groups.contains_key(&o) {
            order.push(o);
        }
        groups.entry(o).or_default().push(p);
    }
    let mut trees = HashMap::new();
    for o in &order {
        trees.insert(*o, build_tree(groups[o].clone(), limit)?);
    }
    let mut out = vec![];
    for (oi, o) in order.iter().enumerate() {
        for p in &groups[o] {
            let mut pieces = vec![p.clone()];
            for (qi, q) in order.iter().enumerate() {
                // its own object too: a mesh that runs through itself (a ledge's drop face going
                // on down inside the cliff beyond it) leaves faces inside its own solid, whose
                // planes a box corner sunk into the slope leaves the solid through, pushed further in
                let _ = (qi, oi);
                if pieces.is_empty() {
                    continue;
                }
                let mut cut = vec![];
                for pc in pieces {
                    split_by(&trees[q], 0, pc, &mut cut);
                }
                pieces = cut
                    .into_iter()
                    .filter(|pc| {
                        let k = pc.verts.len() as f64;
                        let c: V3 = std::array::from_fn(|j| pc.verts.iter().map(|v| v[j]).sum::<f64>() / k);
                        let at = [c[0] + pc.plane[0] * 0.05, c[1] + pc.plane[1] * 0.05, c[2] + pc.plane[2] * 0.05];
                        !objects.inside(*q, at)
                    })
                    .collect();
            }
            out.extend(pieces.into_iter().filter(|p| p.verts.len() >= 3 && thick(&p.verts) > 1e-3));
        }
    }
    Ok(out)
}

/// The parts of the cell box's six faces that lie inside the solid ([`Objects::inside_any`]),
/// facing out of the box: with them the solid in the cell is closed, so its tree is right in
/// every order, also deep inside a mass whose surfaces are in other cells (a plateau whose top
/// is in the cell above read as air in the cell below). The faces are cut along the planes of
/// the cell's surfaces first, so each piece is wholly in or out.
pub fn box_caps(polys: &[Poly], objects: &Objects, lo: V3, hi: V3, limit: usize) -> Result<Vec<Poly>> {
    let tree = build_tree(polys.to_vec(), limit)?;
    let material = polys.first().map_or(0, |p| p.material);
    let c = |x: usize, y: usize, z: usize| -> V3 { [if x == 1 { hi[0] } else { lo[0] }, if y == 1 { hi[1] } else { lo[1] }, if z == 1 { hi[2] } else { lo[2] }] };
    // counter-clockwise seen from outside
    let faces: [([V3; 4], [f64; 3]); 6] = [
        ([c(0, 0, 0), c(0, 0, 1), c(0, 1, 1), c(0, 1, 0)], [-1.0, 0.0, 0.0]),
        ([c(1, 0, 0), c(1, 1, 0), c(1, 1, 1), c(1, 0, 1)], [1.0, 0.0, 0.0]),
        ([c(0, 0, 0), c(1, 0, 0), c(1, 0, 1), c(0, 0, 1)], [0.0, -1.0, 0.0]),
        ([c(0, 1, 0), c(0, 1, 1), c(1, 1, 1), c(1, 1, 0)], [0.0, 1.0, 0.0]),
        ([c(0, 0, 0), c(0, 1, 0), c(1, 1, 0), c(1, 0, 0)], [0.0, 0.0, -1.0]),
        ([c(0, 0, 1), c(1, 0, 1), c(1, 1, 1), c(0, 1, 1)], [0.0, 0.0, 1.0]),
    ];
    let mut out = vec![];
    for (q, n) in faces {
        // a hair outside the wall, so a point on the wall itself reads solid too
        let d = -(n[0] * q[0][0] + n[1] * q[0][1] + n[2] * q[0][2]) - 1e-3;
        let face = Poly { plane: [n[0], n[1], n[2], d], verts: q.to_vec(), material };
        let mut pieces = vec![];
        split_by(&tree, 0, face, &mut pieces);
        for p in pieces {
            if p.verts.len() < 3 || thick(&p.verts) <= 1e-3 {
                continue;
            }
            let k = p.verts.len() as f64;
            let m: V3 = std::array::from_fn(|j| p.verts.iter().map(|v| v[j]).sum::<f64>() / k - n[j] * 0.01);
            if objects.inside_any(m) {
                out.push(p);
            }
        }
    }
    Ok(out)
}

/// A solid-leaf tree of an open surface depends on the polygons' order (the
/// first is the splitter): a cliff's face picked early can cut through the
/// plateau above it and leave it empty. The tree of a few orders (the
/// source's, walls first, floors first, the largest first, the smallest first, a few fixed
/// shuffles) that agrees best with [`Objects::inside`] on a 6 x 6 x 6 raster of the cell and
/// half a unit either side of each polygon (the first that agrees everywhere wins).
fn best_tree(polys: Vec<Poly>, objects: &Objects, lo: V3, hi: V3, limit: usize) -> Result<Vec<Node>> {
    const N: usize = 6;
    let pts: Vec<V3> = (0..N * N * N)
        .map(|i| {
            let f = |k: usize, j: usize| lo[k] + (hi[k] - lo[k]) * (j as f64 + 0.5) / N as f64;
            [f(0, i % N), f(1, i / N % N), f(2, i / (N * N))]
        })
        .collect();
    // and points half a unit either side of each polygon (where a car meets it), inside the cell
    let mut pts = pts;
    for p in &polys {
        let k = p.verts.len() as f64;
        let c: V3 = std::array::from_fn(|j| p.verts.iter().map(|v| v[j]).sum::<f64>() / k);
        for s in [0.5, -0.5] {
            let q = [c[0] + p.plane[0] * s, c[1] + p.plane[1] * s, c[2] + p.plane[2] * s];
            if (0..3).all(|j| q[j] > lo[j] && q[j] < hi[j]) {
                pts.push(q);
            }
        }
    }
    let truth: Vec<bool> = pts.iter().map(|p| objects.inside_any(*p)).collect();
    let score = |t: &[Node]| pts.iter().zip(&truth).filter(|(p, &s)| !t.is_empty() && inside(t, **p) == s || t.is_empty() && !s).count();
    let mut best = build_tree(polys.clone(), limit)?;
    let mut best_score = score(&best);
    let keys: [fn(&Poly) -> f64; 5] =
        [|p| p.plane[1].abs(), |p| -p.plane[1].abs(), |p| -area(&p.verts), |p| area(&p.verts), |p| -p.plane[1]];
    for (i, key) in keys.iter().map(Some).chain([None, None, None]).enumerate() {
        if best_score == pts.len() {
            break;
        }
        let mut v = polys.clone();
        match key {
            Some(key) => v.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal)),
            // a few fixed shuffles
            None => {
                let mut seed = 0x9E3779B97F4A7C15u64 ^ i as u64;
                for j in (1..v.len()).rev() {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    v.swap(j, (seed % (j as u64 + 1)) as usize);
                }
            }
        }
        let t = build_tree(v, limit)?;
        let s = score(&t);
        if s > best_score {
            best = t;
            best_score = s;
        }
    }
    Ok(best)
}

/// `p` cut along the planes of `tree` (from node `k`) down to its leaves.
fn split_by(tree: &[Node], k: usize, p: Poly, out: &mut Vec<Poly>) {
    if tree.is_empty() {
        out.push(p);
        return;
    }
    let n = &tree[k];
    let go = |child: Option<usize>, p: Poly, out: &mut Vec<Poly>| match child {
        Some(c) => split_by(tree, c, p, out),
        None => out.push(p),
    };
    match split_poly(&p.verts, n.plane, EPS) {
        Side::On | Side::Front => go(n.front, p, out),
        Side::Back => go(n.back, p, out),
        Side::Span(f, b) => {
            if let Some(f) = f {
                go(n.front, Poly { plane: p.plane, verts: f, material: p.material }, out);
            }
            if let Some(b) = b {
                go(n.back, Poly { plane: p.plane, verts: b, material: p.material }, out);
            }
        }
    }
}

/// How well the built collision matches the solid triangles from above: on an
/// n x n raster over the grid, the highest solid triangle facing up under
/// each point against the first solid going down in the grid. (points with a
/// surface, points within `tol` of it, the worst misses as (x, z, wanted, got)).
pub fn surface_check(tris: &[ColTri], g: &Grid, n: usize, tol: f64) -> (usize, usize, Vec<(f64, f64, f64, Option<f64>)>) {
    let objects = Objects::new(tris, g.lo, g.size, g.n[0], g.n[2]);
    let (mut have, mut ok, mut bad) = (0, 0, vec![]);
    for j in 0..n {
        for i in 0..n {
            let x = g.lo[0] + (i as f64 + 0.5) * (g.hi[0] - g.lo[0]) / n as f64;
            let z = g.lo[2] + (j as f64 + 0.5) * (g.hi[2] - g.lo[2]) / n as f64;
            // the highest triangle under the top of the grid at (x, z)
            let mut top: Option<(f64, bool)> = None;
            let cx = (((x - g.lo[0]) / g.size[0]).floor().max(0.0) as usize).min(g.n[0] - 1);
            let cz = (((z - g.lo[2]) / g.size[2]).floor().max(0.0) as usize).min(g.n[2] - 1);
            for ((_, col), list) in &objects.cols {
                if *col != cx + g.n[0] * cz {
                    continue;
                }
                for &t in list {
                    let [a, b, c] = tris[t].p;
                    let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
                    if d.abs() < 1e-12 {
                        continue;
                    }
                    let l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / d;
                    let l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / d;
                    let l3 = 1.0 - l1 - l2;
                    if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 {
                        continue;
                    }
                    let y = l1 * a[1] + l2 * b[1] + l3 * c[1];
                    if top.is_none_or(|(ty, _)| y > ty) {
                        top = Some((y, cross(sub(b, a), sub(c, a))[1] > 0.0));
                    }
                }
            }
            let Some((want, true)) = top else { continue };
            have += 1;
            let got = g.ground_below([x, want + 20.0, z], 1.0e5, 0.5);
            if got.is_some_and(|y| (y - want).abs() <= tol) {
                ok += 1;
            } else {
                bad.push((x, z, want, got));
            }
        }
    }
    bad.sort_by(|a, b| {
        let e = |r: &(f64, f64, f64, Option<f64>)| r.3.map_or(f64::MAX, |y| (y - r.2).abs());
        e(b).partial_cmp(&e(a)).unwrap()
    });
    bad.truncate(8);
    (have, ok, bad)
}

/// Per source object (mesh node): faces sampled (a few points on each triangle), the samples a
/// unit in front of a face that the collision calls solid though nothing is there (`phantom`:
/// the car snags or bounces off air) and a unit behind a face that it calls open though that is
/// inside the union (`hole`: the car goes through).
#[derive(Clone, Debug, Default)]
pub struct FaceStats {
    pub samples: usize,
    pub phantom: usize,
    pub hole: usize,
    /// Some failing points (point, phantom?).
    pub worst: Vec<(V3, bool)>,
}

/// [`FaceStats`] of every source object, by its number (`ColTri.object`).
pub fn face_check(tris: &[ColTri], g: &Grid) -> std::collections::BTreeMap<u32, FaceStats> {
    let split = prepare(tris);
    let objects = Objects::new(&split, g.lo, g.size, g.n[0], g.n[2]);
    let mut by_col: HashMap<usize, Vec<u32>> = HashMap::new();
    for (o, c) in objects.cols.keys() {
        by_col.entry(*c).or_default().push(*o);
    }
    let inside_any = |p: V3| {
        let cx = (((p[0] - g.lo[0]) / g.size[0]).floor().max(0.0) as usize).min(g.n[0] - 1);
        let cz = (((p[2] - g.lo[2]) / g.size[2]).floor().max(0.0) as usize).min(g.n[2] - 1);
        by_col.get(&(cx + g.n[0] * cz)).is_some_and(|os| os.iter().any(|&o| objects.inside(o, p)))
    };
    let mut out: std::collections::BTreeMap<u32, FaceStats> = Default::default();
    for t in tris {
        let Some(pl) = plane_of(&t.p) else { continue };
        let n = [pl[0], pl[1], pl[2]];
        let s = out.entry(t.object).or_default();
        for w in [[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0], [0.6, 0.2, 0.2], [0.2, 0.6, 0.2], [0.2, 0.2, 0.6]] {
            let c: V3 = std::array::from_fn(|k| w[0] * t.p[0][k] + w[1] * t.p[1][k] + w[2] * t.p[2][k]);
            let front = [c[0] + n[0], c[1] + n[1], c[2] + n[2]];
            let back = [c[0] - n[0], c[1] - n[1], c[2] - n[2]];
            let in_grid = |p: V3| (0..3).all(|k| p[k] >= g.lo[k] && p[k] < g.hi[k]);
            if !in_grid(front) || !in_grid(back) {
                continue;
            }
            s.samples += 1;
            if g.solid(front) && !inside_any(front) {
                s.phantom += 1;
                if s.worst.len() < 6 {
                    s.worst.push((front, true));
                }
            }
            if !g.solid(back) && inside_any(back) {
                s.hole += 1;
                if s.worst.len() < 6 {
                    s.worst.push((back, false));
                }
            }
        }
    }
    out
}

/// Inside or not, on an n x n x n raster of every cell: the samples, and those where the
/// collision disagrees with "under a surface" ([`Objects::inside`] of any object): solid where
/// nothing is, open inside a solid.
pub fn volume_check(tris: &[ColTri], g: &Grid, n: usize) -> (usize, usize, usize) {
    let split = prepare(tris);
    let objects = Objects::new(&split, g.lo, g.size, g.n[0], g.n[2]);
    let mut by_col: HashMap<usize, Vec<u32>> = HashMap::new();
    for (o, c) in objects.cols.keys() {
        by_col.entry(*c).or_default().push(*o);
    }
    let (mut all, mut phantom, mut hole) = (0, 0, 0);
    for c in &g.cells {
        let cx = (((c.lo[0] + c.hi[0]) / 2.0 - g.lo[0]) / g.size[0]).floor() as usize;
        let cz = (((c.lo[2] + c.hi[2]) / 2.0 - g.lo[2]) / g.size[2]).floor() as usize;
        let os = by_col.get(&(cx + g.n[0] * cz)).cloned().unwrap_or_default();
        for i in 0..n * n * n {
            let f = |k: usize, j: usize| c.lo[k] + (c.hi[k] - c.lo[k]) * (j as f64 + 0.37) / n as f64;
            let p = [f(0, i % n), f(1, i / n % n), f(2, i / (n * n))];
            let want = os.iter().any(|&o| objects.inside(o, p));
            let got = !c.tree.is_empty() && inside(&c.tree, p);
            all += 1;
            if got && !want {
                phantom += 1;
            } else if want && !got {
                hole += 1;
            }
        }
    }
    (all, phantom, hole)
}

/// Walls hanging from the open edges of the floors (an up-facing triangle's edge no other
/// triangle of its piece shares, with open air past it) down to `bottom`, facing away from the floor. Under a surface
/// counts as solid ([`Objects::inside`]), so a one-sided deck, a floor whose sides were never
/// modelled, or the rim of an open terrain stands on solid down to the bottom: but only under
/// its own footprint. Without them nothing in a cell marks where the footprint ends, and the
/// deck's plane, the floor under the deck's edge or the air past the world's rim come out on
/// the wrong side (a deck's solid ran on under the open sky beside it). Two pieces meeting at
/// a seam that was never welded hang two skirts back to back on one plane: they face each
/// other and drop out (two solids touching). Collision only: not drawn, no crease edges.
pub fn skirts(tris: &[ColTri], bottom: f64, objects: &Objects) -> Vec<ColTri> {
    let ids = weld(tris, 1e-3);
    let mut count: HashMap<(u32, usize, usize), u32> = HashMap::new();
    for (t, v) in tris.iter().zip(&ids) {
        for k in 0..3 {
            let (a, b) = (v[k], v[(k + 1) % 3]);
            *count.entry((t.object, a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut out = vec![];
    for (t, v) in tris.iter().zip(&ids) {
        let Some(pl) = plane_of(&t.p) else { continue };
        if pl[1] < 0.1 {
            continue;
        }
        for k in 0..3 {
            let (ia, ib) = (v[k], v[(k + 1) % 3]);
            if count[&(t.object, ia.min(ib), ia.max(ib))] != 1 {
                continue;
            }
            let (a, b, c) = (t.p[k], t.p[(k + 1) % 3], t.p[(k + 2) % 3]);
            if (a[1].min(b[1])) <= bottom + 1e-3 || ((b[0] - a[0]).powi(2) + (b[2] - a[2]).powi(2)).sqrt() < 1e-3 {
                continue;
            }
            let (ad, bd) = ([a[0], bottom, a[2]], [b[0], bottom, b[2]]);
            // out: away from the triangle's third corner, across the edge
            let e = sub(b, a);
            let mut out_n = [-e[2], 0.0, e[0]];
            if dot(out_n, sub(c, a)) > 0.0 {
                out_n = [e[2], 0.0, -e[0]];
            }
            // only where it is open past the edge: an edge left unshared by a T-junction (a
            // cliff's faces meeting the ground mid-edge) has solid past it, and a skirt there
            // would be a wall buried in the mass, which a box corner sunk into the slope
            // leaves through, pushed further in
            let ln = len(out_n).max(1e-12);
            let m = [(a[0] + b[0]) / 2.0 + out_n[0] / ln * 0.5, (a[1] + b[1]) / 2.0 - 0.5, (a[2] + b[2]) / 2.0 + out_n[2] / ln * 0.5];
            if objects.inside_any(m) {
                continue;
            }
            let mut q = [a, b, bd, ad];
            if let Some(p) = plane_of(&q) {
                if dot([p[0], p[1], p[2]], out_n) < 0.0 {
                    q = [b, a, ad, bd];
                }
            }
            out.push(ColTri { p: [q[0], q[1], q[2]], material: t.material, object: t.object });
            out.push(ColTri { p: [q[0], q[2], q[3]], material: t.material, object: t.object });
        }
    }
    out
}

/// How thick a floating deck becomes ([`solidify_decks`]), and the gap under one that makes it
/// floating (about a car's height: something can pass under it).
pub const DECK_THICKNESS: f64 = 4.0;
pub const DECK_CLEARANCE: f64 = 16.0;

/// The pieces the collision is built from: each mesh node split into its connected pieces
/// ([`split_components`]), floating one-sided decks made slabs ([`solidify_decks`]).
pub fn prepare(tris: &[ColTri]) -> Vec<ColTri> {
    solidify_decks(&split_components(tris))
}

/// A one-sided floor that floats (a bridge, a walkway, a grate: an open piece of up-facing
/// triangles only, with another surface more than [`DECK_CLEARANCE`] below most of it) is a
/// slab [`DECK_THICKNESS`] thick: a bottom under it facing down and walls round its open edges.
/// Otherwise "under a surface is solid" fills the room beneath it down to the ground. A floor
/// with the ground right under it (a plate, a decal) or nothing at all below stays as it is.
pub fn solidify_decks(tris: &[ColTri]) -> Vec<ColTri> {
    let mut pieces: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        pieces.entry(t.object).or_default().push(i);
    }
    let ids = weld(tris, 1e-3);
    let up = |t: &ColTri| plane_of(&t.p).is_some_and(|p| p[1] > 0.3);
    // the highest up-facing surface of another piece below a point, by brute force over a
    // coarse x/z bucketing of the up-facing triangles
    let cell = 64.0;
    let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        if !up(t) {
            continue;
        }
        let (mut x0, mut x1, mut z0, mut z1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in &t.p {
            x0 = x0.min(p[0]);
            x1 = x1.max(p[0]);
            z0 = z0.min(p[2]);
            z1 = z1.max(p[2]);
        }
        for bx in (x0 / cell).floor() as i64..=(x1 / cell).floor() as i64 {
            for bz in (z0 / cell).floor() as i64..=(z1 / cell).floor() as i64 {
                buckets.entry((bx, bz)).or_default().push(i);
            }
        }
    }
    let below = |p: V3, own: u32| -> Option<f64> {
        let list = buckets.get(&((p[0] / cell).floor() as i64, (p[2] / cell).floor() as i64))?;
        let mut best: Option<f64> = None;
        for &i in list {
            let t = &tris[i];
            if t.object == own {
                continue;
            }
            let [a, b, c] = t.p;
            let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
            if d.abs() < 1e-12 {
                continue;
            }
            let l1 = ((b[2] - c[2]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[2] - c[2])) / d;
            let l2 = ((c[2] - a[2]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[2] - c[2])) / d;
            let l3 = 1.0 - l1 - l2;
            if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 {
                continue;
            }
            let y = l1 * a[1] + l2 * b[1] + l3 * c[1];
            if y < p[1] - 1e-3 && best.is_none_or(|b| y > b) {
                best = Some(y);
            }
        }
        best
    };
    let mut out = tris.to_vec();
    let mut keys: Vec<u32> = pieces.keys().copied().collect();
    keys.sort_unstable();
    for obj in keys {
        let list = &pieces[&obj];
        if !list.iter().all(|&i| up(&tris[i])) {
            continue;
        }
        let mut count: HashMap<(usize, usize), u32> = HashMap::new();
        for &i in list {
            for k in 0..3 {
                let (a, b) = (ids[i][k], ids[i][(k + 1) % 3]);
                *count.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        if count.values().all(|&n| n != 1) {
            continue;
        }
        let floating = list
            .iter()
            .filter(|&&i| {
                let t = &tris[i];
                let c: V3 = std::array::from_fn(|k| (t.p[0][k] + t.p[1][k] + t.p[2][k]) / 3.0);
                below(c, obj).is_some_and(|y| c[1] - y > DECK_CLEARANCE)
            })
            .count();
        if floating * 2 <= list.len() {
            continue;
        }
        let down = |p: V3| [p[0], p[1] - DECK_THICKNESS, p[2]];
        for &i in list {
            let t = &tris[i];
            out.push(ColTri { p: [down(t.p[0]), down(t.p[2]), down(t.p[1])], ..t.clone() });
            for k in 0..3 {
                let (ia, ib) = (ids[i][k], ids[i][(k + 1) % 3]);
                if count[&(ia.min(ib), ia.max(ib))] != 1 {
                    continue;
                }
                // the wall under edge a -> b faces out the way the triangle's winding does
                let (a, b) = (t.p[k], t.p[(k + 1) % 3]);
                out.push(ColTri { p: [b, a, down(a)], ..t.clone() });
                out.push(ColTri { p: [b, down(a), down(b)], ..t.clone() });
            }
        }
    }
    out
}

/// The game's ray against a cell's tree (`CollisionTerrain.c` RayHitsBSP2, in f32, the same
/// branches): the hit's fraction of the segment, if any. Used to check that a tree gives no
/// hits the point classification does not ([`ray_check`]).
pub fn game_ray(tree: &[Node], s: [f32; 3], e: [f32; 3]) -> Option<f32> {
    game_ray_hit(tree, s, e).map(|h| h.0)
}

/// [`game_ray`] with the node whose plane the game reports for the hit (the one crossed into the
/// solid; for a ray starting in the solid, as the port does for a custom world, the nearest
/// plane it is behind on its way down: `CollisionTerrain.c`).
pub fn game_ray_hit(tree: &[Node], s: [f32; 3], e: [f32; 3]) -> Option<(f32, usize)> {
    fn d(n: &Node, p: [f32; 3]) -> f32 {
        n.plane[0] as f32 * p[0] + n.plane[1] as f32 * p[1] + n.plane[2] as f32 * p[2] + n.plane[3] as f32
    }
    fn nearest(t: &[Node], p: [f32; 3], fallback: usize) -> usize {
        let (mut k, mut best, mut bd) = (0usize, fallback, -d(&t[fallback], p));
        for _ in 0..0x10000 {
            let dd = d(&t[k], p);
            if dd >= 0.0 {
                match t[k].front {
                    Some(f) => k = f,
                    None => break,
                }
            } else {
                if -dd < bd {
                    bd = -dd;
                    best = k;
                }
                match t[k].back {
                    Some(b) => k = b,
                    None => break,
                }
            }
        }
        best
    }
    fn rec(t: &[Node], s: [f32; 3], e: [f32; 3], k: usize, depth: usize) -> Option<(f32, usize)> {
        if depth > 4096 {
            return None;
        }
        let n = &t[k];
        let (d0, d1) = (d(n, s), d(n, e));
        let lerp = |f: f32| -> [f32; 3] { [(e[0] - s[0]) * f + s[0], (e[1] - s[1]) * f + s[1], (e[2] - s[2]) * f + s[2]] };
        if d0 == 0.0 && d1 == 0.0 {
            return n.front.and_then(|f| rec(t, s, e, f, depth + 1));
        }
        if (d0 > 0.0 && d1 < 0.0) || (d0 < 0.0 && d1 > 0.0) {
            let fr = if d0 != d1 { d0 / (d0 - d1) } else { 0.0 };
            let mid = lerp(fr);
            let (near, far) = if d0 > 0.0 { (n.front, n.back) } else { (n.back, n.front) };
            match near {
                Some(c) => {
                    if let Some((h, m)) = rec(t, s, mid, c, depth + 1) {
                        return Some((h * fr, m));
                    }
                }
                None if d0 < 0.0 => return Some((fr, k)),
                None => {}
            }
            return match far {
                Some(c) => rec(t, mid, e, c, depth + 1).map(|(h, m)| {
                    let tt = h * (1.0 - fr) + fr;
                    if tt == fr {
                        (fr, k)
                    } else {
                        (tt, m)
                    }
                }),
                None if d0 > 0.0 => Some((fr, k)),
                None => None,
            };
        }
        if d0 > 0.0 || d1 > 0.0 {
            return n.front.and_then(|f| rec(t, s, e, f, depth + 1));
        }
        match n.back {
            Some(b) => rec(t, s, e, b, depth + 1),
            None => Some((0.0, nearest(t, s, k))),
        }
    }
    if tree.is_empty() {
        None
    } else {
        rec(tree, s, e, 0, 0)
    }
}

/// Short segments in the open just above the surfaces (where a car's box corners move: 0.5 to 8
/// units over the ground, 1 to 4 long, any way), each wholly open by the point classification,
/// fired with the game's ray at every cell they reach ([`game_ray`]; hits outside a cell's box
/// dropped, as `RayHitsBSP` drops them): the segments, and those the game would see hit
/// something (a phantom face: the car is struck by air).
pub fn ray_check(g: &Grid, samples: usize) -> (usize, usize) {
    let mut seed: u64 = 0x2545F4914F6CDD1D;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let (mut n, mut hits) = (0, 0);
    let mut tries = 0;
    while n < samples && tries < samples * 20 {
        tries += 1;
        let (x, z) = (g.lo[0] + rnd() * (g.hi[0] - g.lo[0]), g.lo[2] + rnd() * (g.hi[2] - g.lo[2]));
        let Some(ground) = g.ground_below([x, g.hi[1] - 0.5, z], g.hi[1] - g.lo[1], 0.5) else { continue };
        let s = [x, ground + 0.5 + rnd() * 7.5, z];
        let (a, b) = (rnd() * std::f64::consts::TAU, (rnd() - 0.5) * 0.6);
        let l = 1.0 + rnd() * 3.0;
        let e = [s[0] + l * a.cos() * b.cos(), s[1] + l * b.sin(), s[2] + l * a.sin() * b.cos()];
        if (0..=40).any(|i| {
            let t = i as f64 / 40.0;
            g.solid([s[0] + (e[0] - s[0]) * t, s[1] + (e[1] - s[1]) * t, s[2] + (e[2] - s[2]) * t])
        }) {
            continue;
        }
        n += 1;
        let (sf, ef) = (s.map(|v| v as f32), e.map(|v| v as f32));
        let hit = g.cells.iter().any(|c| {
            let (lo, hi) = (c.lo.map(|v| v as f32), c.hi.map(|v| v as f32));
            if (0..3).any(|k| sf[k].max(ef[k]) < lo[k] || sf[k].min(ef[k]) > hi[k]) {
                return false;
            }
            game_ray(&c.tree, sf, ef).is_some_and(|t| {
                let h: [f32; 3] = std::array::from_fn(|k| sf[k] + (ef[k] - sf[k]) * t);
                (0..3).all(|k| h[k] >= lo[k] - 0.1 && h[k] <= hi[k] + 0.1)
            })
        });
        hits += hit as usize;
    }
    (n, hits)
}

/// Nodes in all, and nodes whose plane is (within [`COPLANAR_COS`] and 0.01 at the node's own
/// polygon's distance... approximated by the planes' distances) one of their ancestors' planes
/// the same way round: each leaves a float-thin solid leaf behind it that the game's ray can hit
/// anywhere along the plane in the cell ([`build_tree`]).
pub fn sliver_nodes(g: &Grid) -> (usize, usize) {
    let (mut all, mut sliver) = (0, 0);
    for c in &g.cells {
        let t = &c.tree;
        all += t.len();
        // ancestors by a walk from the root
        let mut stack: Vec<(usize, Vec<usize>)> = if t.is_empty() { vec![] } else { vec![(0, vec![])] };
        while let Some((k, anc)) = stack.pop() {
            let p = t[k].plane;
            if anc.iter().any(|&a| {
                let q = t[a].plane;
                p[0] * q[0] + p[1] * q[1] + p[2] * q[2] > COPLANAR_COS && (p[3] - q[3]).abs() < COPLANAR_DIST
            }) {
                sliver += 1;
            }
            let mut next = anc.clone();
            next.push(k);
            for ch in [t[k].front, t[k].back].into_iter().flatten() {
                stack.push((ch, next.clone()));
            }
        }
    }
    (all, sliver)
}

/// How near (game units) a square's wall may come to a face lying along it.
pub const WALL_CLEARANCE: f64 = 2.0;
/// How far past its square a cell's tree reaches (surfaces clipped to the square grown by this,
/// its caps on the grown box): past what a car's box corner moves in a frame (400 units a
/// second). The game's ray reports where a piece of a ray that entered the solid leaves it again
/// (`RayHitsBSP2`); closed at the square's own walls, that was the far side of the hair
/// between a surface and a wall below it, facing away: a car on a plain a hair above a square's
/// floor was pushed down into it. Reaching past, the solid holds to the ray's end, the entry is
/// reported, and a hit out there is dropped (`RayHitsBSP` keeps the square's own box).
pub const CELL_OVERLAP: f64 = 16.0;

/// The grid shifted, per axis, by up to a square so that as little face area as can be lies
/// within [`WALL_CLEARANCE`] of a wall across that axis (faces facing along the axis within
/// 11 degrees: floors for y, walls for x and z), one square added to keep everything covered.
pub fn shift_off_faces(tris: &[ColTri], lo: V3, n: [usize; 3], size: V3) -> (V3, [usize; 3], V3) {
    let (mut lo, mut n) = (lo, n);
    for k in 0..3 {
        // (coordinate along k, area) of the faces facing along k
        let faces: Vec<(f64, f64)> = tris
            .iter()
            .filter_map(|t| {
                let pl = plane_of(&t.p)?;
                (pl[k].abs() > 0.98).then(|| ((t.p[0][k] + t.p[1][k] + t.p[2][k]) / 3.0, area(&t.p)))
            })
            .collect();
        if faces.is_empty() {
            continue;
        }
        let cost = |o: f64| -> f64 {
            faces
                .iter()
                .filter(|(c, _)| {
                    let r = (c - (lo[k] - o)).rem_euclid(size[k]);
                    r.min(size[k] - r) < WALL_CLEARANCE
                })
                .map(|(_, a)| a)
                .sum()
        };
        let (mut best, mut best_cost) = (0.0, cost(0.0));
        for i in 1..64 {
            let o = size[k] * i as f64 / 64.0;
            let c = cost(o);
            if c < best_cost - 1e-9 {
                best = o;
                best_cost = c;
            }
        }
        if best > 0.0 {
            lo[k] -= best;
            n[k] += 1;
        }
    }
    let hi = std::array::from_fn(|k| lo[k] + size[k] * n[k] as f64);
    (lo, n, hi)
}

/// Short rays grazing down through the surfaces (from a hair to half a unit over the ground to as
/// far under it, 0.5 to 4 units along it, like a moving car's box corners), fired with the game's ray as the
/// retail game walks it ([`game_ray_hit`]): the rays, and those whose reported face looks away
/// from the ray (the far side of a thin slab: the game pushes the corner the wrong way, into
/// the ground). A slab is a face a hair above a square's wall, the solid below cut off by it.
pub fn surface_ray_check(g: &Grid, samples: usize) -> (usize, usize) {
    surface_ray_check_in(g, samples, g.lo, g.hi)
}

/// [`surface_ray_check`] with the rays' starts in a box (x, z of `lo`..`hi`).
pub fn surface_ray_check_in(g: &Grid, samples: usize, blo: V3, bhi: V3) -> (usize, usize) {
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let (mut n, mut wrong) = (0, 0);
    let mut tries = 0;
    while n < samples && tries < samples * 20 {
        tries += 1;
        let (x, z) = (blo[0] + rnd() * (bhi[0] - blo[0]), blo[2] + rnd() * (bhi[2] - blo[2]));
        let Some(ground) = g.ground_below([x, g.hi[1] - 0.5, z], g.hi[1] - g.lo[1], 0.5) else { continue };
        let a = rnd() * std::f64::consts::TAU;
        let r = 0.5 + rnd() * 3.5;
        let s = [x, ground + 0.02 + rnd() * 0.5, z];
        let e = [x + r * a.cos(), ground - 0.02 - rnd() * 0.5, z + r * a.sin()];
        if g.solid(s) || !g.solid(e) {
            continue;
        }
        n += 1;
        let (sf, ef) = (s.map(|v| v as f32), e.map(|v| v as f32));
        let dir = [ef[0] - sf[0], ef[1] - sf[1], ef[2] - sf[2]];
        let mut best: Option<(f32, [f64; 4])> = None;
        for c in &g.cells {
            let (lo, hi) = (c.lo.map(|v| v as f32), c.hi.map(|v| v as f32));
            if (0..3).any(|k| sf[k] < lo[k] || sf[k] > hi[k]) {
                // the cell the corner is in: the game tests the cells the car's box reaches,
                // which a corner just over a square's floor need not leave
                continue;
            }
            if let Some((t, k)) = game_ray_hit(&c.tree, sf, ef) {
                let h: [f32; 3] = std::array::from_fn(|j| sf[j] + dir[j] * t);
                if (0..3).all(|j| h[j] >= lo[j] - 0.1 && h[j] <= hi[j] + 0.1) && best.is_none_or(|b| t < b.0) {
                    best = Some((t, c.tree[k].plane));
                }
            }
        }
        if let Some((_, pl)) = best {
            if pl[0] * dir[0] as f64 + pl[1] * dir[1] as f64 + pl[2] * dir[2] as f64 > 0.0 {
                wrong += 1;
            }
        }
    }
    (n, wrong)
}
