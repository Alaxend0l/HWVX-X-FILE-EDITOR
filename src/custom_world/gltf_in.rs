//! A plain glTF 2.0 (`.gltf` or `.glb`) from any tool as a custom world's
//! [`Source`](super::Source), by a project file (`world.jsonc`):
//!
//! ```jsonc
//! {
//!   "id": "uid://my-world", "name": "My World", "base": 0,   // the track it is composed over
//!   "model": "model.glb",            // relative to the project file
//!   "scale": 10.0,                   // game units per model unit (glTF: metres)
//!   "rotateY": 0,                    // degrees, about the up axis, after scaling
//!   "offset": [0, 0, 0],             // game units, added last
//!   "leaveOut": ["Helper"],          // nodes (and their children) not used
//!   "renderOnly": ["sky*"],          // drawn, not solid
//!   "collisionOnly": ["col*"],       // solid, not drawn (also extras.collision: true)
//!   "materials": {                   // by the glTF material's name
//!     "Lava": { "texture": null, "colour": [255, 110, 20, 255], "surface": 0 },
//!     "*":    { "texture": 4 }       // the rest
//!   },
//!   "start": [0, 1, 0, 0]            // model units (x, y, z) and a heading in degrees
//! }
//! ```
//!
//! Each mesh node is one geometry, each primitive one group: its triangles
//! with the node's world transform, UVs (`TEXCOORD_0`), vertex colours
//! (`COLOR_0`) times the material's base colour factor times the project's
//! `colour`. `texture` is an index into the base level's world texture pack
//! (none: vertex colour only); `surface` an index into the base collision
//! pack's materials. Collision is the solid nodes' triangles, welded and with
//! degenerate ones dropped, in file order.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;

use super::{ColTri, Mesh, Part, Source, Tri};

type M4 = [[f64; 4]; 4];

const IDENT: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

fn mul(a: &M4, b: &M4) -> M4 {
    let mut r = [[0.0; 4]; 4];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

fn apply(m: &M4, p: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3])
}

fn det3(m: &M4) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

fn nums(v: Option<&Value>, n: usize) -> Option<Vec<f64>> {
    let a = v?.as_array()?;
    (a.len() == n).then(|| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())
}

/// A node's local matrix (row-major, column vectors).
fn local(node: &Value) -> M4 {
    if let Some(m) = nums(node.get("matrix"), 16) {
        // glTF matrices are column-major
        return std::array::from_fn(|i| std::array::from_fn(|j| m[j * 4 + i]));
    }
    let t = nums(node.get("translation"), 3).unwrap_or(vec![0.0; 3]);
    let r = nums(node.get("rotation"), 4).unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
    let s = nums(node.get("scale"), 3).unwrap_or(vec![1.0; 3]);
    let (x, y, z, w) = (r[0], r[1], r[2], r[3]);
    let rot = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    let mut m = IDENT;
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = rot[i][j] * s[j];
        }
        m[i][3] = t[i];
    }
    m
}

fn base64(s: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => bail!("not base64"),
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// Base64 (standard alphabet, padded): for writing samples with embedded buffers.
pub fn base64_encode(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                s.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// A loaded glTF: its JSON and buffers.
pub struct Gltf {
    pub json: Value,
    pub buffers: Vec<Vec<u8>>,
}

impl Gltf {
    pub fn open(path: &Path) -> Result<Gltf> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Gltf::parse(&bytes, path.parent())
    }

    pub fn parse(bytes: &[u8], dir: Option<&Path>) -> Result<Gltf> {
        let (json, bin): (Value, Option<Vec<u8>>) = if bytes.starts_with(b"glTF") {
            let u = |o: usize| -> Result<u32> {
                Ok(u32::from_le_bytes(bytes.get(o..o + 4).context("a truncated .glb")?.try_into().unwrap()))
            };
            ensure!(u(4)? == 2, "glTF binary version {}, not 2", u(4)?);
            let (mut at, mut json, mut bin) = (12usize, None, None);
            while at + 8 <= bytes.len() {
                let (n, kind) = (u(at)? as usize, u(at + 4)?);
                let chunk = bytes.get(at + 8..at + 8 + n).context("a truncated .glb chunk")?;
                match kind {
                    0x4E4F534A => json = Some(serde_json::from_slice(chunk).context("the .glb's JSON")?),
                    0x004E4942 => bin = Some(chunk.to_vec()),
                    _ => {}
                }
                at += 8 + n;
            }
            (json.context("a .glb without JSON")?, bin)
        } else {
            (serde_json::from_slice(bytes).context("not glTF JSON")?, None)
        };
        let mut buffers = vec![];
        for (i, b) in json.get("buffers").and_then(Value::as_array).cloned().unwrap_or_default().iter().enumerate() {
            let data = match b.get("uri").and_then(Value::as_str) {
                None => bin.clone().with_context(|| format!("buffer {i}: no uri and no .glb BIN chunk"))?,
                Some(u) if u.starts_with("data:") => base64(u.split_once(',').map(|x| x.1).unwrap_or(""))?,
                Some(u) => {
                    let p: PathBuf = dir.unwrap_or(Path::new(".")).join(u);
                    std::fs::read(&p).with_context(|| format!("buffer {i}: {}", p.display()))?
                }
            };
            buffers.push(data);
        }
        Ok(Gltf { json, buffers })
    }

    /// An accessor's elements as f64 (normalized integers scaled to 0..1).
    pub fn accessor(&self, i: usize) -> Result<Vec<Vec<f64>>> {
        let a = &self.json["accessors"][i];
        ensure!(a.get("sparse").is_none(), "accessor {i}: sparse accessors are not read");
        let count = a["count"].as_u64().context("accessor count")? as usize;
        let n = match a["type"].as_str().unwrap_or("") {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            t => bail!("accessor {i}: type {t}"),
        };
        let ct = a["componentType"].as_u64().unwrap_or(0);
        let size = match ct {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => bail!("accessor {i}: component type {ct}"),
        };
        let norm = a.get("normalized").and_then(Value::as_bool).unwrap_or(false);
        let Some(bv) = a.get("bufferView").and_then(Value::as_u64) else {
            return Ok(vec![vec![0.0; n]; count]);
        };
        let v = &self.json["bufferViews"][bv as usize];
        let buf = self.buffers.get(v["buffer"].as_u64().unwrap_or(0) as usize).context("bufferView's buffer")?;
        let base = v.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize
            + a.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let stride = v.get("byteStride").and_then(Value::as_u64).map(|s| s as usize).unwrap_or(n * size);
        let mut out = Vec::with_capacity(count);
        for e in 0..count {
            let mut el = Vec::with_capacity(n);
            for c in 0..n {
                let o = base + e * stride + c * size;
                let s = buf.get(o..o + size).with_context(|| format!("accessor {i} runs past its buffer"))?;
                let x = match ct {
                    5126 => f32::from_le_bytes(s.try_into().unwrap()) as f64,
                    5125 => u32::from_le_bytes(s.try_into().unwrap()) as f64,
                    5123 => {
                        let v = u16::from_le_bytes(s.try_into().unwrap()) as f64;
                        if norm { v / 65535.0 } else { v }
                    }
                    5122 => {
                        let v = i16::from_le_bytes(s.try_into().unwrap()) as f64;
                        if norm { (v / 32767.0).max(-1.0) } else { v }
                    }
                    5121 => {
                        let v = s[0] as f64;
                        if norm { v / 255.0 } else { v }
                    }
                    _ => {
                        let v = s[0] as i8 as f64;
                        if norm { (v / 127.0).max(-1.0) } else { v }
                    }
                };
                el.push(x);
            }
            out.push(el);
        }
        Ok(out)
    }

    /// Every mesh node with its world matrix and name path, in scene order
    /// (depth first), skipping the subtrees `skip` says to.
    pub fn mesh_nodes(&self, skip: &dyn Fn(&str) -> bool) -> Vec<(usize, M4, String)> {
        let nodes = self.json["nodes"].as_array().cloned().unwrap_or_default();
        let scene = self.json.get("scene").and_then(Value::as_u64).unwrap_or(0) as usize;
        let roots: Vec<usize> = self.json["scenes"][scene]["nodes"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as usize).collect())
            .unwrap_or_else(|| (0..nodes.len()).collect());
        let mut out = vec![];
        fn walk(nodes: &[Value], i: usize, parent: &M4, skip: &dyn Fn(&str) -> bool, out: &mut Vec<(usize, M4, String)>, depth: usize) {
            let Some(n) = nodes.get(i) else { return };
            let name = n.get("name").and_then(Value::as_str).unwrap_or("");
            if skip(name) || depth > 64 {
                return;
            }
            let m = mul(parent, &local(n));
            if n.get("mesh").is_some() {
                out.push((i, m, name.to_string()));
            }
            for c in n.get("children").and_then(Value::as_array).cloned().unwrap_or_default() {
                if let Some(c) = c.as_u64() {
                    walk(nodes, c as usize, &m, skip, out, depth + 1);
                }
            }
        }
        for r in roots {
            walk(&nodes, r, &IDENT, skip, &mut out, 0);
        }
        out
    }
}

/// A name pattern: exact, or with one `*` anywhere (case-insensitive).
pub fn matches(pat: &str, name: &str) -> bool {
    let (p, n) = (pat.to_ascii_lowercase(), name.to_ascii_lowercase());
    match p.split_once('*') {
        None => p == n,
        Some((a, b)) => n.len() >= a.len() + b.len() && n.starts_with(a) && n.ends_with(b),
    }
}

fn any(list: &[String], name: &str) -> bool {
    list.iter().any(|p| matches(p, name))
}

/// How a material is drawn and what it is to drive on.
#[derive(Clone, Debug, PartialEq)]
pub struct MatRule {
    pub texture: Option<u32>,
    pub colour: [f64; 4],
    pub surface: u32,
}

/// A project file, read.
#[derive(Clone, Debug)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub base: u32,
    pub model: PathBuf,
    pub scale: f64,
    pub rotate_y: f64,
    pub offset: [f64; 3],
    pub leave_out: Vec<String>,
    pub render_only: Vec<String>,
    pub collision_only: Vec<String>,
    pub materials: Vec<(String, MatRule)>,
    pub start: [f64; 4],
    /// Put the start on the ground below it (the first solid going down), this
    /// many game units above it: `"startToGround": 5`.
    pub start_to_ground: Option<f64>,
}

fn strip_comments(s: &str) -> String {
    // JSONC: // and /* */ comments outside strings, trailing commas
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let (mut i, mut in_str) = (0, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c as char);
            if c == b'\\' && i + 1 < b.len() {
                out.push(b[i + 1] as char);
                i += 1;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
            out.push('"');
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        } else if c == b',' {
            let mut j = i + 1;
            while j < b.len() && (b[j] as char).is_whitespace() {
                j += 1;
            }
            if !(j < b.len() && (b[j] == b'}' || b[j] == b']')) {
                out.push(',');
            }
        } else {
            out.push(c as char);
        }
        i += 1;
    }
    out
}

/// Reads a JSONC file (comments, trailing commas).
pub fn read_jsonc(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&strip_comments(&text)).with_context(|| format!("{}: not JSON(C)", path.display()))
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

impl Project {
    pub fn read(path: &Path) -> Result<Project> {
        let v = read_jsonc(path)?;
        let dir = path.parent().unwrap_or(Path::new("."));
        let model = v.get("model").and_then(Value::as_str).context("the project names no \"model\"")?;
        let mut materials = vec![];
        if let Some(m) = v.get("materials").and_then(Value::as_object) {
            for (k, r) in m {
                let c = nums(r.get("colour"), 4).or_else(|| nums(r.get("colour"), 3).map(|mut c| {
                    c.push(255.0);
                    c
                }));
                materials.push((
                    k.clone(),
                    MatRule {
                        texture: r.get("texture").and_then(Value::as_u64).map(|x| x as u32),
                        colour: c.map(|c| [c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, c[3] / 255.0]).unwrap_or([1.0; 4]),
                        surface: r.get("surface").and_then(Value::as_u64).unwrap_or(0) as u32,
                    },
                ));
            }
        }
        let start = nums(v.get("start"), 4).unwrap_or(vec![0.0; 4]);
        Ok(Project {
            id: v.get("id").and_then(Value::as_str).unwrap_or("uid://custom-world").into(),
            name: v.get("name").and_then(Value::as_str).unwrap_or("Custom World").into(),
            base: v.get("base").and_then(Value::as_u64).unwrap_or(0) as u32,
            model: dir.join(model),
            scale: v.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
            rotate_y: v.get("rotateY").and_then(Value::as_f64).unwrap_or(0.0),
            offset: nums(v.get("offset"), 3).map(|o| [o[0], o[1], o[2]]).unwrap_or([0.0; 3]),
            leave_out: strings(&v, "leaveOut"),
            render_only: strings(&v, "renderOnly"),
            collision_only: strings(&v, "collisionOnly"),
            materials,
            start: [start[0], start[1], start[2], start[3]],
            start_to_ground: v.get("startToGround").and_then(Value::as_f64),
        })
    }

    fn rule(&self, name: &str) -> MatRule {
        self.materials
            .iter()
            .find(|(k, _)| k != "*" && matches(k, name))
            .or_else(|| self.materials.iter().find(|(k, _)| k == "*"))
            .map(|(_, r)| r.clone())
            .unwrap_or(MatRule { texture: None, colour: [1.0; 4], surface: 0 })
    }

    /// Model coordinates to game units: scale, the turn about the up axis, the offset.
    pub fn place(&self) -> M4 {
        let (s, c) = self.rotate_y.to_radians().sin_cos();
        let k = self.scale;
        [[c * k, 0.0, s * k, self.offset[0]], [0.0, k, 0.0, self.offset[1]], [-s * k, 0.0, c * k, self.offset[2]], [0.0, 0.0, 0.0, 1.0]]
    }

    /// The start in game units (the heading turned with the model).
    pub fn start_game(&self) -> [f32; 4] {
        let p = apply(&self.place(), [self.start[0], self.start[1], self.start[2]]);
        [p[0] as f32, p[1] as f32, p[2] as f32, (self.start[3] + self.rotate_y) as f32]
    }
}

/// What the import found, for the report.
#[derive(Debug, Default)]
pub struct ImportReport {
    pub nodes: Vec<String>,
    pub left_out: usize,
    pub degenerate: usize,
    pub bounds: Option<([f64; 3], [f64; 3])>,
    pub materials: Vec<(String, usize)>,
}

/// The project's model as a [`Source`].
pub fn import(p: &Project) -> Result<(Source, ImportReport)> {
    let g = Gltf::open(&p.model)?;
    let mut rep = ImportReport::default();
    let all = g.mesh_nodes(&|_| false).len();
    let nodes = g.mesh_nodes(&|n| any(&p.leave_out, n));
    rep.left_out = all - nodes.len();
    let place = p.place();
    let mats = g.json.get("materials").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut meshes = vec![];
    let mut collision = vec![];
    let mut used: HashMap<String, usize> = HashMap::new();
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for (ni, m, name) in nodes {
        let node = &g.json["nodes"][ni];
        let mi = node["mesh"].as_u64().unwrap() as usize;
        let mesh = &g.json["meshes"][mi];
        let col_flag = node.pointer("/extras/collision").and_then(Value::as_bool).or_else(|| mesh.pointer("/extras/collision").and_then(Value::as_bool));
        let col_only = col_flag == Some(true) || any(&p.collision_only, &name);
        let render_only = col_flag == Some(false) || any(&p.render_only, &name);
        let world = mul(&place, &m);
        // a mirroring transform turns the winding over
        let flip = det3(&world) < 0.0;
        let mut parts = vec![];
        for (k, pr) in mesh["primitives"].as_array().cloned().unwrap_or_default().iter().enumerate() {
            if pr.get("mode").and_then(Value::as_u64).unwrap_or(4) != 4 {
                continue;
            }
            let at = &pr["attributes"];
            let pos = g.accessor(at["POSITION"].as_u64().context("a primitive without POSITION")? as usize)?;
            let n = pos.len();
            let uv = match at.get("TEXCOORD_0").and_then(Value::as_u64) {
                Some(i) => g.accessor(i as usize)?,
                None => vec![vec![0.0, 0.0]; n],
            };
            let col = match at.get("COLOR_0").and_then(Value::as_u64) {
                Some(i) => g.accessor(i as usize)?,
                None => vec![vec![1.0; 4]; n],
            };
            let idx: Vec<usize> = match pr.get("indices").and_then(Value::as_u64) {
                Some(i) => g.accessor(i as usize)?.iter().map(|x| x[0] as usize).collect(),
                None => (0..n).collect(),
            };
            let mat = pr.get("material").and_then(Value::as_u64).map(|i| &mats[i as usize]);
            let mname = mat.and_then(|m| m.get("name")).and_then(Value::as_str).unwrap_or("").to_string();
            let factor = mat
                .and_then(|m| nums(m.pointer("/pbrMetallicRoughness/baseColorFactor"), 4))
                .unwrap_or(vec![1.0; 4]);
            let rule = p.rule(&mname);
            let wpos: Vec<[f64; 3]> = pos.iter().map(|v| apply(&world, [v[0], v[1], v[2]])).collect();
            let mut tris = vec![];
            for t in idx.chunks_exact(3) {
                ensure!(t.iter().all(|&i| i < n), "{name}: an index past the vertices");
                let o = if flip { [t[0], t[2], t[1]] } else { [t[0], t[1], t[2]] };
                let pts = o.map(|i| wpos[i]);
                if super::terrain::plane_of(&pts).is_none() {
                    rep.degenerate += 1;
                    continue;
                }
                for q in &pts {
                    for a in 0..3 {
                        lo[a] = lo[a].min(q[a]);
                        hi[a] = hi[a].max(q[a]);
                    }
                }
                if !render_only {
                    collision.push(ColTri { p: pts, material: rule.surface, object: ni as u32 });
                }
                if !col_only {
                    let c = |i: usize| -> [u8; 4] {
                        let v = &col[i];
                        let a = if v.len() > 3 { v[3] } else { 1.0 };
                        let ch = [v[0], v[1], v[2], a];
                        std::array::from_fn(|k| (ch[k] * factor[k] * rule.colour[k] * 255.0).round().clamp(0.0, 255.0) as u8)
                    };
                    tris.push(Tri {
                        p: pts.map(|q| q.map(|x| x as f32)),
                        uv: o.map(|i| [uv[i][0] as f32, uv[i][1] as f32]),
                        col: o.map(c),
                    });
                }
            }
            *used.entry(mname.clone()).or_default() += tris.len();
            // one draw holds at most 21845 triangles
            for (j, chunk) in tris.chunks(20000).enumerate() {
                let nm = if mname.is_empty() { format!("{name}_{k}_{j}") } else { mname.clone() };
                parts.push(Part { name: nm, texture: rule.texture, tris: chunk.to_vec() });
            }
        }
        if !parts.is_empty() {
            rep.nodes.push(name.clone());
            meshes.push(Mesh { name, parts });
        }
    }
    ensure!(!meshes.is_empty(), "{}: no triangles to draw", p.model.display());
    ensure!(!collision.is_empty(), "{}: no solid triangles", p.model.display());
    rep.bounds = Some((lo, hi));
    let mut u: Vec<(String, usize)> = used.into_iter().collect();
    u.sort();
    rep.materials = u;
    Ok((Source { meshes, collision, start: p.start_game() }, rep))
}

/// The demo world as a plain glTF (one node per mesh, materials named after
/// the parts, an embedded buffer): the sample a project reads back.
pub fn write_gltf(src: &Source) -> Value {
    let mut bin: Vec<u8> = vec![];
    let (mut views, mut accs, mut meshes, mut nodes, mut mats) = (vec![], vec![], vec![], vec![], vec![]);
    let mut mat_ix: HashMap<String, usize> = HashMap::new();
    let mut add = |bin: &mut Vec<u8>, data: Vec<u8>, count: usize, ty: &str, ct: u32, minmax: Option<([f32; 3], [f32; 3])>| -> usize {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        views.push(serde_json::json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": data.len()}));
        bin.extend(data);
        let mut a = serde_json::json!({"bufferView": views.len() - 1, "componentType": ct, "count": count, "type": ty});
        if let Some((lo, hi)) = minmax {
            a["min"] = serde_json::json!(lo);
            a["max"] = serde_json::json!(hi);
        }
        if ct == 5121 {
            a["normalized"] = Value::Bool(true);
        }
        accs.push(a);
        accs.len() - 1
    };
    for m in &src.meshes {
        let mut prims = vec![];
        for part in &m.parts {
            let (mut pos, mut uv, mut col) = (vec![], vec![], vec![]);
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for t in &part.tris {
                for i in 0..3 {
                    for k in 0..3 {
                        pos.extend(t.p[i][k].to_le_bytes());
                        lo[k] = lo[k].min(t.p[i][k]);
                        hi[k] = hi[k].max(t.p[i][k]);
                    }
                    uv.extend(t.uv[i][0].to_le_bytes());
                    uv.extend(t.uv[i][1].to_le_bytes());
                    col.extend(t.col[i]);
                }
            }
            let n = part.tris.len() * 3;
            let pa = add(&mut bin, pos, n, "VEC3", 5126, Some((lo, hi)));
            let ua = add(&mut bin, uv, n, "VEC2", 5126, None);
            let ca = add(&mut bin, col, n, "VEC4", 5121, None);
            let mi = *mat_ix.entry(part.name.clone()).or_insert_with(|| {
                mats.push(serde_json::json!({"name": part.name}));
                mats.len() - 1
            });
            prims.push(serde_json::json!({"attributes": {"POSITION": pa, "TEXCOORD_0": ua, "COLOR_0": ca}, "material": mi, "mode": 4}));
        }
        meshes.push(serde_json::json!({"name": m.name, "primitives": prims}));
        nodes.push(serde_json::json!({"name": m.name, "mesh": meshes.len() - 1}));
    }
    serde_json::json!({
        "asset": {"version": "2.0", "generator": "hwvx-x world demo --gltf"},
        "scene": 0,
        "scenes": [{"nodes": (0..nodes.len()).collect::<Vec<_>>()}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": mats,
        "accessors": accs,
        "bufferViews": views,
        "buffers": [{"byteLength": bin.len(), "uri": format!("data:application/octet-stream;base64,{}", base64_encode(&bin))}],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip() {
        for n in 0..10 {
            let b: Vec<u8> = (0..n).map(|x| (x * 37 + 5) as u8).collect();
            assert_eq!(base64(&base64_encode(&b)).unwrap(), b);
        }
    }

    #[test]
    fn jsonc_comments() {
        let v: Value = serde_json::from_str(&strip_comments("{ // a\n \"a\": \"x//y\", /* b */ \"b\": [1, 2,], }")).unwrap();
        assert_eq!(v["a"], "x//y");
        assert_eq!(v["b"][1], 2);
    }

    #[test]
    fn patterns() {
        assert!(matches("col*", "Collision_1"));
        assert!(matches("*Boundry", "CageBoundry"));
        assert!(!matches("sun plane", "sun planes"));
    }
}
