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
        if platform.is_beta() {
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
