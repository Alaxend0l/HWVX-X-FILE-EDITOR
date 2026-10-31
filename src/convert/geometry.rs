//! Geometry: PS2-prototype models (VIF packets of triangle strips) to
//! GameCube models (indexed GX triangle lists).
//!
//! See `docs/beta-cars/FORMATS.md` ("Geometry") for the layouts. In short:
//!
//! * Prototype: header `u16 0x1005`, LODs of 24 bytes, groups of 32 bytes
//!   whose packet holds batches (<= 79 vertices) of position (+ADC bit),
//!   UV, and normal or RGBA, drawn as GS triangle strips.
//! * GameCube: header `u16 0x2003`, LODs of 0x18 bytes with an indexed
//!   vertex buffer, groups of 0x20 bytes each with one `GX_DRAW_TRIANGLES`
//!   display list of `(position, normal|colour, uv)` u16 index triples.
//! * Car shadows (CarAInfo+0x54) are closed hulls on the prototype and
//!   shadow-volume edge lists on GameCube.

use std::collections::HashMap;

use anyhow::{bail, ensure, Context, Result};

use crate::blob::{Blob, Reloc};
use crate::format::{align, Endian};

const LE: Endian = Endian::Little;
const BE: Endian = Endian::Big;

pub const BETA_MAGIC: u16 = 0x1005;
pub const GC_MAGIC: u16 = 0x2003;
pub const NO_TEXTURE: u32 = 0x0000_FFFF;

/// Group flag bits (shared by both builds).
pub const F_ALPHA: u16 = 0x0001;
pub const F_ENV: u16 = 0x0002;
pub const F_LIT: u16 = 0x0004;
pub const F_ENV_ADD: u16 = 0x0010;
pub const F_ALPHA2: u16 = 0x0080;
pub const F_SHADOW: u16 = 0x4000;
/// GC-only LOD vertex-buffer class bits.
pub const F_VB_NORMALS: u16 = 0x0800;
pub const F_VB_COLOURS: u16 = 0x1000;
pub const F_VB_BOTH: u16 = 0x2000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Attr {
    Normal([f32; 3]),
    Colour([u8; 4]),
    None,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub attr: Attr,
    /// PS2 ADC bit (set = no triangle kicked at this vertex).
    adc: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub flags: u16,
    pub colour: [u8; 4],
    pub blend: u16,
    pub sort: u16,
    /// Texture relocation value (`index | pack << 16`), if textured.
    pub texture: Option<u32>,
    pub tris: Vec<[Vertex; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lod {
    /// Texture-name list bytes (NUL-terminated names, extra NUL at the end).
    pub names: Vec<u8>,
    pub groups: Vec<Group>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    /// 0 = static; 7 = skinned (not supported).
    pub kind: u32,
    pub lod_distances: Vec<f32>,
    pub bbox_min: [f32; 3],
    pub radius_1c: f32,
    pub bbox_max: [f32; 3],
    pub lods: Vec<Lod>,
}

fn f32_at(b: &[u8], off: usize) -> Result<f32> {
    Ok(f32::from_bits(LE.u32_at(b, off).context("read past the end of the blob")?))
}

fn u16_at(b: &[u8], off: usize) -> Result<u16> {
    let s = b.get(off..off + 2).context("read past the end of the blob")?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], off: usize) -> Result<u32> {
    LE.u32_at(b, off).context("read past the end of the blob")
}

/// Is there a prototype geometry header at `off`?
pub fn is_beta_geometry(blob: &Blob, off: u32) -> bool {
    u16_at(&blob.data, off as usize).ok() == Some(BETA_MAGIC)
        && blob.pointers.contains(&(off + 8))
        && blob.pointers.contains(&(off + 0xC))
}

/// Read the NUL-terminated name list at `off` (ends with an empty name).
fn name_list(b: &[u8], off: usize) -> Result<Vec<u8>> {
    let mut end = off;
    loop {
        let n = b.get(end..).context("name list runs off the blob")?.iter().position(|&c| c == 0).context("unterminated name")?;
        end += n + 1;
        if n == 0 {
            break;
        }
    }
    Ok(b[off..end].to_vec())
}

/// VIF commands we care about in a group packet.
struct Batch {
    verts: Vec<Vertex>,
}

fn read_packet(b: &[u8], pkt: usize) -> Result<Vec<Batch>> {
    let qwc = (u32_at(b, pkt + 0x10)? & 0xFFFF) as usize;
    let mut o = pkt + 0x20;
    let end = o + qwc * 16;
    ensure!(end <= b.len(), "VIF packet at {pkt:#x} runs past the end of the blob");
    let mut batches: Vec<Batch> = Vec::new();
    let mut pos: Vec<([f32; 3], bool)> = vec![];
    let mut uvs: Vec<[f32; 2]> = vec![];
    let mut attrs: Vec<Attr> = vec![];
    let mut in_batch = false;

    let flush = |pos: &mut Vec<([f32; 3], bool)>, uvs: &mut Vec<[f32; 2]>, attrs: &mut Vec<Attr>, batches: &mut Vec<Batch>| -> Result<()> {
        if pos.is_empty() {
            return Ok(());
        }
        ensure!(uvs.len() == pos.len(), "batch has {} positions but {} UVs", pos.len(), uvs.len());
        let verts = pos
            .iter()
            .enumerate()
            .map(|(i, (p, adc))| Vertex { pos: *p, uv: uvs[i], attr: attrs.get(i).copied().unwrap_or(Attr::None), adc: *adc })
            .collect();
        batches.push(Batch { verts });
        pos.clear();
        uvs.clear();
        attrs.clear();
        Ok(())
    };

    while o + 4 <= end {
        let w = u32_at(b, o)?;
        o += 4;
        let cmd = (w >> 24) & 0x7F;
        let num = ((w >> 16) & 0xFF) as usize;
        let imm = (w & 0xFFFF) as usize;
        if cmd >= 0x60 {
            let fmt = cmd & 0xF;
            let n = if num == 0 { 256 } else { num };
            let (bits, comps) = match fmt {
                0x0 => (32, 1),
                0x1 => (16, 1),
                0x2 => (8, 1),
                0x4 => (32, 2),
                0x5 => (16, 2),
                0x6 => (8, 2),
                0x8 => (32, 3),
                0x9 => (16, 3),
                0xA => (8, 3),
                0xC => (32, 4),
                0xD => (16, 4),
                0xE => (8, 4),
                0xF => (16, 1),
                _ => bail!("unknown VIF UNPACK format {fmt:#x}"),
            };
            let size = (bits * comps * n).div_ceil(32) * 4;
            let addr = imm & 0x3FF;
            let d = o;
            ensure!(d + size <= end, "UNPACK runs past the packet end");
            match (addr, fmt) {
                (0, 0xC) if n == 1 => {
                    // GIF tag: a new batch begins.
                    if in_batch {
                        flush(&mut pos, &mut uvs, &mut attrs, &mut batches)?;
                    }
                    in_batch = true;
                }
                (1, 0xC) => {
                    for i in 0..n {
                        let q = d + 16 * i;
                        let p = [f32_at(b, q)?, f32_at(b, q + 4)?, f32_at(b, q + 8)?];
                        pos.push((p, u32_at(b, q + 12)? & 0x8000 != 0));
                    }
                }
                (2, 0x4) => {
                    for i in 0..n {
                        uvs.push([f32_at(b, d + 8 * i)?, f32_at(b, d + 8 * i + 4)?]);
                    }
                }
                (3, 0x8) => {
                    for i in 0..n {
                        let q = d + 12 * i;
                        attrs.push(Attr::Normal([f32_at(b, q)?, f32_at(b, q + 4)?, f32_at(b, q + 8)?]));
                    }
                }
                (3, 0xE) => {
                    for i in 0..n {
                        let q = d + 4 * i;
                        attrs.push(Attr::Colour([b[q], b[q + 1], b[q + 2], b[q + 3]]));
                    }
                }
                _ => bail!("unexpected UNPACK (format {fmt:#x}, address {addr}) in a model packet"),
            }
            o += size;
        } else {
            match cmd {
                0x50 | 0x51 => o += 16 * if imm == 0 { 65536 } else { imm },
                0x20 => o += 4,
                0x30 | 0x31 => o += 16,
                0x4A => o += 8 * if num == 0 { 256 } else { num },
                _ => {} // NOP, STCYCL, OFFSET, BASE, MSCNT, ...
            }
        }
    }
    flush(&mut pos, &mut uvs, &mut attrs, &mut batches)?;
    Ok(batches)
}

fn same_vertex(a: &Vertex, b: &Vertex) -> bool {
    a.pos == b.pos && a.uv == b.uv && a.attr == b.attr
}

/// Triangles of one batch under GS strip rules. `offset` is the strip
/// position of vertex 0 when the batch continues the previous one's strip.
/// Returns the triangles and the strip position of the last vertex.
fn batch_tris(vs: &[Vertex], offset: isize) -> (Vec<[Vertex; 3]>, isize) {
    let mut tris = Vec::new();
    let mut start: isize = -offset;
    for i in 0..vs.len() {
        if i >= 1 && vs[i - 1].adc && vs[i].adc && !(i == 1 && offset != 0) {
            start = i as isize - 1;
        }
        if i >= 2 && !vs[i].adc {
            let (a, b, c) = (vs[i - 2], vs[i - 1], vs[i]);
            tris.push(if (i as isize - start) % 2 == 0 { [a, b, c] } else { [b, a, c] });
        }
    }
    (tris, vs.len() as isize - 1 - start)
}

/// Decode a group's packet into triangles (counter-clockwise).
fn group_tris(batches: &[Batch]) -> Vec<[Vertex; 3]> {
    let mut out = Vec::new();
    let mut prev: Option<&[Vertex]> = None;
    let mut last: isize = 0;
    for batch in batches {
        let vs = &batch.verts;
        let mut off = 0;
        if let Some(p) = prev {
            // A strip cut by the batch limit re-sends the previous batch's last
            // two vertices; its winding parity carries on.
            if vs.len() >= 3
                && p.len() >= 2
                && !p[p.len() - 1].adc
                && same_vertex(&vs[0], &p[p.len() - 2])
                && same_vertex(&vs[1], &p[p.len() - 1])
                && !vs[2].adc
            {
                off = last - 1;
            }
        }
        let (t, l) = batch_tris(vs, off);
        out.extend(t);
        last = l;
        prev = Some(vs);
    }
    out
}

/// Read a prototype geometry from a (little-endian) blob.
pub fn read_beta(blob: &Blob, off: u32) -> Result<Model> {
    let b = &blob.data;
    let g = off as usize;
    ensure!(u16_at(b, g)? == BETA_MAGIC, "no prototype geometry at {off:#x}");
    let num_lods = u16_at(b, g + 2)? as usize;
    let kind = u32_at(b, g + 4)?;
    ensure!(kind == 0, "skinned geometry (type {kind}) at {off:#x} is not supported");
    let lods_at = u32_at(b, g + 8)? as usize;
    let dist_at = u32_at(b, g + 0xC)? as usize;
    let tex_slots: HashMap<u32, u32> = blob.texture_relocs.iter().map(|r| (r.slot, r.value)).collect();

    let mut lods = Vec::with_capacity(num_lods);
    for l in 0..num_lods {
        let lod = lods_at + l * 24;
        ensure!(b.get(lod) == Some(&1), "LOD {l} of {off:#x} is not a static LOD");
        let num_groups = b[lod + 2] as usize;
        let groups_at = u32_at(b, lod + 8)? as usize;
        let names_at = u32_at(b, lod + 0x14)? as usize;
        let names = if blob.pointers.contains(&((lod + 0x14) as u32)) { name_list(b, names_at)? } else { vec![0] };
        let mut groups = Vec::with_capacity(num_groups);
        for k in 0..num_groups {
            let gr = groups_at + k * 32;
            let pkt = u32_at(b, gr + 0xC)? as usize;
            let tris = group_tris(&read_packet(b, pkt).with_context(|| format!("group {k} of LOD {l} at {off:#x}"))?);
            let expected = u32_at(b, gr + 0x10)? as usize;
            ensure!(
                tris.len() == expected,
                "group {k} of LOD {l} at {off:#x}: decoded {} triangles, header says {expected}",
                tris.len()
            );
            groups.push(Group {
                flags: u16_at(b, gr + 2)?,
                colour: [b[gr + 4], b[gr + 5], b[gr + 6], b[gr + 7]],
                blend: u16_at(b, gr + 0x14)?,
                sort: u16_at(b, gr + 0x16)?,
                texture: tex_slots.get(&((gr + 0x18) as u32)).copied(),
                tris,
            });
        }
        lods.push(Lod { names, groups });
    }
    let lod_distances = (0..num_lods).map(|i| f32_at(b, dist_at + 4 * i)).collect::<Result<_>>()?;
    let v3 = |o: usize| -> Result<[f32; 3]> { Ok([f32_at(b, o)?, f32_at(b, o + 4)?, f32_at(b, o + 8)?]) };
    Ok(Model {
        kind,
        lod_distances,
        bbox_min: v3(g + 0x10)?,
        radius_1c: f32_at(b, g + 0x1C)?,
        bbox_max: v3(g + 0x20)?,
        lods,
    })
}

/// Prototype vertex colours are half-scale PS2 values.
fn gc_colour(c: [u8; 4]) -> [u8; 4] {
    let ch = |v: u8| if v >= 0x7F { 0xFF } else { v * 2 };
    [ch(c[0]), ch(c[1]), ch(c[2]), (c[3] as u32 * 4 + 3).min(255) as u8]
}

/// GC group flags before the LOD class bits are added.
fn gc_flags(beta: u16) -> u16 {
    if beta & F_ALPHA != 0 {
        beta & !F_LIT
    } else {
        beta
    }
}

/// Does a group with these (GC) flags index normals (else colours)?
fn uses_normals(f: u16) -> bool {
    f & (F_ENV | F_LIT | F_ENV_ADD) != 0 && f & (F_ALPHA | F_ALPHA2) == 0
}

/// De-duplicating array builder keyed by exact bit patterns.
struct Pool<const N: usize> {
    index: HashMap<[u32; N], u16>,
    items: Vec<[u32; N]>,
}

impl<const N: usize> Pool<N> {
    fn new() -> Self {
        Pool { index: HashMap::new(), items: vec![] }
    }
    fn add(&mut self, v: [u32; N]) -> Result<u16> {
        if let Some(&i) = self.index.get(&v) {
            return Ok(i);
        }
        ensure!(self.items.len() < 0xFFFF, "more than 65535 vertices in one LOD");
        let i = self.items.len() as u16;
        self.items.push(v);
        self.index.insert(v, i);
        Ok(i)
    }
}

fn bits3(v: [f32; 3]) -> [u32; 3] {
    [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()]
}

/// Writer for a new big-endian object blob with offsets from 0.
struct Out {
    blob: Blob,
}

impl Out {
    fn at(&mut self, alignment: usize) -> u32 {
        let a = align(self.blob.data.len(), alignment);
        self.blob.data.resize(a, 0);
        a as u32
    }
    fn reserve(&mut self, alignment: usize, len: usize) -> u32 {
        let a = self.at(alignment);
        self.blob.data.resize(a as usize + len, 0);
        a
    }
    fn w32(&mut self, off: u32, v: u32) {
        self.blob.put_u32(BE, off, v);
    }
    fn w16(&mut self, off: u32, v: u16) {
        let o = off as usize;
        self.blob.data[o..o + 2].copy_from_slice(&v.to_be_bytes());
    }
    fn wf(&mut self, off: u32, v: f32) {
        self.w32(off, v.to_bits());
    }
    fn ptr(&mut self, slot: u32, target: u32) {
        self.blob.set_ptr(BE, slot, target);
    }
    fn push(&mut self, bytes: &[u8], alignment: usize) -> u32 {
        let a = self.at(alignment);
        self.blob.data.extend_from_slice(bytes);
        a
    }
}

fn write_header(o: &mut Out, m: &Model, num_lods: u16) -> (u32, u32) {
    let g = o.reserve(32, 0x40);
    o.w16(g, GC_MAGIC);
    o.w16(g + 2, num_lods);
    o.w16(g + 4, m.kind as u16);
    for k in 0..3usize {
        let at = 4 * k as u32;
        o.wf(g + 0x10 + at, m.bbox_min[k]);
        o.wf(g + 0x20 + at, m.bbox_max[k]);
        o.wf(g + 0x30 + at, (m.bbox_min[k] + m.bbox_max[k]) / 2.0);
    }
    o.wf(g + 0x1C, m.radius_1c);
    let d: f32 = (0..3).map(|k| (m.bbox_max[k] - m.bbox_min[k]).powi(2)).sum::<f32>().sqrt();
    o.wf(g + 0x2C, d / 2.0);
    let lods = o.reserve(16, 0x18 * num_lods as usize);
    o.ptr(g + 8, lods);
    (g, lods)
}

fn write_distances(o: &mut Out, g: u32, m: &Model) {
    let d = o.reserve(16, 4 * m.lod_distances.len());
    for (i, &v) in m.lod_distances.iter().enumerate() {
        o.wf(d + 4 * i as u32, v);
    }
    o.ptr(g + 0xC, d);
}

/// Build a GameCube geometry. `remap_texture` turns a prototype texture
/// relocation value into the GC one (or `None` to drop the texture).
/// The result is a stand-alone blob (offsets from 0) to graft into a pack.
pub fn build_gc(m: &Model, remap_texture: &mut dyn FnMut(u32) -> Option<u32>) -> Result<Blob> {
    let mut o = Out { blob: Blob::default() };
    let (g, lods_at) = write_header(&mut o, m, m.lods.len() as u16);
    let mut dls: Vec<(u32, Vec<u8>)> = vec![];

    for (l, lod) in m.lods.iter().enumerate() {
        let lod_at = lods_at + 0x18 * l as u32;
        let flags: Vec<u16> = lod.groups.iter().map(|gr| gc_flags(gr.flags)).collect();
        let need_n = flags.iter().any(|&f| uses_normals(f));
        let need_c = flags.iter().any(|&f| !uses_normals(f));
        let class = match (need_n, need_c) {
            (true, true) => F_VB_BOTH,
            (true, false) => F_VB_NORMALS,
            _ => F_VB_COLOURS,
        };

        let (mut pos, mut uv, mut nrm, mut clr) = (Pool::<3>::new(), Pool::<2>::new(), Pool::<3>::new(), Pool::<1>::new());
        let mut group_dls = vec![];
        for (gr, &f) in lod.groups.iter().zip(&flags) {
            let mut dl = vec![0x90u8];
            let count = gr.tris.len() * 3;
            ensure!(count <= 0xFFFF, "group has too many vertices for one draw");
            dl.extend_from_slice(&(count as u16).to_be_bytes());
            for t in &gr.tris {
                for v in t {
                    let p = pos.add(bits3(v.pos))?;
                    let second = if uses_normals(f) {
                        let n = match v.attr {
                            Attr::Normal(n) => n,
                            _ => [0.0, 1.0, 0.0],
                        };
                        nrm.add(bits3(n))?
                    } else {
                        let c = match v.attr {
                            Attr::Colour(c) => gc_colour(c),
                            _ => [0xFF; 4],
                        };
                        clr.add([u32::from_be_bytes(c)])?
                    };
                    let t = uv.add([v.uv[0].to_bits(), v.uv[1].to_bits()])?;
                    for i in [p, second, t] {
                        dl.extend_from_slice(&i.to_be_bytes());
                    }
                }
            }
            dl.resize(align(dl.len(), 32), 0);
            group_dls.push(dl);
        }

        // Groups.
        let groups_at = o.reserve(16, 0x20 * lod.groups.len());
        o.w16(lod_at + 2, lod.groups.len() as u16);
        o.ptr(lod_at + 8, groups_at);
        for (k, (gr, &f)) in lod.groups.iter().zip(&flags).enumerate() {
            let at = groups_at + 0x20 * k as u32;
            o.w16(at + 2, f | class);
            o.blob.data[at as usize + 4..at as usize + 8].copy_from_slice(&gr.colour);
            o.w32(at + 0x10, group_dls[k].len() as u32);
            o.w16(at + 0x14, gr.blend);
            o.w16(at + 0x16, gr.sort);
            let tex = if f & F_ENV != 0 { None } else { gr.texture.and_then(&mut *remap_texture) };
            match tex {
                Some(v) => {
                    o.w32(at + 0x18, 0);
                    o.blob.texture_relocs.push(Reloc { slot: at + 0x18, value: v });
                }
                None => o.w32(at + 0x18, NO_TEXTURE),
            }
            dls.push((at, std::mem::take(&mut group_dls[k])));
        }

        // Vertex buffer: positions | UVs | colours | normals.
        let mut data = vec![];
        for p in &pos.items {
            p.iter().for_each(|x| data.extend_from_slice(&x.to_be_bytes()));
        }
        let uv_off = data.len() as u32;
        for t in &uv.items {
            t.iter().for_each(|x| data.extend_from_slice(&x.to_be_bytes()));
        }
        let clr_off = if clr.items.is_empty() { 0 } else { data.len() as u32 };
        for c in &clr.items {
            data.extend_from_slice(&c[0].to_be_bytes());
        }
        let nrm_off = if nrm.items.is_empty() { 0 } else { data.len() as u32 };
        for n in &nrm.items {
            n.iter().for_each(|x| data.extend_from_slice(&x.to_be_bytes()));
        }
        let vb = o.reserve(16, 0x10);
        o.w32(vb, uv_off);
        o.w32(vb + 4, clr_off);
        o.w32(vb + 8, nrm_off);
        o.w32(vb + 0xC, 0x10 + data.len() as u32);
        o.blob.data.extend_from_slice(&data);
        o.ptr(lod_at + 0xC, vb);

        let names = o.push(&lod.names, 4);
        o.ptr(lod_at + 0x14, names);
    }
    write_distances(&mut o, g, m);
    for (group, dl) in dls {
        let at = o.push(&dl, 32);
        o.ptr(group + 0x1C, at);
    }
    o.at(32);
    Ok(o.blob)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn neg_face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let (u, v) = (sub(b, a), sub(c, a));
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < 1e-12 {
        [0.0; 3]
    } else {
        [-n[0] / len, -n[1] / len, -n[2] / len]
    }
}

/// Build a GameCube shadow volume from the prototype's shadow hull (a
/// closed mesh): one edge record per edge shared by two non-coplanar faces.
pub fn build_gc_shadow(m: &Model) -> Result<Blob> {
    ensure!(m.lods.len() == 1, "shadow hull has {} LODs, expected 1", m.lods.len());
    let lod = &m.lods[0];
    let mut o = Out { blob: Blob::default() };
    let (g, lod_at) = write_header(&mut o, m, 1);

    let mut pos = Pool::<3>::new();
    let mut edges_per_group: Vec<Vec<u8>> = vec![];
    for gr in &lod.groups {
        // directed edge -> face normal
        let mut faces: HashMap<(u16, u16), [f32; 3]> = HashMap::new();
        let mut order: Vec<(u16, u16)> = vec![];
        for t in &gr.tris {
            let idx = [pos.add(bits3(t[0].pos))?, pos.add(bits3(t[1].pos))?, pos.add(bits3(t[2].pos))?];
            if idx[0] == idx[1] || idx[1] == idx[2] || idx[0] == idx[2] {
                continue; // degenerate (strip joins)
            }
            let n = neg_face_normal(t[0].pos, t[1].pos, t[2].pos);
            for (a, b) in [(idx[0], idx[1]), (idx[1], idx[2]), (idx[2], idx[0])] {
                if faces.insert((a, b), n).is_none() {
                    order.push((a, b));
                }
            }
        }
        let mut bytes = vec![];
        for &(a, b) in &order {
            let Some(&n1) = faces.get(&(b, a)) else { continue };
            if a > b {
                continue; // each undirected edge once
            }
            let n0 = faces[&(a, b)];
            let dot = n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2];
            if dot > 0.99999 {
                continue; // coplanar: never a silhouette
            }
            for x in n0 {
                bytes.extend_from_slice(&x.to_be_bytes());
            }
            bytes.extend_from_slice(&a.to_be_bytes());
            bytes.extend_from_slice(&b.to_be_bytes());
            for x in n1 {
                bytes.extend_from_slice(&x.to_be_bytes());
            }
            bytes.extend_from_slice(&0u32.to_be_bytes());
        }
        edges_per_group.push(bytes);
    }

    let groups_at = o.reserve(16, 0x20 * lod.groups.len());
    o.w16(lod_at + 2, lod.groups.len() as u16);
    o.ptr(lod_at + 8, groups_at);
    let mut pbytes = vec![];
    for p in &pos.items {
        p.iter().for_each(|x| pbytes.extend_from_slice(&x.to_be_bytes()));
    }
    let positions = o.push(&pbytes, 32);
    o.ptr(lod_at + 0xC, positions);
    let names = o.push(&lod.names, 4);
    o.ptr(lod_at + 0x14, names);
    write_distances(&mut o, g, m);
    for (k, edges) in edges_per_group.iter().enumerate() {
        let at = groups_at + 0x20 * k as u32;
        o.w16(at + 2, F_SHADOW);
        o.blob.data[at as usize + 4..at as usize + 8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x00]);
        o.w32(at + 0x10, (edges.len() / 0x20) as u32);
        let e = o.push(edges, 32);
        o.ptr(at + 0x1C, e);
    }
    o.at(32);
    Ok(o.blob)
}

/// Every byte range of a prototype geometry (header, LODs, groups,
/// packets, names, distances), so the converter can drop the old copy.
pub fn beta_extent(blob: &Blob, off: u32) -> Result<Vec<(u32, u32)>> {
    let b = &blob.data;
    let g = off as usize;
    let num_lods = u16_at(b, g + 2)? as usize;
    let lods_at = u32_at(b, g + 8)? as usize;
    let dist_at = u32_at(b, g + 0xC)? as usize;
    let mut r = vec![(off, off + 0x40), (lods_at as u32, (lods_at + 24 * num_lods) as u32), (dist_at as u32, (dist_at + 4 * num_lods) as u32)];
    for l in 0..num_lods {
        let lod = lods_at + 24 * l;
        let ng = b[lod + 2] as usize;
        let groups = u32_at(b, lod + 8)? as usize;
        r.push((groups as u32, (groups + 32 * ng) as u32));
        if blob.pointers.contains(&((lod + 0x14) as u32)) {
            let n = u32_at(b, lod + 0x14)? as usize;
            r.push((n as u32, (n + name_list(b, n)?.len()) as u32));
        }
        for k in 0..ng {
            let pkt = u32_at(b, groups + 32 * k + 0xC)? as usize;
            let qwc = (u32_at(b, pkt + 0x10)? & 0xFFFF) as usize;
            r.push((pkt as u32, (pkt + 0x20 + qwc * 16) as u32));
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, adc: bool) -> Vertex {
        Vertex { pos: [x, 0.0, 0.0], uv: [0.0, 0.0], attr: Attr::None, adc }
    }

    #[test]
    fn strip_parity() {
        // strip of 5 vertices -> 3 triangles, alternating winding
        let vs = [v(0.0, true), v(1.0, true), v(2.0, false), v(3.0, false), v(4.0, false)];
        let (t, _) = batch_tris(&vs, 0);
        let xs: Vec<[f32; 3]> = t.iter().map(|t| [t[0].pos[0], t[1].pos[0], t[2].pos[0]]).collect();
        assert_eq!(xs, vec![[0.0, 1.0, 2.0], [2.0, 1.0, 3.0], [2.0, 3.0, 4.0]]);
    }

    #[test]
    fn colour_scale() {
        assert_eq!(gc_colour([0x7F, 0x12, 0, 0x3F]), [0xFF, 0x24, 0, 0xFF]);
    }
}
