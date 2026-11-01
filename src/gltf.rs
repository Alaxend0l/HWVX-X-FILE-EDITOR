//! Export GameCube models to glTF 2.0 binary (`.glb`).
//!
//! * Car packs: the car's skeleton (rest pose) becomes a node hierarchy; the
//!   model's rigid parts and breakable body parts hang off their bones, and
//!   skinned geometry becomes a glTF skin over the same bones.
//! * Any other pack (items, worlds, quad geometry packs, interfaces...), or a
//!   car with [`Options::scan`]: every geometry found in the pack, each as its
//!   own node at the origin (world geometry is already in world space).
//!
//! Geometry layout: `docs/beta-cars/research/geometry.md` §1. Each group
//! becomes one primitive. Its display list holds `(position, normal|colour,
//! uv)` index triples (plus a u8 matrix index in skinned geometry); which
//! one the second index is depends on the group flags, as in RenderGroup
//! (Render_Geometry_.c). Textures (the pack's own, shared packs', or a world
//! texture pack's) are embedded as PNG. glTF and the game agree on axes
//! (Y up, cars face +Z), winding (counter-clockwise) and UV origin (top left),
//! so values are copied unchanged.

use std::collections::{BTreeMap, HashMap};

use anyhow::{ensure, Result};
use serde_json::{json, Value};

use crate::blob::Blob;
use crate::convert::geometry::{F_ALPHA, F_ALPHA2, F_ENV, F_ENV_ADD, F_LIT, F_SHADOW, GC_MAGIC};
use crate::convert::texture::Image;
use crate::format::Endian;

const E: Endian = Endian::Big;

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Export every LOD (LOD n > 0 as extra nodes named `..._lodN`), not just LOD 0.
    pub all_lods: bool,
    /// Cars: also export the damaged variants of breakable body parts.
    pub damaged: bool,
    /// Export every geometry in the pack at the origin, even for a car.
    pub scan: bool,
}

/// Decoded textures a pack can refer to.
#[derive(Default)]
pub struct Textures {
    /// Texture relocations with pack id 0: the pack's own textures (or, for a
    /// quad geometry pack, its world texture pack's).
    pub local: Vec<Image>,
    /// Shared packs by pack id (1 = HW_share, 2 = HW_Cars share, ...).
    pub shared: HashMap<u32, Vec<Image>>,
}

/// What the export did, for the command line.
#[derive(Debug, Default)]
pub struct Report {
    pub meshes: usize,
    pub nodes: usize,
    pub triangles: usize,
    pub textures: usize,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Reading GameCube geometry

fn u16_at(b: &Blob, o: u32) -> Option<u16> {
    let s = b.data.get(o as usize..o as usize + 2)?;
    Some(u16::from_be_bytes([s[0], s[1]]))
}

fn f32_at(b: &Blob, o: u32) -> Option<f32> {
    b.u32(E, o).map(f32::from_bits)
}

fn v3_at(b: &Blob, o: u32) -> Option<[f32; 3]> {
    Some([f32_at(b, o)?, f32_at(b, o + 4)?, f32_at(b, o + 8)?])
}

/// Is there a GameCube geometry header at `g`?
pub fn is_geometry(b: &Blob, g: u32) -> bool {
    g % 32 == 0 && u16_at(b, g) == Some(GC_MAGIC) && b.pointers.contains(&(g + 8)) && b.pointers.contains(&(g + 0xC))
}

/// One display-list vertex: matrix index (skinned only) and the three indices.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct DlVert {
    mtx: u8,
    pos: u16,
    second: u16,
    uv: u16,
}

struct Group {
    flags: u16,
    colour: [u8; 4],
    blend: u16,
    /// Texture relocation value (`index | pack << 16`).
    texture: Option<u32>,
    /// Triangle list.
    verts: Vec<DlVert>,
}

#[derive(Default)]
struct VertexBuffer {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    clr: Vec<[u8; 4]>,
    nrm: Vec<[f32; 3]>,
}

struct Lod {
    vb: VertexBuffer,
    groups: Vec<Group>,
    /// Texture names of the LOD's name list (first-use order).
    names: Vec<String>,
}

struct Geometry {
    skinned: bool,
    lods: Vec<Lod>,
}

fn read_vb(b: &Blob, vb: u32) -> Option<VertexBuffer> {
    let (uv_off, clr_off, nrm_off, size) = (b.u32(E, vb)?, b.u32(E, vb + 4)?, b.u32(E, vb + 8)?, b.u32(E, vb + 0xC)?);
    let data = vb + 0x10;
    // arrays are packed in the order pos | uv | clr | nrm; counts are implicit
    let end = size.saturating_sub(0x10).min(b.data.len() as u32 - data.min(b.data.len() as u32));
    let next = |from: u32| [clr_off, nrm_off, end].into_iter().filter(|&o| o > from).min().unwrap_or(end).min(end);
    let mut out = VertexBuffer::default();
    for i in 0..uv_off.min(end) / 12 {
        out.pos.push(v3_at(b, data + 12 * i)?);
    }
    for i in 0..next(uv_off).saturating_sub(uv_off) / 8 {
        let o = data + uv_off + 8 * i;
        out.uv.push([f32_at(b, o)?, f32_at(b, o + 4)?]);
    }
    if clr_off != 0 {
        for i in 0..next(clr_off).saturating_sub(clr_off) / 4 {
            let o = (data + clr_off + 4 * i) as usize;
            out.clr.push(b.data.get(o..o + 4)?.try_into().ok()?);
        }
    }
    if nrm_off != 0 {
        for i in 0..end.saturating_sub(nrm_off) / 12 {
            out.nrm.push(v3_at(b, data + nrm_off + 12 * i)?);
        }
    }
    Some(out)
}

/// The triangles of one display list (GX_DRAW_TRIANGLES, also strips and
/// fans, which the game data never uses).
fn read_dl(b: &Blob, dl: u32, size: u32, skinned: bool, warn: &mut Vec<String>) -> Vec<DlVert> {
    let end = (dl as usize + size as usize).min(b.data.len());
    let stride = if skinned { 7 } else { 6 };
    let mut p = dl as usize;
    let mut tris = vec![];
    while p < end {
        let op = b.data[p];
        if op == 0 {
            p += 1; // GX_NOP padding
            continue;
        }
        let Some(n) = u16_at(b, p as u32 + 1) else { break };
        let n = n as usize;
        p += 3;
        if !matches!(op & 0xF8, 0x90 | 0x98 | 0xA0) || p + n * stride > end {
            warn.push(format!("display list at {dl:#x}: unsupported command {op:#04x}"));
            break;
        }
        let vs: Vec<DlVert> = (0..n)
            .map(|i| {
                let o = p + i * stride;
                let (mtx, o) = if skinned { (b.data[o], o + 1) } else { (0, o) };
                let h = |k: usize| u16::from_be_bytes([b.data[o + k], b.data[o + k + 1]]);
                DlVert { mtx, pos: h(0), second: h(2), uv: h(4) }
            })
            .collect();
        p += n * stride;
        match op & 0xF8 {
            0x90 => tris.extend(vs.chunks_exact(3).flatten().copied()),
            0x98 => {
                for i in 2..n {
                    let t = if i % 2 == 0 { [i - 2, i - 1, i] } else { [i - 1, i - 2, i] };
                    tris.extend(t.map(|k| vs[k]));
                }
            }
            _ => {
                for i in 2..n {
                    tris.extend([vs[0], vs[i - 1], vs[i]]);
                }
            }
        }
    }
    tris
}

fn name_list(b: &Blob, mut o: u32) -> Vec<String> {
    let mut names = vec![];
    while let Some(rest) = b.data.get(o as usize..) {
        let n = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
        if n == 0 || names.len() > 256 {
            break;
        }
        names.push(String::from_utf8_lossy(&rest[..n]).into_owned());
        o += n as u32 + 1;
    }
    names
}

fn read_geometry(b: &Blob, g: u32, tex_slots: &HashMap<u32, u32>, warn: &mut Vec<String>) -> Option<Geometry> {
    if !is_geometry(b, g) {
        return None;
    }
    let num_lods = u16_at(b, g + 2)?.min(16) as u32;
    let skinned = u16_at(b, g + 4)? != 0;
    let lods_at = b.ptr(E, g + 8)?;
    let mut lods = vec![];
    for l in 0..num_lods {
        let lod = lods_at + 0x18 * l;
        let ng = u16_at(b, lod + 2)? as u32;
        let groups_at = b.ptr(E, lod + 8)?;
        let first_flags = u16_at(b, groups_at + 2)?;
        if ng == 0 || first_flags & F_SHADOW != 0 {
            continue; // shadow volume (edge lists), nothing to draw
        }
        let vb = b.ptr(E, lod + 0xC).and_then(|v| read_vb(b, v)).unwrap_or_default();
        let names = b.ptr(E, lod + 0x14).map(|n| name_list(b, n)).unwrap_or_default();
        let mut groups = vec![];
        for k in 0..ng {
            let gr = groups_at + 0x20 * k;
            let flags = u16_at(b, gr + 2)?;
            let Some(dl) = b.ptr(E, gr + 0x1C) else { continue };
            if flags & F_SHADOW != 0 {
                continue;
            }
            let verts = read_dl(b, dl, b.u32(E, gr + 0x10)?, skinned, warn);
            let c = gr as usize + 4;
            groups.push(Group {
                flags,
                colour: b.data[c..c + 4].try_into().ok()?,
                blend: u16_at(b, gr + 0x14)?,
                texture: tex_slots.get(&(gr + 0x18)).copied(),
                verts,
            });
        }
        lods.push(Lod { vb, groups, names });
    }
    Some(Geometry { skinned, lods })
}

// ---------------------------------------------------------------------------
// PNG (uncompressed deflate; glTF viewers only need it to be valid)

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in bytes {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

pub fn png(img: &Image) -> Vec<u8> {
    let mut raw = Vec::with_capacity((img.w * 4 + 1) * img.h);
    for row in img.rgba.chunks_exact(img.w * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(0xFFFF).collect();
    for (i, blk) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        let n = blk.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(blk);
    }
    if blocks.is_empty() {
        z.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    };
    let mut ihdr = vec![];
    ihdr.extend_from_slice(&(img.w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(img.h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

// ---------------------------------------------------------------------------
// glTF document

const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_BYTE: u32 = 5121;
const UNSIGNED_SHORT: u32 = 5123;
const UNSIGNED_INT: u32 = 5125;

#[derive(Clone, PartialEq, Eq, Hash)]
struct MatKey {
    texture: Option<usize>,
    kind: Kind,
    colour: [u8; 4],
    flags: u16,
    blend: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    /// Lit, normals, optional texture.
    Lit,
    /// Unlit: vertex colours, optional texture.
    Unlit,
    /// Translucent: vertex colours with alpha, texture.
    Alpha,
    /// Sphere-mapped chrome: normals, tinted by the group colour.
    Env,
}

fn group_kind(flags: u16) -> Kind {
    if flags & (F_ALPHA | F_ALPHA2) != 0 {
        Kind::Alpha
    } else if flags & F_ENV != 0 {
        Kind::Env
    } else if flags & (F_LIT | F_ENV_ADD) != 0 {
        Kind::Lit
    } else {
        Kind::Unlit
    }
}

struct Doc<'a> {
    textures: &'a Textures,
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
    meshes: Vec<Value>,
    materials: Vec<Value>,
    gl_textures: Vec<Value>,
    images: Vec<Value>,
    nodes: Vec<Value>,
    skins: Vec<Value>,
    mat_index: HashMap<MatKey, usize>,
    /// texture relocation value -> (glTF texture, has cut-out/translucent alpha)
    tex_index: HashMap<u32, Option<(usize, bool)>>,
    tex_names: HashMap<u32, String>,
    /// (geometry, lod) -> glTF mesh (None: nothing to draw)
    mesh_index: HashMap<(u32, usize), Option<usize>>,
    unlit_used: bool,
    report: Report,
}

impl<'a> Doc<'a> {
    fn new(textures: &'a Textures) -> Self {
        Doc {
            textures,
            bin: vec![],
            views: vec![],
            accessors: vec![],
            meshes: vec![],
            materials: vec![],
            gl_textures: vec![],
            images: vec![],
            nodes: vec![],
            skins: vec![],
            mat_index: HashMap::new(),
            tex_index: HashMap::new(),
            tex_names: HashMap::new(),
            mesh_index: HashMap::new(),
            unlit_used: false,
            report: Report::default(),
        }
    }

    fn warn(&mut self, w: String) {
        if !self.report.warnings.contains(&w) {
            self.report.warnings.push(w);
        }
    }

    fn view(&mut self, bytes: &[u8], target: Option<u32>, stride: Option<usize>) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let mut v = json!({ "buffer": 0, "byteOffset": self.bin.len(), "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        if let Some(s) = stride {
            v["byteStride"] = json!(s);
        }
        self.bin.extend_from_slice(bytes);
        self.views.push(v);
        self.views.len() - 1
    }

    fn accessor(&mut self, bytes: &[u8], target: u32, comp: u32, count: usize, ty: &str, extra: Value) -> usize {
        let view = self.view(bytes, Some(target), None);
        let mut a = json!({ "bufferView": view, "componentType": comp, "count": count, "type": ty });
        if let (Value::Object(a), Value::Object(x)) = (&mut a, extra) {
            a.extend(x);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }

    fn floats(&mut self, data: &[f32], ty: &str, n: usize, minmax: bool) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();
        let extra = if minmax {
            let mut lo = vec![f32::MAX; n];
            let mut hi = vec![f32::MIN; n];
            for v in data.chunks_exact(n) {
                for k in 0..n {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            json!({ "min": lo, "max": hi })
        } else {
            json!({})
        };
        self.accessor(&bytes, ARRAY_BUFFER, FLOAT, data.len() / n, ty, extra)
    }

    /// glTF texture for a texture relocation value, embedding the image.
    fn texture(&mut self, value: u32) -> Option<(usize, bool)> {
        if let Some(t) = self.tex_index.get(&value) {
            return *t;
        }
        let (pack, idx) = (value >> 16, (value & 0xFFFF) as usize);
        let set = if pack == 0 { Some(&self.textures.local) } else { self.textures.shared.get(&pack) };
        let img = set.and_then(|s| s.get(idx));
        let t = match img {
            Some(img) => {
                let bytes = png(img);
                let view = self.view(&bytes, None, None);
                let name = self.tex_names.get(&value).cloned().unwrap_or_else(|| format!("tex_{pack}_{idx}"));
                self.images.push(json!({ "name": name, "mimeType": "image/png", "bufferView": view }));
                self.gl_textures.push(json!({ "sampler": 0, "source": self.images.len() - 1, "name": name }));
                let alpha = img.rgba.chunks_exact(4).any(|p| p[3] < 255);
                Some((self.gl_textures.len() - 1, alpha))
            }
            None => {
                let w = if pack == 0 && self.textures.local.is_empty() {
                    "the pack has no textures of its own (geometry packs: pass --textures <world texture pack>)".to_string()
                } else if pack == 0 {
                    format!("texture {idx} is missing from the pack")
                } else {
                    format!("shared pack {pack} not found (pass --share <extracted archive holding it>); its textures are left out")
                };
                self.warn(w);
                None
            }
        };
        self.tex_index.insert(value, t);
        t
    }

    fn material(&mut self, g: &Group) -> usize {
        let kind = group_kind(g.flags);
        // env groups have no base texture on GameCube (the env map replaces it)
        let tex = if kind == Kind::Env { None } else { g.texture.and_then(|v| self.texture(v)) };
        let key = MatKey { texture: tex.map(|t| t.0), kind, colour: g.colour, flags: g.flags, blend: g.blend };
        if let Some(&m) = self.mat_index.get(&key) {
            return m;
        }
        let tex_name = tex.and_then(|(t, _)| self.gl_textures[t]["name"].as_str().map(str::to_string));
        let mut pbr = json!({ "metallicFactor": 0.0, "roughnessFactor": 0.8 });
        let mut m = json!({});
        if let Some((t, _)) = tex {
            pbr["baseColorTexture"] = json!({ "index": t });
        }
        let name = match kind {
            Kind::Env => {
                let c = if g.colour[..3] == [0, 0, 0] { [200, 200, 200] } else { [g.colour[0], g.colour[1], g.colour[2]] };
                pbr["baseColorFactor"] = json!([c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]);
                pbr["metallicFactor"] = json!(1.0);
                pbr["roughnessFactor"] = json!(0.15);
                format!("chrome_{:02x}{:02x}{:02x}", c[0], c[1], c[2])
            }
            Kind::Alpha => {
                m["alphaMode"] = json!("BLEND");
                m["doubleSided"] = json!(true);
                if g.blend == 2 {
                    // additive (lights, glows): there is no glTF equivalent
                    m["emissiveFactor"] = json!([1.0, 1.0, 1.0]);
                    if let Some((t, _)) = tex {
                        m["emissiveTexture"] = json!({ "index": t });
                    }
                }
                tex_name.clone().unwrap_or_else(|| "translucent".into())
            }
            Kind::Lit | Kind::Unlit => {
                if tex.is_some_and(|t| t.1) {
                    m["alphaMode"] = json!("MASK"); // 1-bit cut-out (CMPR alpha)
                    m["alphaCutoff"] = json!(0.5);
                }
                if g.flags & F_ENV_ADD != 0 {
                    pbr["metallicFactor"] = json!(0.4);
                    pbr["roughnessFactor"] = json!(0.3);
                }
                tex_name.clone().unwrap_or_else(|| if kind == Kind::Lit { "lit".into() } else { "flat".into() })
            }
        };
        if kind == Kind::Unlit {
            m["extensions"] = json!({ "KHR_materials_unlit": {} });
            self.unlit_used = true;
        }
        m["name"] = json!(name);
        m["pbrMetallicRoughness"] = pbr;
        m["extras"] = json!({
            "hwvx_flags": format!("{:#06x}", g.flags),
            "hwvx_blend": g.blend,
            "hwvx_colour": format!("{:02x}{:02x}{:02x}{:02x}", g.colour[0], g.colour[1], g.colour[2], g.colour[3]),
        });
        self.materials.push(m);
        self.mat_index.insert(key, self.materials.len() - 1);
        self.materials.len() - 1
    }

    /// Remember the texture names of a LOD's name list: they are listed in
    /// first-use order of its textured (non-env) groups.
    fn learn_names(&mut self, lod: &Lod) {
        let mut used: Vec<u32> = vec![];
        for g in &lod.groups {
            if let Some(v) = g.texture.filter(|_| group_kind(g.flags) != Kind::Env) {
                if !used.contains(&v) {
                    used.push(v);
                }
            }
        }
        if used.len() == lod.names.len() {
            for (v, n) in used.into_iter().zip(&lod.names) {
                let n = n.strip_suffix(".tga").or_else(|| n.strip_suffix(".TGA")).unwrap_or(n);
                self.tex_names.entry(v).or_insert_with(|| n.to_string());
            }
        }
    }

    /// glTF mesh of LOD `l` of the geometry at `g`. `joints`: bone count, for
    /// skinned geometry exported as a skin.
    fn mesh(&mut self, b: &Blob, tex_slots: &HashMap<u32, u32>, g: u32, l: usize, joints: Option<usize>) -> Option<usize> {
        if let Some(m) = self.mesh_index.get(&(g, l)) {
            return *m;
        }
        let mut warn = vec![];
        let geo = read_geometry(b, g, tex_slots, &mut warn);
        for w in warn {
            self.warn(w);
        }
        let m = geo.and_then(|geo| {
            let lod = geo.lods.get(l)?;
            self.learn_names(lod);
            let skin = if geo.skinned { joints } else { None };
            if geo.skinned && skin.is_none() {
                self.warn(format!("skinned geometry at {g:#x} exported unposed (no skeleton)"));
            }
            let prims: Vec<Value> = lod.groups.iter().filter_map(|gr| self.primitive(&lod.vb, gr, skin, g)).collect();
            if prims.is_empty() {
                return None;
            }
            let name = if l == 0 { format!("geom_{g:x}") } else { format!("geom_{g:x}_lod{l}") };
            self.meshes.push(json!({ "name": name, "primitives": prims }));
            Some(self.meshes.len() - 1)
        });
        self.mesh_index.insert((g, l), m);
        m
    }

    fn primitive(&mut self, vb: &VertexBuffer, gr: &Group, skin: Option<usize>, g: u32) -> Option<Value> {
        let kind = group_kind(gr.flags);
        let normals = matches!(kind, Kind::Lit | Kind::Env);
        let textured = kind != Kind::Env && gr.texture.is_some();
        let mut remap: HashMap<DlVert, u32> = HashMap::new();
        let mut order: Vec<DlVert> = vec![];
        let mut indices: Vec<u32> = vec![];
        let mut bad = 0;
        for t in gr.verts.chunks_exact(3) {
            if t.iter().any(|v| v.pos as usize >= vb.pos.len()) {
                bad += 1;
                continue;
            }
            for v in t {
                let i = *remap.entry(*v).or_insert_with(|| {
                    order.push(*v);
                    order.len() as u32 - 1
                });
                indices.push(i);
            }
        }
        if bad > 0 {
            self.warn(format!("geometry {g:#x}: {bad} triangle(s) with out-of-range positions dropped"));
        }
        if indices.is_empty() {
            return None;
        }
        self.report.triangles += indices.len() / 3;
        let mut attrs = serde_json::Map::new();
        let pos: Vec<f32> = order.iter().flat_map(|v| vb.pos[v.pos as usize]).collect();
        attrs.insert("POSITION".into(), json!(self.floats(&pos, "VEC3", 3, true)));
        if normals {
            let n: Vec<f32> = order
                .iter()
                .flat_map(|v| {
                    let n = vb.nrm.get(v.second as usize).copied().unwrap_or([0.0, 1.0, 0.0]);
                    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    if l > 1e-6 { [n[0] / l, n[1] / l, n[2] / l] } else { [0.0, 1.0, 0.0] }
                })
                .collect();
            attrs.insert("NORMAL".into(), json!(self.floats(&n, "VEC3", 3, false)));
        } else {
            let c: Vec<u8> = order
                .iter()
                .flat_map(|v| {
                    let mut c = vb.clr.get(v.second as usize).copied().unwrap_or([0xFF; 4]);
                    if kind != Kind::Alpha {
                        c[3] = 0xFF; // opaque groups read RGB only
                    }
                    c
                })
                .collect();
            if c.iter().any(|&x| x != 0xFF) {
                let a = self.accessor(&c, ARRAY_BUFFER, UNSIGNED_BYTE, order.len(), "VEC4", json!({ "normalized": true }));
                attrs.insert("COLOR_0".into(), json!(a));
            }
        }
        if textured {
            let uv: Vec<f32> = order.iter().flat_map(|v| vb.uv.get(v.uv as usize).copied().unwrap_or([0.0; 2])).collect();
            attrs.insert("TEXCOORD_0".into(), json!(self.floats(&uv, "VEC2", 2, false)));
        }
        if let Some(n) = skin {
            let mut j = Vec::with_capacity(order.len() * 4);
            let mut bad = false;
            for v in &order {
                // GX matrix row address -> bone (matrixlookup: 3 rows each, 0x3C skipped)
                let m = v.mtx as usize;
                let bone = if m < 0x3C { m / 3 } else { 20 + (m - 0x40) / 3 };
                bad |= bone >= n;
                j.extend_from_slice(&[if bone < n { bone as u8 } else { 0 }, 0, 0, 0]);
            }
            if bad {
                self.warn(format!("skinned geometry {g:#x}: matrix index past the skeleton"));
            }
            attrs.insert("JOINTS_0".into(), json!(self.accessor(&j, ARRAY_BUFFER, UNSIGNED_BYTE, order.len(), "VEC4", json!({}))));
            let w: Vec<f32> = order.iter().flat_map(|_| [1.0, 0.0, 0.0, 0.0]).collect();
            attrs.insert("WEIGHTS_0".into(), json!(self.floats(&w, "VEC4", 4, false)));
        }
        let idx = if order.len() <= 0xFFFF {
            let bytes: Vec<u8> = indices.iter().flat_map(|&i| (i as u16).to_le_bytes()).collect();
            self.accessor(&bytes, ELEMENT_ARRAY_BUFFER, UNSIGNED_SHORT, indices.len(), "SCALAR", json!({}))
        } else {
            let bytes: Vec<u8> = indices.iter().flat_map(|&i| i.to_le_bytes()).collect();
            self.accessor(&bytes, ELEMENT_ARRAY_BUFFER, UNSIGNED_INT, indices.len(), "SCALAR", json!({}))
        };
        let material = self.material(gr);
        Some(json!({ "attributes": attrs, "indices": idx, "material": material, "mode": 4 }))
    }

    fn node(&mut self, v: Value) -> usize {
        self.nodes.push(v);
        self.nodes.len() - 1
    }

    fn add_child(&mut self, parent: usize, child: usize) {
        let p = &mut self.nodes[parent];
        if p.get("children").is_none() {
            p["children"] = json!([]);
        }
        p["children"].as_array_mut().unwrap().push(json!(child));
    }

    /// Mesh nodes for a geometry under `parent` (LOD 0, or every LOD).
    fn geometry_nodes(&mut self, b: &Blob, ts: &HashMap<u32, u32>, g: u32, name: &str, parent: usize, opts: &Options, mut node: Value) {
        let n = if opts.all_lods { u16_at(b, g + 2).unwrap_or(1).min(16) as usize } else { 1 };
        for l in 0..n {
            if let Some(m) = self.mesh(b, ts, g, l, None) {
                node["name"] = json!(if l == 0 { name.to_string() } else { format!("{name}_lod{l}") });
                node["mesh"] = json!(m);
                let id = self.node(node.clone());
                self.add_child(parent, id);
            }
        }
    }

    fn glb(mut self, root: usize) -> Result<(Vec<u8>, Report)> {
        let mut doc = json!({
            "asset": { "version": "2.0", "generator": concat!("hwvx-xfile ", env!("CARGO_PKG_VERSION")) },
            "scene": 0,
            "scenes": [{ "nodes": [root] }],
            "nodes": self.nodes,
            "buffers": [{ "byteLength": 0 }],
        });
        let mut put = |k: &str, v: Vec<Value>| {
            if !v.is_empty() {
                doc[k] = json!(v);
            }
        };
        self.report.meshes = self.meshes.len();
        self.report.textures = self.images.len();
        put("meshes", self.meshes);
        put("materials", self.materials);
        put("textures", self.gl_textures.clone());
        put("images", self.images);
        put("skins", self.skins);
        if !self.gl_textures.is_empty() {
            doc["samplers"] = json!([{ "magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497 }]);
        }
        if self.unlit_used {
            doc["extensionsUsed"] = json!(["KHR_materials_unlit"]);
        }
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        doc["buffers"][0]["byteLength"] = json!(self.bin.len());
        if !self.views.is_empty() {
            doc["bufferViews"] = json!(self.views);
            doc["accessors"] = json!(self.accessors);
        } else {
            doc.as_object_mut().unwrap().remove("buffers");
        }
        self.report.nodes = doc["nodes"].as_array().map_or(0, |n| n.len());
        let mut js = serde_json::to_vec(&doc)?;
        while js.len() % 4 != 0 {
            js.push(b' ');
        }
        let bin_chunk = if self.bin.is_empty() { 0 } else { 8 + self.bin.len() };
        let total = 12 + 8 + js.len() + bin_chunk;
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(js.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&js);
        if !self.bin.is_empty() {
            out.extend_from_slice(&(self.bin.len() as u32).to_le_bytes());
            out.extend_from_slice(b"BIN\0");
            out.extend_from_slice(&self.bin);
        }
        Ok((out, self.report))
    }
}

// ---------------------------------------------------------------------------
// Cars

/// Rest-pose bones: (rotation x,y,z,w, translation, parent).
fn skeleton(b: &Blob, skel: u32) -> Vec<([f32; 4], [f32; 3], Option<usize>)> {
    let n = b.u32(E, skel).unwrap_or(0).min(256) as usize;
    let Some(nodes) = b.ptr(E, skel + 0x10) else { return vec![] };
    let mut out = vec![];
    // stack of (bone, children left)
    let mut stack: Vec<(usize, u8)> = vec![];
    for i in 0..n {
        let o = nodes + 0x20 * i as u32;
        let (Some(q), Some(t), Some(&children)) =
            (v3_at(b, o).zip(f32_at(b, o + 0xC)), v3_at(b, o + 0x10), b.data.get(o as usize + 0x1C))
        else {
            break;
        };
        while matches!(stack.last(), Some((_, 0))) {
            stack.pop();
        }
        let parent = stack.last_mut().map(|(p, left)| {
            *left -= 1;
            *p
        });
        out.push(([q.0[0], q.0[1], q.0[2], q.1], t, parent));
        stack.push((i, children));
    }
    out
}

fn export_car(doc: &mut Doc, b: &Blob, ts: &HashMap<u32, u32>, root: usize, opts: &Options) -> Option<()> {
    let car = b.ptr(E, b.ptr(E, 4)?)?;
    let model = b.ptr(E, car + 0x78)?;
    let anim = b.ptr(E, model + 0x10)?;
    let bones = b.ptr(E, anim + 0x14).map(|s| skeleton(b, s)).unwrap_or_default();
    ensure_some(!bones.is_empty())?;

    // bones
    let base = doc.nodes.len();
    for (i, (q, t, _)) in bones.iter().enumerate() {
        doc.node(json!({ "name": format!("bone_{i}"), "rotation": q, "translation": t }));
    }
    for (i, (_, _, parent)) in bones.iter().enumerate() {
        doc.add_child(parent.map_or(root, |p| base + p), base + i);
    }
    let bone_node = |k: usize| base + k.min(bones.len() - 1);

    // rigid parts (MotionAnim.rigid: {geometry, u16 bone})
    let rigid = b.u32(E, anim + 8).unwrap_or(0).min(0x400);
    let list = b.u32(E, anim + 0xC).unwrap_or(0);
    for i in 0..rigid {
        let en = list + 8 * i;
        let (Some(g), Some(bone)) = (b.u32(E, en), u16_at(b, en + 4)) else { continue };
        doc.geometry_nodes(b, ts, g, &format!("part_{i}"), bone_node(bone as usize), opts, json!({}));
    }

    // breakable body parts (CarPartList, see CarA_DrawScript)
    if let Some(list) = b.ptr(E, anim + 0x38) {
        let n = b.u32(E, list).unwrap_or(0).min(0x100);
        if let Some(parts) = b.ptr(E, list + 4) {
            for i in 0..n {
                let c = parts + 0x1C * i;
                let (Some(&flags), Some(&xform), Some(bone), Some(g)) =
                    (b.data.get(c as usize), b.data.get(c as usize + 1), u16_at(b, c + 4), b.u32(E, c + 8))
                else {
                    continue;
                };
                let damaged = flags & 1 != 0;
                if damaged && !opts.damaged {
                    continue;
                }
                let mut node = json!({});
                if xform != 0 {
                    if let Some(off) = b.ptr(E, c + 0xC).and_then(|o| v3_at(b, o)) {
                        node["translation"] = json!(off);
                    }
                }
                let name = if damaged { format!("damaged_{i}") } else { format!("body_{i}") };
                doc.geometry_nodes(b, ts, g, &name, bone_node(bone as usize), opts, node);
            }
        }
    }

    // skinned geometry (MotionAnim.geoms), positions are bone-local:
    // identity inverse bind matrices
    let skinned = b.u32(E, anim).unwrap_or(0).min(0x40);
    if skinned > 0 {
        if let Some(geoms) = b.ptr(E, anim + 4) {
            let joints: Vec<usize> = (0..bones.len()).map(|i| base + i).collect();
            doc.skins.push(json!({ "name": "skeleton", "joints": joints, "skeleton": base }));
            let skin = doc.skins.len() - 1;
            for i in 0..skinned {
                let Some(g) = b.ptr(E, geoms + 4 * i).or_else(|| b.u32(E, geoms + 4 * i)) else { continue };
                if let Some(m) = doc.mesh(b, ts, g, 0, Some(bones.len())) {
                    let id = doc.node(json!({ "name": format!("skinned_{i}"), "mesh": m, "skin": skin }));
                    doc.add_child(root, id);
                }
            }
        }
    }
    Some(())
}

fn ensure_some(c: bool) -> Option<()> {
    c.then_some(())
}

/// Export a GameCube pack's models as a `.glb`. `is_car`: the pack is a car
/// pack (pack type 0), exported posed on its skeleton unless `opts.scan`.
pub fn export(b: &Blob, name: &str, is_car: bool, textures: &Textures, opts: &Options) -> Result<(Vec<u8>, Report)> {
    let ts: HashMap<u32, u32> = b.texture_relocs.iter().map(|r| (r.slot, r.value)).collect();
    let mut doc = Doc::new(textures);
    let root = doc.node(json!({ "name": name }));
    let mut done = false;
    if is_car && !opts.scan {
        done = export_car(&mut doc, b, &ts, root, opts).is_some();
        if !done {
            doc.warn("no car model found; exporting every geometry instead".into());
            doc.nodes.truncate(1);
            doc.nodes[0] = json!({ "name": name });
        }
    }
    if !done {
        let geoms: BTreeMap<u32, ()> =
            (0..b.data.len() as u32 / 32).map(|i| i * 32).filter(|&g| is_geometry(b, g)).map(|g| (g, ())).collect();
        for &g in geoms.keys() {
            doc.geometry_nodes(b, &ts, g, &format!("geom_{g:x}"), root, opts, json!({}));
        }
    }
    ensure!(!doc.meshes.is_empty(), "no drawable geometry found");
    doc.glb(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn png_layout() {
        let img = Image { w: 2, h: 1, rgba: vec![1, 2, 3, 4, 5, 6, 7, 8] };
        let p = png(&img);
        assert_eq!(&p[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&p[12..16], b"IHDR");
        assert_eq!(&p[p.len() - 8..p.len() - 4], b"IEND");
    }

    #[test]
    fn matrix_lookup_inverse() {
        // matrixlookup (data_gen.c): 0, 3, ... 0x39, then 0x40, 0x43, ...
        for i in 0..40usize {
            let m = if i < 20 { 3 * i } else { 0x40 + 3 * (i - 20) };
            let bone = if m < 0x3C { m / 3 } else { 20 + (m - 0x40) / 3 };
            assert_eq!(bone, i);
        }
    }
}
