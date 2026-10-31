//! Sounds: PS2-prototype SFX (VAG ADPCM) to GameCube SFX (PCM16 big-endian).
//!
//! Both builds keep a table of 32-byte descriptors at `sfx_offset` in the
//! pack's sound region, followed by the samples:
//!
//! ```text
//! 0x00 u32 data pointer (sound-region offset, listed in sound_pointers)
//! 0x04 u32 length in bytes      0x08 u32 loop start in bytes (~0 = one-shot)
//! 0x0C u32 sample rate          0x10 u8 0, 0x11 u8 bits (GC: 16)
//! 0x12 u16 volume (0x7FFF)      0x14 12 zero bytes
//! ```
//!
//! The prototype stores headerless VAG (16-byte blocks; block 0 is always
//! silence; the loop start is the byte offset of a block). GC stores signed
//! PCM16 big-endian with sample starts 32-byte aligned.

use anyhow::{ensure, Context, Result};

use crate::format::align;

pub const DESCRIPTOR_LEN: usize = 0x20;
pub const ONE_SHOT: u32 = 0xFFFF_FFFF;

const VAG_FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

/// Decode PS2 SPU ADPCM blocks into signed 16-bit samples (28 per block).
pub fn decode_vag(data: &[u8]) -> Vec<i16> {
    let mut out = Vec::with_capacity(data.len() / 16 * 28);
    let (mut s1, mut s2) = (0i32, 0i32);
    for block in data.chunks_exact(16) {
        let shift = (block[0] & 0x0F) as i32;
        let (f0, f1) = VAG_FILTERS[((block[0] >> 4) as usize).min(4)];
        for &byte in &block[2..16] {
            for nib in [byte & 0x0F, byte >> 4] {
                let raw = ((nib as i32) << 28 >> 16) >> shift;
                let s = (raw + ((s1 * f0 + s2 * f1 + 32) >> 6)).clamp(-32768, 32767);
                out.push(s as i16);
                s2 = s1;
                s1 = s;
            }
        }
    }
    out
}

/// One sound, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sound {
    pub samples: Vec<i16>,
    /// Loop start in samples, or `None` for a one-shot.
    pub loop_start: Option<u32>,
    pub rate: u32,
    pub volume: u16,
}

/// Read every prototype descriptor of a sound region (little-endian).
pub fn read_beta_sounds(region: &[u8], sfx_offset: usize, count: usize) -> Result<Vec<Sound>> {
    let w = |o: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(region.get(o..o + 4).context("sound descriptor outside the sound region")?.try_into()?))
    };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let d = sfx_offset + i * DESCRIPTOR_LEN;
        let (ptr, len, lp, rate) = (w(d)? as usize, w(d + 4)? as usize, w(d + 8)?, w(d + 0xC)?);
        let volume = u16::from_le_bytes([region[d + 0x12], region[d + 0x13]]);
        let vag = region.get(ptr..ptr + len).with_context(|| format!("sound {i}: samples outside the sound region"))?;
        let mut samples = decode_vag(vag);
        // Block 0 is always 28 samples of silence: drop it (GC sounds start
        // directly and loop from 0).
        let drop = samples.len().min(28);
        samples.drain(..drop);
        let loop_start = if lp == ONE_SHOT {
            None
        } else {
            Some(((lp / 16).saturating_sub(1)) * 28)
        };
        out.push(Sound { samples, loop_start, rate, volume });
    }
    Ok(out)
}

/// Build a GC sound region: descriptors then samples. Returns the region
/// and the sound-pointer table (offsets of the descriptors' data fields).
pub fn build_gc_region(sounds: &[Sound]) -> Result<(Vec<u8>, Vec<u32>)> {
    let table = align(sounds.len() * DESCRIPTOR_LEN, 32);
    let mut region = vec![0u8; table];
    let mut pointers = Vec::with_capacity(sounds.len());
    for (i, s) in sounds.iter().enumerate() {
        let at = align(region.len(), 32);
        region.resize(at, 0);
        for &v in &s.samples {
            region.extend_from_slice(&v.to_be_bytes());
        }
        let len = (s.samples.len() * 2) as u32;
        let lp = match s.loop_start {
            None => ONE_SHOT,
            Some(l) => {
                ensure!((l as usize) < s.samples.len().max(1), "sound {i}: loop start past the end");
                l * 2
            }
        };
        let d = i * DESCRIPTOR_LEN;
        region[d..d + 4].copy_from_slice(&(at as u32).to_be_bytes());
        region[d + 4..d + 8].copy_from_slice(&len.to_be_bytes());
        region[d + 8..d + 12].copy_from_slice(&lp.to_be_bytes());
        region[d + 12..d + 16].copy_from_slice(&s.rate.to_be_bytes());
        region[d + 0x11] = 16;
        region[d + 0x12..d + 0x14].copy_from_slice(&s.volume.to_be_bytes());
        pointers.push(d as u32);
    }
    // Readable padding after the last sample (the DMA copies whole 32s).
    let end = align(region.len(), 32);
    region.resize(end, 0);
    Ok((region, pointers))
}

/// Write a sound as a mono 16-bit WAV (for listening / checks).
pub fn wav(s: &Sound) -> Vec<u8> {
    let data_len = (s.samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&s.rate.to_le_bytes());
    out.extend_from_slice(&(s.rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &v in &s.samples {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vag_filter0_is_plain_nibbles() {
        // shift 12, filter 0: sample = nibble (sign-extended)
        let mut block = [0u8; 16];
        block[0] = 12;
        block[2] = 0x1F; // samples -1 (0xF), then 1
        let s = decode_vag(&block);
        assert_eq!(&s[..2], &[-1, 1]);
        assert_eq!(s.len(), 28);
    }

    #[test]
    fn region_layout() {
        let snd = Sound { samples: vec![1, -2, 3], loop_start: Some(0), rate: 22050, volume: 0x7FFF };
        let (r, ptrs) = build_gc_region(&[snd.clone(), Sound { loop_start: None, ..snd }]).unwrap();
        assert_eq!(ptrs, vec![0, 0x20]);
        let data0 = u32::from_be_bytes(r[0..4].try_into().unwrap()) as usize;
        assert_eq!(data0 % 32, 0);
        assert_eq!(&r[data0..data0 + 2], &1i16.to_be_bytes());
        assert_eq!(u32::from_be_bytes(r[0x28..0x2C].try_into().unwrap()), ONE_SHOT);
        assert_eq!(r[0x11], 16);
        assert_eq!(r.len() % 32, 0);
    }
}
