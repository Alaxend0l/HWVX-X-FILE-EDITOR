//! Textures: decode PS2-prototype texture records, encode GameCube ones.
//!
//! GameCube record (one per texture, a 0x20-byte slot in the table at the
//! pack's `texture_data_offset`):
//!
//! ```text
//! 0x0 u16 LE format (1 = RGBA8, 4 = CMPR)   0x2 u16 LE width
//! 0x4 u16 LE height                          0x6 u16 LE mip levels (0 = none)
//! 0x8 u32 BE pixel pointer (32-aligned)      0xC u32 0, then 16 zero bytes
//! ```
//!
//! Prototype record (0x10 bytes, packed, little-endian): format (0 RGB24,
//! 1 RGBA32, 2 8-bit indexed), width, height, mip value m (m-1 levels
//! stored), pixel pointer, palette index. Palettes are 256 RGBA entries in
//! the PS2 CSM1 order (index bits 3 and 4 swapped); pixels are linear.

use anyhow::{bail, ensure, Context, Result};

pub const GC_FMT_RGBA8: u16 = 1;
pub const GC_FMT_CMPR: u16 = 4;
pub const GC_HEADER_SLOT: usize = 0x20;
pub const BETA_HEADER_LEN: usize = 0x10;
pub const PALETTE_LEN: usize = 0x400;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    /// RGBA8888, row-major.
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn px(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.w + x) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    /// Pixel with coordinates clamped to the image (for padding to tiles).
    fn px_clamped(&self, x: usize, y: usize) -> [u8; 4] {
        self.px(x.min(self.w - 1), y.min(self.h - 1))
    }

    /// 2x2 box filter (alpha-weighted colour), for mip levels.
    pub fn half(&self) -> Image {
        let (w, h) = ((self.w / 2).max(1), (self.h / 2).max(1));
        let mut rgba = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 4];
                let mut wsum = 0u32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = self.px_clamped(x * 2 + dx, y * 2 + dy);
                    let a = p[3] as u32;
                    for c in 0..3 {
                        sum[c] += p[c] as u32 * a;
                    }
                    sum[3] += a;
                    wsum += a;
                }
                for c in 0..3 {
                    rgba.push(if wsum == 0 { 0 } else { ((sum[c] + wsum / 2) / wsum) as u8 });
                }
                rgba.push(((sum[3] + 2) / 4) as u8);
            }
        }
        Image { w, h, rgba }
    }

    /// Fraction of pixels whose alpha is neither (nearly) 0 nor 255.
    pub fn partial_alpha(&self) -> f64 {
        let n = self.w * self.h;
        let partial = self.rgba.chunks_exact(4).filter(|p| p[3] > 8 && p[3] < 247).count();
        partial as f64 / n.max(1) as f64
    }
}

/// PS2 CSM1 CLUT order: swap index bits 3 and 4.
pub fn csm1(i: usize) -> usize {
    (i & !0x18) | ((i & 8) << 1) | ((i & 0x10) >> 1)
}

/// A prototype texture header (already read from the blob).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BetaTexHeader {
    pub format: u16,
    pub w: u16,
    pub h: u16,
    pub mips: u16,
    pub pixels: u32,
    pub palette: u32,
}

impl BetaTexHeader {
    pub fn read(blob: &[u8], off: usize) -> Result<BetaTexHeader> {
        let b = blob.get(off..off + BETA_HEADER_LEN).context("texture header outside the blob")?;
        let h = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
        let w = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        Ok(BetaTexHeader { format: h(0), w: h(2), h: h(4), mips: h(6), pixels: w(8), palette: w(12) })
    }

    /// Bytes of level 0.
    pub fn level0_len(&self) -> usize {
        let n = self.w as usize * self.h as usize;
        match self.format {
            0 => n * 3,
            1 => n * 4,
            _ => n,
        }
    }

    /// Bytes of all stored levels (m - 1 levels; m = 0 means 1).
    pub fn stored_len(&self) -> usize {
        let levels = (self.mips as usize).saturating_sub(1).max(1);
        let (mut w, mut h, mut total) = (self.w as usize, self.h as usize, 0);
        let bpp = match self.format {
            0 => 3,
            1 => 4,
            _ => 1,
        };
        for _ in 0..levels {
            total += w * h * bpp;
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        total
    }
}

/// Decode level 0 of a prototype texture. `palette_base` is the blob offset
/// of the pack's palettes.
pub fn decode_beta(blob: &[u8], hdr: &BetaTexHeader, palette_base: usize) -> Result<Image> {
    let (w, h) = (hdr.w as usize, hdr.h as usize);
    ensure!(w > 0 && h > 0, "texture has zero size");
    let start = hdr.pixels as usize;
    let px = blob.get(start..start + hdr.level0_len()).context("texture pixels outside the blob")?;
    let mut rgba = Vec::with_capacity(w * h * 4);
    match hdr.format {
        0 => px.chunks_exact(3).for_each(|p| rgba.extend_from_slice(&[p[0], p[1], p[2], 255])),
        1 => rgba.extend_from_slice(px),
        2 => {
            let pal_off = palette_base + hdr.palette as usize * PALETTE_LEN;
            let pal = blob.get(pal_off..pal_off + PALETTE_LEN).context("palette outside the blob")?;
            for &i in px {
                let s = csm1(i as usize) * 4;
                rgba.extend_from_slice(&pal[s..s + 4]);
            }
        }
        f => bail!("unknown prototype texture format {f}"),
    }
    Ok(Image { w, h, rgba })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GcFormat {
    Rgba8,
    Cmpr,
}

/// Format choice: real translucency needs RGBA8, everything else (opaque or
/// cut-out) goes to CMPR. Matches the GC release's own choices on the cars
/// present in both builds.
pub fn choose_format(img: &Image, name_hint: &str) -> GcFormat {
    let n = name_hint.to_ascii_lowercase();
    if img.partial_alpha() > 0.25 || ((n.contains("glass") || n.contains("window")) && img.partial_alpha() > 0.01) {
        GcFormat::Rgba8
    } else {
        GcFormat::Cmpr
    }
}

/// Number of CMPR levels for a texture: chains stop at 16x16 as on GC.
pub fn cmpr_levels(img: &Image, beta_mips: u16) -> usize {
    if beta_mips == 0 {
        return 1;
    }
    let size = img.w.min(img.h);
    let max = (size.trailing_zeros() as usize).saturating_sub(3).max(1);
    ((beta_mips as usize).saturating_sub(1)).clamp(1, max)
}

/// An encoded GC texture: header fields and pixel bytes.
#[derive(Clone, Debug)]
pub struct GcTexture {
    pub format: u16,
    pub w: u16,
    pub h: u16,
    pub mips: u16,
    pub pixels: Vec<u8>,
}

impl GcTexture {
    /// The 0x20-byte table slot; the pixel pointer is filled in by the caller.
    pub fn header(&self, pixel_ptr: u32) -> [u8; GC_HEADER_SLOT] {
        let mut h = [0u8; GC_HEADER_SLOT];
        h[0..2].copy_from_slice(&self.format.to_le_bytes());
        h[2..4].copy_from_slice(&self.w.to_le_bytes());
        h[4..6].copy_from_slice(&self.h.to_le_bytes());
        h[6..8].copy_from_slice(&self.mips.to_le_bytes());
        h[8..12].copy_from_slice(&pixel_ptr.to_be_bytes());
        h
    }
}

pub fn encode_gc(img: &Image, format: GcFormat, levels: usize) -> Result<GcTexture> {
    ensure!(img.w <= 512 && img.h <= 512, "GC textures are at most 512x512 ({}x{})", img.w, img.h);
    let (w, h) = (img.w as u16, img.h as u16);
    Ok(match format {
        GcFormat::Rgba8 => GcTexture { format: GC_FMT_RGBA8, w, h, mips: 0, pixels: encode_rgba8(img) },
        GcFormat::Cmpr => {
            let mut pixels = Vec::new();
            let mut level = img.clone();
            for k in 0..levels {
                if k > 0 {
                    level = level.half();
                }
                pixels.extend(encode_cmpr(&level));
            }
            GcTexture { format: GC_FMT_CMPR, w, h, mips: if levels <= 1 { 0 } else { levels as u16 }, pixels }
        }
    })
}

/// RGBA8: 4x4 tiles; per tile 16 AR pairs then 16 GB pairs.
pub fn encode_rgba8(img: &Image) -> Vec<u8> {
    let (tw, th) = (img.w.div_ceil(4), img.h.div_ceil(4));
    let mut out = Vec::with_capacity(tw * th * 64);
    for ty in 0..th {
        for tx in 0..tw {
            let mut ar = [0u8; 32];
            let mut gb = [0u8; 32];
            for i in 0..16 {
                let p = img.px_clamped(tx * 4 + i % 4, ty * 4 + i / 4);
                ar[i * 2] = p[3];
                ar[i * 2 + 1] = p[0];
                gb[i * 2] = p[1];
                gb[i * 2 + 1] = p[2];
            }
            out.extend_from_slice(&ar);
            out.extend_from_slice(&gb);
        }
    }
    out
}

fn to565(c: [f32; 3]) -> u16 {
    let q = |v: f32, max: f32| ((v.clamp(0.0, 255.0) * max + 127.5) / 255.0) as u16;
    q(c[0], 31.0) << 11 | q(c[1], 63.0) << 5 | q(c[2], 31.0)
}

fn from565(c: u16) -> [i32; 3] {
    let r = (c >> 11) & 31;
    let g = (c >> 5) & 63;
    let b = c & 31;
    [((r << 3) | (r >> 2)) as i32, ((g << 2) | (g >> 4)) as i32, ((b << 3) | (b >> 2)) as i32]
}

/// Encode one 4x4 block (16 RGBA pixels, row-major) as a GC CMPR sub-block.
fn encode_block(px: &[[u8; 4]; 16]) -> [u8; 8] {
    let transparent: Vec<bool> = px.iter().map(|p| p[3] < 128).collect();
    let opaque: Vec<[f32; 3]> = px
        .iter()
        .zip(&transparent)
        .filter(|(_, &t)| !t)
        .map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32])
        .collect();
    if opaque.is_empty() {
        return [0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF];
    }
    let has_alpha = transparent.iter().any(|&t| t);

    // Endpoints: extremes along the principal axis of the opaque colours.
    let n = opaque.len() as f32;
    let mut mean = [0f32; 3];
    for c in &opaque {
        for k in 0..3 {
            mean[k] += c[k] / n;
        }
    }
    let mut cov = [[0f32; 3]; 3];
    for c in &opaque {
        let d = [c[0] - mean[0], c[1] - mean[1], c[2] - mean[2]];
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] += d[i] * d[j];
            }
        }
    }
    let mut axis = [1f32, 1.0, 1.0];
    for _ in 0..8 {
        let v = [
            cov[0][0] * axis[0] + cov[0][1] * axis[1] + cov[0][2] * axis[2],
            cov[1][0] * axis[0] + cov[1][1] * axis[1] + cov[1][2] * axis[2],
            cov[2][0] * axis[0] + cov[2][1] * axis[1] + cov[2][2] * axis[2],
        ];
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len < 1e-6 {
            break;
        }
        axis = [v[0] / len, v[1] / len, v[2] / len];
    }
    let proj = |c: &[f32; 3]| (c[0] - mean[0]) * axis[0] + (c[1] - mean[1]) * axis[1] + (c[2] - mean[2]) * axis[2];
    let (mut lo, mut hi) = (opaque[0], opaque[0]);
    for c in &opaque {
        if proj(c) < proj(&lo) {
            lo = *c;
        }
        if proj(c) > proj(&hi) {
            hi = *c;
        }
    }
    let (mut c0, mut c1) = (to565(hi), to565(lo));

    // 4-colour mode needs c0 > c1; 3-colour + transparent needs c0 <= c1.
    if has_alpha {
        if c0 > c1 {
            std::mem::swap(&mut c0, &mut c1);
        }
    } else {
        if c0 < c1 {
            std::mem::swap(&mut c0, &mut c1);
        }
        if c0 == c1 {
            let mut out = [0u8; 8];
            out[0..2].copy_from_slice(&c0.to_be_bytes());
            out[2..4].copy_from_slice(&c1.to_be_bytes());
            return out; // all indices 0
        }
    }
    let (a, b) = (from565(c0), from565(c1));
    let palette: Vec<[i32; 3]> = if c0 > c1 {
        vec![
            a,
            b,
            [(2 * a[0] + b[0]) / 3, (2 * a[1] + b[1]) / 3, (2 * a[2] + b[2]) / 3],
            [(a[0] + 2 * b[0]) / 3, (a[1] + 2 * b[1]) / 3, (a[2] + 2 * b[2]) / 3],
        ]
    } else {
        vec![a, b, [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2]]
    };

    let mut out = [0u8; 8];
    out[0..2].copy_from_slice(&c0.to_be_bytes());
    out[2..4].copy_from_slice(&c1.to_be_bytes());
    for (i, p) in px.iter().enumerate() {
        let idx = if transparent[i] && c0 <= c1 {
            3
        } else {
            let mut best = (i32::MAX, 0);
            for (k, q) in palette.iter().enumerate() {
                let d = (p[0] as i32 - q[0]).pow(2) + (p[1] as i32 - q[1]).pow(2) + (p[2] as i32 - q[2]).pow(2);
                if d < best.0 {
                    best = (d, k);
                }
            }
            best.1 as u8
        };
        out[4 + i / 4] |= idx << (6 - 2 * (i % 4));
    }
    out
}

/// CMPR: 8x8 tiles in row-major order, each 4 DXT1-style sub-blocks
/// (TL, TR, BL, BR), colours big-endian, indices MSB-first.
pub fn encode_cmpr(img: &Image) -> Vec<u8> {
    let (tw, th) = (img.w.div_ceil(8), img.h.div_ceil(8));
    let mut out = Vec::with_capacity(tw * th * 32);
    for ty in 0..th {
        for tx in 0..tw {
            for (sx, sy) in [(0, 0), (4, 0), (0, 4), (4, 4)] {
                let mut px = [[0u8; 4]; 16];
                for (i, p) in px.iter_mut().enumerate() {
                    *p = img.px_clamped(tx * 8 + sx + i % 4, ty * 8 + sy + i / 4);
                }
                out.extend_from_slice(&encode_block(&px));
            }
        }
    }
    out
}

/// Decode level 0 of a GC texture (for checks and previews).
pub fn decode_gc(format: u16, w: usize, h: usize, px: &[u8]) -> Result<Image> {
    let mut rgba = vec![0u8; w * h * 4];
    let mut put = |x: usize, y: usize, p: [u8; 4]| {
        if x < w && y < h {
            let i = (y * w + x) * 4;
            rgba[i..i + 4].copy_from_slice(&p);
        }
    };
    match format {
        GC_FMT_RGBA8 => {
            let (tw, th) = (w.div_ceil(4), h.div_ceil(4));
            ensure!(px.len() >= tw * th * 64, "RGBA8 data too short");
            for t in 0..tw * th {
                let tile = &px[t * 64..t * 64 + 64];
                for i in 0..16 {
                    let p = [tile[i * 2 + 1], tile[32 + i * 2], tile[32 + i * 2 + 1], tile[i * 2]];
                    put((t % tw) * 4 + i % 4, (t / tw) * 4 + i / 4, p);
                }
            }
        }
        GC_FMT_CMPR => {
            let (tw, th) = (w.div_ceil(8), h.div_ceil(8));
            ensure!(px.len() >= tw * th * 32, "CMPR data too short");
            for t in 0..tw * th {
                for (s, (sx, sy)) in [(0, 0), (4, 0), (0, 4), (4, 4)].into_iter().enumerate() {
                    let b = &px[t * 32 + s * 8..t * 32 + s * 8 + 8];
                    let c0 = u16::from_be_bytes([b[0], b[1]]);
                    let c1 = u16::from_be_bytes([b[2], b[3]]);
                    let (a, bb) = (from565(c0), from565(c1));
                    let pal: [[i32; 4]; 4] = if c0 > c1 {
                        [
                            [a[0], a[1], a[2], 255],
                            [bb[0], bb[1], bb[2], 255],
                            [(2 * a[0] + bb[0]) / 3, (2 * a[1] + bb[1]) / 3, (2 * a[2] + bb[2]) / 3, 255],
                            [(a[0] + 2 * bb[0]) / 3, (a[1] + 2 * bb[1]) / 3, (a[2] + 2 * bb[2]) / 3, 255],
                        ]
                    } else {
                        [
                            [a[0], a[1], a[2], 255],
                            [bb[0], bb[1], bb[2], 255],
                            [(a[0] + bb[0]) / 2, (a[1] + bb[1]) / 2, (a[2] + bb[2]) / 2, 255],
                            [0, 0, 0, 0],
                        ]
                    };
                    for i in 0..16 {
                        let idx = (b[4 + i / 4] >> (6 - 2 * (i % 4))) & 3;
                        let q = pal[idx as usize];
                        put(
                            (t % tw) * 8 + sx + i % 4,
                            (t / tw) * 8 + sy + i / 4,
                            [q[0] as u8, q[1] as u8, q[2] as u8, q[3] as u8],
                        );
                    }
                }
            }
        }
        f => bail!("unsupported GC texture format {f}"),
    }
    Ok(Image { w, h, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: usize, h: usize, alpha: impl Fn(usize, usize) -> u8) -> Image {
        let mut rgba = vec![];
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 128, alpha(x, y)]);
            }
        }
        Image { w, h, rgba }
    }

    fn mean_err(a: &Image, b: &Image) -> f64 {
        let mut e = 0u64;
        for (p, q) in a.rgba.chunks_exact(4).zip(b.rgba.chunks_exact(4)) {
            if p[3] >= 128 {
                e += (0..3).map(|c| (p[c] as i32 - q[c] as i32).unsigned_abs() as u64).sum::<u64>();
            }
        }
        e as f64 / (a.w * a.h * 3) as f64
    }

    #[test]
    fn csm1_swaps_bits_3_and_4() {
        assert_eq!(csm1(8), 16);
        assert_eq!(csm1(16), 8);
        assert_eq!(csm1(24), 24);
        assert_eq!(csm1(7), 7);
        assert!((0..256).all(|i| csm1(csm1(i)) == i));
    }

    #[test]
    fn rgba8_roundtrip_is_exact() {
        let img = gradient(16, 8, |x, _| (x * 16) as u8);
        let back = decode_gc(GC_FMT_RGBA8, 16, 8, &encode_rgba8(&img)).unwrap();
        assert_eq!(back, img);
    }

    #[test]
    fn cmpr_roundtrip_is_close() {
        let img = gradient(64, 64, |_, _| 255);
        let enc = encode_cmpr(&img);
        assert_eq!(enc.len(), 64 * 64 / 2);
        let back = decode_gc(GC_FMT_CMPR, 64, 64, &enc).unwrap();
        assert!(mean_err(&img, &back) < 6.0, "{}", mean_err(&img, &back));
    }

    #[test]
    fn cmpr_keeps_cutout_alpha() {
        let img = gradient(16, 16, |x, _| if x < 8 { 0 } else { 255 });
        let back = decode_gc(GC_FMT_CMPR, 16, 16, &encode_cmpr(&img)).unwrap();
        for (p, q) in img.rgba.chunks_exact(4).zip(back.rgba.chunks_exact(4)) {
            assert_eq!(p[3] >= 128, q[3] == 255);
        }
    }

    #[test]
    fn mip_levels_stop_at_16() {
        let img = gradient(64, 64, |_, _| 255);
        assert_eq!(cmpr_levels(&img, 7), 3);
        assert_eq!(cmpr_levels(&img, 0), 1);
        let t = encode_gc(&img, GcFormat::Cmpr, 3).unwrap();
        assert_eq!(t.pixels.len(), 2048 + 512 + 128);
        assert_eq!(t.mips, 3);
    }
}

/// Uncompressed 32-bit TGA (top-left origin), for previews.
pub fn tga(img: &Image) -> Vec<u8> {
    let mut out = vec![0u8; 18];
    out[2] = 2;
    out[12..14].copy_from_slice(&(img.w as u16).to_le_bytes());
    out[14..16].copy_from_slice(&(img.h as u16).to_le_bytes());
    out[16] = 32;
    out[17] = 0x28; // 8 alpha bits, top-left origin
    for p in img.rgba.chunks_exact(4) {
        out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    out
}

/// Every texture of a pack's data blob, decoded (level 0), with header info.
pub fn decode_pack_textures(
    platform: crate::format::Platform,
    blob: &[u8],
    num_textures: u32,
    texture_data_offset: u32,
    palette_offset: u32,
) -> Result<Vec<(String, Image)>> {
    let mut out = Vec::new();
    for i in 0..num_textures as usize {
        if platform == crate::format::Platform::Pc {
            let o = texture_data_offset as usize + i * PC_HEADER_LEN;
            let b = blob.get(o..o + PC_HEADER_LEN).context("texture header outside the blob")?;
            let le = |k: usize| u16::from_le_bytes([b[k], b[k + 1]]);
            let ptr = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
            let (f, w, h) = (le(0), le(2) as usize, le(4) as usize);
            let img = decode_pc(f, w, h, blob.get(ptr..).context("texture pixels outside the blob")?)?;
            out.push((format!("fmt{} {}x{} levels{}", f, w, h, le(6)), img));
        } else if platform.is_beta() {
            let h = BetaTexHeader::read(blob, texture_data_offset as usize + i * BETA_HEADER_LEN)?;
            let img = decode_beta(blob, &h, palette_offset as usize)?;
            out.push((format!("fmt{} {}x{} mips{} pal{}", h.format, h.w, h.h, h.mips, h.palette), img));
        } else {
            let o = texture_data_offset as usize + i * GC_HEADER_SLOT;
            let b = blob.get(o..o + 0x10).context("texture header outside the blob")?;
            let le = |k: usize| u16::from_le_bytes([b[k], b[k + 1]]);
            let ptr = crate::format::Endian::Big.u32_at(b, 8).unwrap() as usize;
            let (f, w, h) = (le(0), le(2) as usize, le(4) as usize);
            let img = decode_gc(f, w, h, blob.get(ptr..).context("texture pixels outside the blob")?)?;
            out.push((format!("fmt{} {}x{} mips{}", f, w, h, le(6)), img));
        }
    }
    Ok(out)
}

/// Read an uncompressed 24/32-bit TGA (as written by `tga` or most editors).
pub fn read_tga(b: &[u8]) -> Result<Image> {
    ensure!(b.len() >= 18, "not a TGA file");
    let id_len = b[0] as usize;
    ensure!(b[1] == 0 && b[2] == 2, "only uncompressed true-colour TGA files are supported");
    let w = u16::from_le_bytes([b[12], b[13]]) as usize;
    let h = u16::from_le_bytes([b[14], b[15]]) as usize;
    let bpp = b[16] as usize / 8;
    ensure!(bpp == 3 || bpp == 4, "TGA must be 24 or 32 bits per pixel");
    let top_down = b[17] & 0x20 != 0;
    let px = b.get(18 + id_len..18 + id_len + w * h * bpp).context("TGA pixel data is truncated")?;
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        let sy = if top_down { y } else { h - 1 - y };
        for x in 0..w {
            let s = &px[(sy * w + x) * bpp..];
            let d = (y * w + x) * 4;
            rgba[d..d + 4].copy_from_slice(&[s[2], s[1], s[0], if bpp == 4 { s[3] } else { 255 }]);
        }
    }
    Ok(Image { w, h, rgba })
}

impl Image {
    /// Mirror top to bottom.
    pub fn flipped(&self) -> Image {
        let mut rgba = Vec::with_capacity(self.rgba.len());
        for y in (0..self.h).rev() {
            rgba.extend_from_slice(&self.rgba[y * self.w * 4..(y + 1) * self.w * 4]);
        }
        Image { w: self.w, h: self.h, rgba }
    }

    /// Box-filtered resize (for thumbnails).
    pub fn resized(&self, w: usize, h: usize) -> Image {
        let mut rgba = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let (x0, x1) = (x * self.w / w, ((x + 1) * self.w / w).max(x * self.w / w + 1));
                let (y0, y1) = (y * self.h / h, ((y + 1) * self.h / h).max(y * self.h / h + 1));
                let mut acc = [0u32; 4];
                let mut n = 0;
                for sy in y0..y1.min(self.h) {
                    for sx in x0..x1.min(self.w) {
                        let p = self.px(sx, sy);
                        for k in 0..4 {
                            acc[k] += p[k] as u32;
                        }
                        n += 1;
                    }
                }
                for k in 0..4 {
                    rgba.push((acc[k] / n.max(1)) as u8);
                }
            }
        }
        Image { w, h, rgba }
    }
}

// ---- PC (DXT) ------------------------------------------------------------
//
// PC texture records are packed 0x10-byte headers at `texture_data_offset`
// (little-endian): u16 format (4 = DXT1 without alpha, 5 = DXT5), u16 width,
// u16 height, u16 levels (full chain down to 1x1, or 0 = one level),
// u32 pixel pointer (32-aligned), u32 0. Levels follow each other unpadded.

pub const PC_FMT_DXT1: u16 = 4;
pub const PC_FMT_DXT5: u16 = 5;
pub const PC_HEADER_LEN: usize = 0x10;

/// GameCube textures store each colour channel squared (`gc = c² / 255`)
/// compared with the PC and PS2-prototype art; apply that before encoding
/// for GameCube so converted cars match the retail ones.
pub fn gc_tone(img: &Image) -> Image {
    let mut out = img.clone();
    for p in out.rgba.chunks_exact_mut(4) {
        for c in &mut p[..3] {
            *c = ((*c as u32 * *c as u32 + 127) / 255) as u8;
        }
    }
    out
}

/// One 4x4 block as standard DXT1 colour data (little-endian colours,
/// pixel 0 in the low bits), always in 4-colour mode (alpha ignored).
fn encode_dxt_colour(px: &[[u8; 4]; 16]) -> [u8; 8] {
    // reuse the GameCube block encoder on an opaque copy, then convert
    let mut opaque = *px;
    for p in &mut opaque {
        p[3] = 255;
    }
    let gc = encode_block(&opaque);
    let c0 = u16::from_be_bytes([gc[0], gc[1]]);
    let c1 = u16::from_be_bytes([gc[2], gc[3]]);
    let mut out = [0u8; 8];
    out[0..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    let mut bits = 0u32;
    for i in 0..16 {
        let idx = (gc[4 + i / 4] >> (6 - 2 * (i % 4))) & 3;
        bits |= (idx as u32) << (2 * i);
    }
    out[4..8].copy_from_slice(&bits.to_le_bytes());
    out
}

/// DXT5 alpha block: two endpoints (8-colour mode) and 3-bit indices.
fn encode_dxt5_alpha(px: &[[u8; 4]; 16]) -> [u8; 8] {
    let (lo, hi) = px.iter().fold((255u8, 0u8), |(l, h), p| (l.min(p[3]), h.max(p[3])));
    let mut out = [0u8; 8];
    out[0] = hi;
    out[1] = lo;
    if hi == lo {
        return out; // all indices 0 = a0
    }
    let pal: Vec<i32> = (0..8)
        .map(|k| match k {
            0 => hi as i32,
            1 => lo as i32,
            k => ((8 - k) as i32 * hi as i32 + (k - 1) as i32 * lo as i32) / 7,
        })
        .collect();
    let mut bits = 0u64;
    for (i, p) in px.iter().enumerate() {
        let a = p[3] as i32;
        let best = (0..8).min_by_key(|&k| (pal[k] - a).abs()).unwrap();
        bits |= (best as u64) << (3 * i);
    }
    out[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
    out
}

fn blocks(img: &Image, mut f: impl FnMut(&[[u8; 4]; 16])) {
    for by in 0..img.h.div_ceil(4) {
        for bx in 0..img.w.div_ceil(4) {
            let mut px = [[0u8; 4]; 16];
            for (i, p) in px.iter_mut().enumerate() {
                *p = img.px_clamped(bx * 4 + i % 4, by * 4 + i / 4);
            }
            f(&px);
        }
    }
}

pub fn encode_dxt1(img: &Image) -> Vec<u8> {
    let mut out = vec![];
    blocks(img, |px| out.extend_from_slice(&encode_dxt_colour(px)));
    out
}

pub fn encode_dxt5(img: &Image) -> Vec<u8> {
    let mut out = vec![];
    blocks(img, |px| {
        out.extend_from_slice(&encode_dxt5_alpha(px));
        out.extend_from_slice(&encode_dxt_colour(px));
    });
    out
}

/// An encoded PC texture: header fields and pixel bytes (all levels).
pub struct PcTexture {
    pub format: u16,
    pub w: u16,
    pub h: u16,
    pub levels: u16,
    pub pixels: Vec<u8>,
}

impl PcTexture {
    pub fn header(&self, pixel_ptr: u32) -> [u8; PC_HEADER_LEN] {
        let mut h = [0u8; PC_HEADER_LEN];
        for (k, v) in [self.format, self.w, self.h, self.levels].into_iter().enumerate() {
            h[k * 2..k * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }
        h[8..12].copy_from_slice(&pixel_ptr.to_le_bytes());
        h
    }
}

/// Encode for PC: DXT5 if any pixel is not fully opaque, else DXT1. With
/// `mipmapped`, the full chain down to 1x1 (as the PC data stores it).
pub fn encode_pc(img: &Image, mipmapped: bool) -> Result<PcTexture> {
    ensure!(img.w <= 1024 && img.h <= 1024, "texture too large ({}x{})", img.w, img.h);
    let alpha = img.rgba.chunks_exact(4).any(|p| p[3] < 255);
    let format = if alpha { PC_FMT_DXT5 } else { PC_FMT_DXT1 };
    let levels = if mipmapped { (img.w.max(img.h).trailing_zeros() + 1) as usize } else { 1 };
    let mut pixels = vec![];
    let mut level = img.clone();
    for k in 0..levels {
        if k > 0 {
            level = level.half();
        }
        pixels.extend(if alpha { encode_dxt5(&level) } else { encode_dxt1(&level) });
    }
    Ok(PcTexture {
        format,
        w: img.w as u16,
        h: img.h as u16,
        levels: if mipmapped { levels as u16 } else { 0 },
        pixels,
    })
}

fn dxt_colours(c0: u16, c1: u16, four: bool) -> [[u8; 4]; 4] {
    let e = |c: u16| from565(c);
    let (a, b) = (e(c0), e(c1));
    let m = |x: [i32; 3], a: u8| [x[0] as u8, x[1] as u8, x[2] as u8, a];
    if four || c0 > c1 {
        [
            m(a, 255),
            m(b, 255),
            m([(2 * a[0] + b[0]) / 3, (2 * a[1] + b[1]) / 3, (2 * a[2] + b[2]) / 3], 255),
            m([(a[0] + 2 * b[0]) / 3, (a[1] + 2 * b[1]) / 3, (a[2] + 2 * b[2]) / 3], 255),
        ]
    } else {
        [m(a, 255), m(b, 255), m([(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2], 255), [0, 0, 0, 0]]
    }
}

/// Decode level 0 of a PC texture (DXT1 / DXT5 / raw RGB / RGBA).
pub fn decode_pc(format: u16, w: usize, h: usize, px: &[u8]) -> Result<Image> {
    let mut rgba = vec![0u8; w * h * 4];
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    match format {
        PC_FMT_DXT1 | PC_FMT_DXT5 => {
            let stride = if format == PC_FMT_DXT1 { 8 } else { 16 };
            ensure!(px.len() >= bw * bh * stride, "DXT data too short");
            for b in 0..bw * bh {
                let blk = &px[b * stride..b * stride + stride];
                let (ab, cb) = if format == PC_FMT_DXT5 { (Some(&blk[..8]), &blk[8..]) } else { (None, blk) };
                let c0 = u16::from_le_bytes([cb[0], cb[1]]);
                let c1 = u16::from_le_bytes([cb[2], cb[3]]);
                let pal = dxt_colours(c0, c1, format == PC_FMT_DXT5);
                let bits = u32::from_le_bytes(cb[4..8].try_into().unwrap());
                let alpha = ab.map(|a| {
                    let (a0, a1) = (a[0] as i32, a[1] as i32);
                    let mut ab = [0u8; 8];
                    ab[..6].copy_from_slice(&a[2..8]);
                    (a0, a1, u64::from_le_bytes(ab))
                });
                for i in 0..16 {
                    let (x, y) = ((b % bw) * 4 + i % 4, (b / bw) * 4 + i / 4);
                    if x >= w || y >= h {
                        continue;
                    }
                    let mut p = pal[((bits >> (2 * i)) & 3) as usize];
                    if format == PC_FMT_DXT1 {
                        p[3] = 255; // uploaded as RGB DXT1: alpha ignored
                    }
                    if let Some((a0, a1, ab)) = alpha {
                        let k = ((ab >> (3 * i)) & 7) as i32;
                        p[3] = match k {
                            0 => a0,
                            1 => a1,
                            k if a0 > a1 => ((8 - k) * a0 + (k - 1) * a1) / 7,
                            6 => 0,
                            7 => 255,
                            k => ((6 - k) * a0 + (k - 1) * a1) / 5,
                        } as u8;
                    }
                    let d = (y * w + x) * 4;
                    rgba[d..d + 4].copy_from_slice(&p);
                }
            }
        }
        0 | 1 => {
            let bpp = if format == 0 { 3 } else { 4 };
            ensure!(px.len() >= w * h * bpp, "raw texture data too short");
            for i in 0..w * h {
                let s = &px[i * bpp..];
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[s[0], s[1], s[2], if bpp == 4 { s[3] } else { 255 }]);
            }
        }
        f => bail!("unsupported PC texture format {f}"),
    }
    Ok(Image { w, h, rgba })
}

#[cfg(test)]
mod pc_tests {
    use super::*;

    fn img(w: usize, h: usize, a: impl Fn(usize, usize) -> u8) -> Image {
        let mut rgba = vec![];
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[(x * 255 / w) as u8, 40, (y * 255 / h) as u8, a(x, y)]);
            }
        }
        Image { w, h, rgba }
    }

    #[test]
    fn dxt1_roundtrip_close() {
        let i = img(32, 32, |_, _| 255);
        let t = encode_pc(&i, true).unwrap();
        assert_eq!(t.format, PC_FMT_DXT1);
        assert_eq!(t.levels, 6);
        assert_eq!(t.pixels.len(), 512 + 128 + 32 + 8 + 8 + 8);
        let d = decode_pc(t.format, 32, 32, &t.pixels).unwrap();
        let err: u64 = i.rgba.iter().zip(&d.rgba).map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs() as u64).sum();
        assert!((err as f64 / i.rgba.len() as f64) < 6.0);
    }

    #[test]
    fn dxt5_keeps_alpha() {
        let i = img(16, 16, |x, _| (x * 16) as u8);
        let t = encode_pc(&i, false).unwrap();
        assert_eq!(t.format, PC_FMT_DXT5);
        let d = decode_pc(t.format, 16, 16, &t.pixels).unwrap();
        for (a, b) in i.rgba.chunks_exact(4).zip(d.rgba.chunks_exact(4)) {
            assert!((a[3] as i32 - b[3] as i32).abs() <= 20);
        }
    }

    #[test]
    fn gc_tone_squares() {
        let i = Image { w: 1, h: 1, rgba: vec![255, 128, 0, 77] };
        assert_eq!(gc_tone(&i).rgba, vec![255, 64, 0, 77]);
    }
}
