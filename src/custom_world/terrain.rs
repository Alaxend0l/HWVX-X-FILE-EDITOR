//! A world's collision pack (type 5) from triangles: the terrain grid of
//! `docs/worlds/README.md` ("Collision packs", "BSP build rules"), ported from
//! HWVX-SDL's `tools/xdl_chunk.py` (`build_collision`, `write_collision`) and
//! `tools/bsp_build.py` (`build`).
//!
//! * The grid: squares of about 250 x 200 x 250 over the triangles' bounds
//!   (grown by 1, and by a square's height above), each a whole fraction of
//!   the span.
//! * Per square: the triangles clipped to its box, in the source order (the
//!   first polygon is the splitter), built into a solid-leaf tree: coplanar
//!   polygons facing the same way go in front, facing the other way are
//!   dropped (two solids touching there, a box standing on the ground: an
//!   inside face), tolerance 1e-4, pre-order. A cell's solid only matters in
//!   its own box (`RayHitsBSP` drops hits outside it).
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
}

/// Build settings.
#[derive(Clone, Debug)]
pub struct Options {
    /// Target square size across (x, z) and up (y).
    pub cell_xz: f64,
    pub cell_y: f64,
    /// Faces meeting at more than this many degrees make a crease edge.
    pub crease_deg: f64,
}

impl Default for Options {
    fn default() -> Options {
        Options { cell_xz: 250.0, cell_y: 200.0, crease_deg: 0.5 }
    }
}

const EPS: f64 = 1e-4;
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
            match split_poly(&p.verts, plane, EPS) {
                Side::On => {
                    if dot(n, [p.plane[0], p.plane[1], p.plane[2]]) > 0.0 {
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
pub fn crease_edges(tris: &[ColTri], deg: f64) -> (Vec<Edge>, usize, usize) {
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
                if dot(normals[i].unwrap(), normals[j].unwrap()) < cos {
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
    fn square(&self, p: V3) -> Option<[usize; 3]> {
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

/// The grid of a collision mesh.
pub fn build_grid(tris: &[ColTri], opt: &Options) -> Result<Grid> {
    ensure!(!tris.is_empty(), "no collision triangles");
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
    let (nx, ny, nz) = (n[0], n[1], n[2]);

    // polygons per square, in source order
    let mut per: HashMap<[usize; 3], Vec<Poly>> = HashMap::new();
    for t in tris {
        let Some(plane) = plane_of(&t.p) else { continue };
        let a: [usize; 3] = std::array::from_fn(|k| {
            let m = t.p.iter().map(|p| p[k]).fold(f64::MAX, f64::min);
            (((m - lo[k]) / size[k]).floor().max(0.0) as usize).min(n[k] - 1)
        });
        let b: [usize; 3] = std::array::from_fn(|k| {
            let m = t.p.iter().map(|p| p[k]).fold(f64::MIN, f64::max);
            (((m - lo[k]) / size[k]).floor().max(0.0) as usize).min(n[k] - 1)
        });
        for x in a[0]..=b[0] {
            for y in a[1]..=b[1] {
                for z in a[2]..=b[2] {
                    let clo = [lo[0] + size[0] * x as f64, lo[1] + size[1] * y as f64, lo[2] + size[2] * z as f64];
                    let chi = [clo[0] + size[0], clo[1] + size[1], clo[2] + size[2]];
                    let Some(piece) = clip_to_box(t.p.to_vec(), clo, chi) else { continue };
                    if piece.len() < 3 || thick(&piece) <= 1e-2 || faces_in(&piece, plane, clo, chi) {
                        continue;
                    }
                    per.entry([x, y, z]).or_default().push(Poly { plane, verts: piece, material: t.material });
                }
            }
        }
    }

    let (edges, open_edges, shared_edges) = crease_edges(tris, opt.crease_deg);
    ensure!(edges.len() <= 0xFFFF, "{} crease edges: the cells' edge indices are u16", edges.len());
    let mut keys: Vec<[usize; 3]> = per.keys().copied().collect();
    keys.sort_by_key(|k| (k[1], k[2], k[0]));
    let mut index = vec![0u32; nx * ny * nz];
    let mut cells = vec![];
    for key in keys {
        let [x, y, z] = key;
        let clo = [lo[0] + size[0] * x as f64, lo[1] + size[1] * y as f64, lo[2] + size[2] * z as f64];
        let chi = [clo[0] + size[0], clo[1] + size[1], clo[2] + size[2]];
        let tree = build_tree(per.remove(&key).unwrap(), 0xFFFF)?;
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
