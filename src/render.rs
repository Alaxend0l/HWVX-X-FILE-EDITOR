//! A small software renderer for GameCube car packs: poses the car's model
//! parts on its skeleton (rest pose), textures them with the pack's own
//! textures and draws a 3/4 view on a transparent background. Used to make
//! car-select thumbnails and to preview converted cars.

use anyhow::{Context, Result};

use crate::blob::Blob;
use crate::convert::texture::{self, Image};
use crate::format::Endian;


type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// Rotation (quaternion x,y,z,w) + translation.
#[derive(Clone, Copy)]
struct Xf {
    q: [f32; 4],
    t: V3,
}

impl Xf {
    fn rot(&self, v: V3) -> V3 {
        let [x, y, z, w] = self.q;
        let u = [x, y, z];
        let t = cross(u, v);
        let t = [2.0 * t[0], 2.0 * t[1], 2.0 * t[2]];
        let c = cross(u, t);
        [v[0] + w * t[0] + c[0], v[1] + w * t[1] + c[1], v[2] + w * t[2] + c[2]]
    }
    fn apply(&self, v: V3) -> V3 {
        let r = self.rot(v);
        [r[0] + self.t[0], r[1] + self.t[1], r[2] + self.t[2]]
    }
    /// self ∘ child
    fn then(&self, child: &Xf) -> Xf {
        let [ax, ay, az, aw] = self.q;
        let [bx, by, bz, bw] = child.q;
        Xf {
            q: [
                aw * bx + ax * bw + ay * bz - az * by,
                aw * by - ax * bz + ay * bw + az * bx,
                aw * bz + ax * by - ay * bx + az * bw,
                aw * bw - ax * bx - ay * by - az * bz,
            ],
            t: self.apply(child.t),
        }
    }
}

struct Tri {
    p: [V3; 3],
    uv: [[f32; 2]; 3],
    tex: Option<usize>,
    /// flat colour when untextured (env-mapped chrome etc.)
    colour: [u8; 4],
    alpha: bool,
}

fn f32_at(b: &Blob, e: Endian, off: u32) -> f32 {
    f32::from_bits(b.u32(e, off).unwrap_or(0))
}

/// Rest-pose world transforms of a skeleton (pre-order, child counts).
fn bone_transforms(b: &Blob, e: Endian, skel: u32) -> Vec<Xf> {
    let n = b.u32(e, skel).unwrap_or(0).min(256) as usize;
    let Some(bones) = b.ptr(e, skel + 0x10) else { return vec![] };
    let mut out = Vec::with_capacity(n);
    // stack of (parent world transform, children left)
    let mut stack: Vec<(Xf, u8)> = vec![];
    for i in 0..n {
        let o = bones + 0x20 * i as u32;
        let local = Xf {
            q: [f32_at(b, e, o), f32_at(b, e, o + 4), f32_at(b, e, o + 8), f32_at(b, e, o + 0xC)],
            t: [f32_at(b, e, o + 0x10), f32_at(b, e, o + 0x14), f32_at(b, e, o + 0x18)],
        };
        while matches!(stack.last(), Some((_, 0))) {
            stack.pop();
        }
        let world = match stack.last_mut() {
            Some((parent, left)) => {
                *left -= 1;
                parent.then(&local)
            }
            None => local,
        };
        out.push(world);
        let children = b.data[(o + 0x1C) as usize];
        stack.push((world, children));
    }
    out
}

fn rd16(e: Endian, b: &[u8], o: u32) -> u16 {
    let x = [b[o as usize], b[o as usize + 1]];
    if e == Endian::Big {
        u16::from_be_bytes(x)
    } else {
        u16::from_le_bytes(x)
    }
}

/// Triangles of LOD 0 of a GameCube or PC geometry, transformed by `xf`.
fn geometry_tris(b: &Blob, e: Endian, g: u32, xf: &Xf, out: &mut Vec<Tri>) {
    use crate::convert::geometry::{GC_MAGIC, PC_MAGIC};
    let half = |o: u32| rd16(e, &b.data, o);
    // half(g + 4) != 0: skinned geometry (not drawn by the preview)
    if b.data.len() < (g + 0x40) as usize || half(g + 4) != 0 {
        return;
    }
    let pc = match half(g) {
        GC_MAGIC => false,
        PC_MAGIC => true,
        _ => return,
    };
    let Some(lod) = b.ptr(e, g + 8) else { return };
    let (Some(groups), Some(vb)) = (b.ptr(e, lod + 8), b.ptr(e, lod + 0xC)) else { return };
    let ng = half(lod + 2) as u32;
    let tex_slots: std::collections::HashMap<u32, u32> = b.texture_relocs.iter().map(|r| (r.slot, r.value)).collect();
    // vertex fetch: (position index, uv index) -> position, uv
    let (pos_at, pos_stride, uv_at) = if pc {
        (b.u32(e, vb).unwrap_or(0), 16, b.u32(e, vb + 8).unwrap_or(0))
    } else {
        (vb + 0x10, 12, vb + 0x10 + b.u32(e, vb).unwrap_or(0))
    };
    for k in 0..ng {
        let gr = groups + 0x20 * k;
        let flags = half(gr + 2);
        if flags & crate::convert::geometry::F_SHADOW != 0 {
            continue;
        }
        let Some(dl) = b.ptr(e, gr + 0x1C) else { continue };
        // (position, uv) index pairs of each vertex
        let verts: Vec<(u32, u32)> = if pc {
            let n = b.u32(e, gr + 0x10).unwrap_or(0) * 3;
            (0..n).map(|v| {
                let i = half(dl + 2 * v) as u32;
                (i, i)
            }).collect()
        } else {
            if b.data.get(dl as usize) != Some(&0x90) {
                continue;
            }
            let n = half(dl + 1) as u32;
            (0..n).map(|v| (half(dl + 3 + v * 6) as u32, half(dl + 3 + v * 6 + 4) as u32)).collect()
        };
        let tex = tex_slots.get(&(gr + 0x18)).filter(|v| *v >> 16 == 0).map(|v| (*v & 0xFFFF) as usize);
        let colour = if flags & 2 != 0 { [170, 175, 185, 255] } else { [150, 150, 150, 255] };
        let alpha = flags & 1 != 0;
        for t in verts.chunks_exact(3) {
            let mut p = [[0.0; 3]; 3];
            let mut uv = [[0.0; 2]; 3];
            for (j, &(pi, ti)) in t.iter().enumerate() {
                let pp = pos_at + pi * pos_stride;
                p[j] = xf.apply([f32_at(b, e, pp), f32_at(b, e, pp + 4), f32_at(b, e, pp + 8)]);
                let tt = uv_at + ti * 8;
                uv[j] = [f32_at(b, e, tt), f32_at(b, e, tt + 4)];
            }
            out.push(Tri { p, uv, tex, colour, alpha });
        }
    }
}

/// Render the car of a GameCube or PC car pack. `textures`: the pack's own
/// textures, decoded. Returns a `size`×`size` RGBA image (transparent
/// background). `yaw`/`pitch` in degrees: 0/0 looks at the car's front.
pub fn render_car(b: &Blob, e: Endian, textures: &[Image], size: usize, yaw: f32, pitch: f32) -> Result<Image> {
    let car = b.ptr(e, b.ptr(e, 4).context("no root table")?).context("no car")?;
    let model = b.ptr(e, car + 0x78).context("car has no model")?;
    let anim = b.ptr(e, model + 0x10).context("car has no motion animation")?;
    let bones = b.ptr(e, anim + 0x14).map(|s| bone_transforms(b, e, s)).unwrap_or_default();
    let parts = b.u32(e, anim + 8).unwrap_or(0).min(0x400);
    let list = b.u32(e, anim + 0xC).context("no model parts")?;
    let ident = Xf { q: [0.0, 0.0, 0.0, 1.0], t: [0.0; 3] };
    let mut tris = vec![];
    for i in 0..parts {
        let en = list + 8 * i;
        let g = b.u32(e, en).unwrap_or(0);
        let bone = rd16(e, &b.data, en + 4) as usize;
        geometry_tris(b, e, g, bones.get(bone).unwrap_or(&ident), &mut tris);
    }
    // Breakable body parts (CarPartList, CarA_DrawScript): undamaged ones
    // have flag bit 0 clear; `xform` shifts the part by its offset vector.
    if let Some(list) = b.ptr(e, anim + 0x38) {
        let n = b.u32(e, list).unwrap_or(0).min(0x100);
        if let Some(parts) = b.ptr(e, list + 4) {
            for i in 0..n {
                let c = parts + 0x1C * i;
                let (flags, xform) = (b.data[c as usize], b.data[c as usize + 1]);
                if flags & 1 != 0 {
                    continue;
                }
                let bone = rd16(e, &b.data, c + 4) as usize;
                let mut xf = *bones.get(bone).unwrap_or(&ident);
                if xform != 0 {
                    if let Some(o) = b.ptr(e, c + 0xC) {
                        xf.t = xf.apply([f32_at(b, e, o), f32_at(b, e, o + 4), f32_at(b, e, o + 8)]);
                    }
                }
                let g = b.u32(e, c + 8).unwrap_or(0);
                geometry_tris(b, e, g, &xf, &mut tris);
            }
        }
    }

    // camera: orthographic 3/4 view
    let (ys, yc) = yaw.to_radians().sin_cos();
    let (ps, pc) = pitch.to_radians().sin_cos();
    let fwd = norm([-ys * pc, -ps, -yc * pc]); // view direction (towards the car)
    let right = norm(cross(fwd, [0.0, 1.0, 0.0]));
    let up = cross(right, fwd);
    let view = |p: V3| [dot(p, right), dot(p, up), dot(p, fwd)];
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for t in &tris {
        for p in t.p {
            let v = view(p);
            for k in 0..2 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
    }
    let ss = 3usize; // supersampling
    let big = size * ss;
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-3) * 1.08;
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let to_px = |v: V3| -> [f32; 3] {
        [((v[0] - centre[0]) / span + 0.5) * big as f32, ((centre[1] - v[1]) / span + 0.5) * big as f32, v[2]]
    };
    let light = norm([-0.4, 0.8, 0.45]);
    let mut colour = vec![[0f32; 4]; big * big];
    let mut depth = vec![f32::MAX; big * big];
    for t in &tris {
        let n = norm(cross(sub(t.p[1], t.p[0]), sub(t.p[2], t.p[0])));
        // two-sided lighting (winding is not needed for a preview)
        let shade = 0.45 + 0.55 * dot(n, light).abs();
        let s: Vec<[f32; 3]> = t.p.iter().map(|&p| to_px(view(p))).collect();
        let area = (s[1][0] - s[0][0]) * (s[2][1] - s[0][1]) - (s[2][0] - s[0][0]) * (s[1][1] - s[0][1]);
        if area.abs() < 1e-6 {
            continue;
        }
        let (x0, x1) = (s.iter().map(|v| v[0]).fold(f32::MAX, f32::min).max(0.0) as usize, (s.iter().map(|v| v[0]).fold(f32::MIN, f32::max).ceil() as usize).min(big - 1));
        let (y0, y1) = (s.iter().map(|v| v[1]).fold(f32::MAX, f32::min).max(0.0) as usize, (s.iter().map(|v| v[1]).fold(f32::MIN, f32::max).ceil() as usize).min(big - 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((s[1][0] - px) * (s[2][1] - py) - (s[2][0] - px) * (s[1][1] - py)) / area;
                let w1 = ((s[2][0] - px) * (s[0][1] - py) - (s[0][0] - px) * (s[2][1] - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * s[0][2] + w1 * s[1][2] + w2 * s[2][2];
                let i = y * big + x;
                if z >= depth[i] {
                    continue;
                }
                let mut c = [t.colour[0] as f32, t.colour[1] as f32, t.colour[2] as f32, 255.0];
                if let Some(img) = t.tex.and_then(|k| textures.get(k)) {
                    let u = w0 * t.uv[0][0] + w1 * t.uv[1][0] + w2 * t.uv[2][0];
                    let v = w0 * t.uv[0][1] + w1 * t.uv[1][1] + w2 * t.uv[2][1];
                    let tx = ((u.rem_euclid(1.0)) * img.w as f32) as usize % img.w;
                    let ty = ((v.rem_euclid(1.0)) * img.h as f32) as usize % img.h;
                    let p = img.px(tx, ty);
                    c = [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32];
                    if c[3] < 128.0 && !t.alpha {
                        continue; // cut-out
                    }
                }
                if t.alpha {
                    // translucent (glass): blend over what is there, keep depth
                    let a = (c[3] / 255.0).clamp(0.25, 0.8);
                    let d = &mut colour[i];
                    for k in 0..3 {
                        d[k] = d[k] * (1.0 - a) + c[k] * shade * a;
                    }
                    d[3] = d[3].max(a * 255.0);
                    continue;
                }
                depth[i] = z;
                colour[i] = [c[0] * shade, c[1] * shade, c[2] * shade, 255.0];
            }
        }
    }
    // downsample
    let mut rgba = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for dy in 0..ss {
                for dx in 0..ss {
                    let c = colour[(y * ss + dy) * big + x * ss + dx];
                    for k in 0..3 {
                        acc[k] += c[k] * c[3] / 255.0;
                    }
                    acc[3] += c[3];
                }
            }
            let n = (ss * ss) as f32;
            let a = acc[3] / n;
            for k in 0..3 {
                rgba.push(if a > 0.0 { (acc[k] / n * 255.0 / a).clamp(0.0, 255.0) as u8 } else { 0 });
            }
            rgba.push(a.clamp(0.0, 255.0) as u8);
        }
    }
    Ok(Image { w: size, h: size, rgba })
}

/// Decode a GameCube pack's own textures (level 0).
pub fn pack_textures(b: &Blob, platform: crate::format::Platform, num: u32, offset: u32) -> Result<Vec<Image>> {
    Ok(texture::decode_pack_textures(platform, &b.data, num, offset, 0)?.into_iter().map(|(_, i)| i).collect())
}
