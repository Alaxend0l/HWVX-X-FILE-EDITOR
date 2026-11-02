//! `XNC` - **X** Velocity **N**ext **C**ontainer: a new, JSON-era pack
//! archive designed by this toolchain. The authoritative specification
//! lives in `docs/compiler/custom-format.md`; this module is its
//! normative implementation.
//!
//! Design goals (see `docs/compiler/roadmap.md`):
//! - keep the proven `XArchive` payload model: entries are complete
//!   platform-order packs, so game-side loaders keep working after unpack;
//! - fix the known pain points: versioned entries, per-entry metadata
//!   (sub_id) without offset arithmetic, 4-byte alignment, explicit
//!   endianness (always little-endian headers), per-entry checksums;
//! - stream-friendly: fixed header -> TOC -> data, no back-patching;
//! - reserved compression slots (`method`), v0 only stores raw payloads.
//!
//! ## Layout (all integers little-endian)
//!
//! ```text
//! Header (fixed, 0x14 bytes)
//!   0x00  magic    "XNC\x1A"
//!   0x04  version  u8   (0)
//!   0x05  flags    u8   (0)
//!   0x06  reserved u16  (0)
//!   0x08  count    u32  (TOC entries)
//!   0x0C  toc_len  u32  (TOC byte size)
//!   0x10  hcrc     u32  CRC32-MPEG-2 of header with hcrc=0
//! TOC: count entries, byte-exact size toc_len
//!   0x00  version   u32
//!   0x04  type      u32
//!   0x08  id        u32
//!   0x0C  sub_id    u32
//!   0x10  data_size u64   uncompressed payload size
//!   0x18  offset    u64   from data-area start, uncompressed
//!   0x20  name_len  u8    following fixed part, ASCII
//!   0x21  method    u8    0=raw (v0 only)
//!   0x22  stored    u64   on-disk payload size
//!   0x2A  dcrc      u32   CRC32-MPEG-2 of stored payload
//!   0x2E  reserved  u16   (0)
//!   0x30  name[name_len]
//! Data area starts at align_up(0x14 + toc_len, 16); each payload
//! begins 16-byte aligned and payloads are stored raw in v0.
//! ```

use crate::format::align;
use anyhow::{ensure, Result};
use std::io::Write;

/// Magic bytes at file offset 0.
pub const XNC_MAGIC: [u8; 4] = *b"XNC\x1A";
/// Container format version implemented by this module.
pub const XNC_VERSION: u8 = 0;
/// Entry schema version emitted by this module.
pub const ENTRY_VERSION: u32 = 0;
/// Fixed header size.
pub const HEADER_LEN: usize = 0x14;
/// Fixed part of a TOC entry (before the variable name).
pub const ENTRY_FIXED_LEN: usize = 0x30;
/// Alignment of the data area and of every payload inside it.
pub const DATA_ALIGN: usize = 16;

/// Storage methods (TOC `method` byte).
pub mod method {
    /// Raw bytes, stored as-is. The only method defined in v0.
    pub const RAW: u8 = 0;
    /// Reserved id for a future zstd layer (not implemented).
    pub const RESERVED_ZSTD: u8 = 1;
    /// Reserved id for a future deflate layer (not implemented).
    pub const RESERVED_DEFLATE: u8 = 2;
}

/// CRC32-MPEG-2: poly `0x04C11DB7`, init `0xFFFFFFFF`, no reflection,
/// no final xor. Same polynomial family used across the console era of
/// the engine; chosen here for predictability, not legacy.
pub fn crc32_mpeg2(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= (b as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// One entry of an `XNC` container: metadata + raw (platform-order) pack
/// payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XncEntry {
    /// Entry schema version (currently 0).
    pub version: u32,
    /// Pack type id (same id space as `XArchive` `PackType`).
    pub ptype: u32,
    /// Pack id.
    pub id: u32,
    /// Sub_id (quad/geometry flavor); `u32::MAX` = absent.
    pub sub_id: u32,
    /// Optional original name (e.g. `HW_CARS`), ASCII, <= 255 chars.
    pub name: Option<String>,
    /// Raw payload bytes, platform byte order.
    pub data: Vec<u8>,
}

impl XncEntry {
    fn encoded_len(&self) -> usize {
        ENTRY_FIXED_LEN + self.name.as_ref().map_or(0, |n| n.len())
    }
}

/// Sentinel for "no sub_id".
pub const NO_SUB_ID: u32 = u32::MAX;

/// An in-memory `XNC` container.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XncFile {
    /// Entries in storage order.
    pub entries: Vec<XncEntry>,
}

impl XncFile {
    /// Build the container, computing all offsets and checksums.
    pub fn build<W: Write>(w: &mut W, entries: &[XncEntry]) -> Result<usize> {
        for (i, e) in entries.iter().enumerate() {
            let nlen = e.name.as_ref().map_or(0, |n| n.len());
            ensure!(nlen <= u8::MAX as usize, "entry {i}: name too long");
            if let Some(n) = &e.name {
                ensure!(n.is_ascii(), "entry {i}: name must be ASCII");
            }
        }
        let toc_len: usize = entries.iter().map(|e| e.encoded_len()).sum();
        let data_area = align(HEADER_LEN + toc_len, DATA_ALIGN);

        // Data area buffer (entries appended with alignment padding).
        let mut data_buf: Vec<u8> = Vec::new();
        let mut toc_buf: Vec<u8> = Vec::with_capacity(toc_len);
        for e in entries.iter() {
            let pad = (DATA_ALIGN - data_buf.len() % DATA_ALIGN) % DATA_ALIGN;
            data_buf.resize(data_buf.len() + pad, 0);
            let offset = data_buf.len() as u64;
            data_buf.extend_from_slice(&e.data);
            let dcrc = crc32_mpeg2(&e.data);

            let mut fixed = [0u8; ENTRY_FIXED_LEN];
            fixed[0x00..0x04].copy_from_slice(&e.version.to_le_bytes());
            fixed[0x04..0x08].copy_from_slice(&e.ptype.to_le_bytes());
            fixed[0x08..0x0C].copy_from_slice(&e.id.to_le_bytes());
            fixed[0x0C..0x10].copy_from_slice(&e.sub_id.to_le_bytes());
            fixed[0x10..0x18].copy_from_slice(&(e.data.len() as u64).to_le_bytes());
            fixed[0x18..0x20].copy_from_slice(&offset.to_le_bytes());
            fixed[0x20] = e.name.as_ref().map_or(0, |n| n.len()) as u8;
            fixed[0x21] = method::RAW;
            fixed[0x22..0x2A].copy_from_slice(&(e.data.len() as u64).to_le_bytes());
            fixed[0x2A..0x2E].copy_from_slice(&dcrc.to_le_bytes());
            toc_buf.extend_from_slice(&fixed);
            if let Some(n) = &e.name {
                toc_buf.extend_from_slice(n.as_bytes());
            }
        }

        let mut header = [0u8; HEADER_LEN];
        header[0x00..0x04].copy_from_slice(&XNC_MAGIC);
        header[0x04] = XNC_VERSION;
        header[0x05] = 0; // flags
        header[0x08..0x0C].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        header[0x0C..0x10].copy_from_slice(&(toc_len as u32).to_le_bytes());
        let hcrc = crc32_mpeg2(&header); // hcrc field still 0
        header[0x10..0x14].copy_from_slice(&hcrc.to_le_bytes());

        w.write_all(&header)?;
        w.write_all(&toc_buf)?;
        let pad = data_area - (HEADER_LEN + toc_len);
        w.write_all(&vec![0u8; pad])?;
        w.write_all(&data_buf)?;
        Ok(HEADER_LEN + toc_len + pad + data_buf.len())
    }

    /// Parse a container written by [`XncFile::build`] (v0, raw only).
    pub fn parse(data: &[u8]) -> Result<Self> {
        ensure!(data.len() >= HEADER_LEN, "xnc: file too short");
        ensure!(data[0x00..0x04] == XNC_MAGIC, "xnc: bad magic");
        ensure!(
            data[0x04] == XNC_VERSION,
            "xnc: unsupported version {}",
            data[0x04]
        );
        ensure!(data[0x05] == 0, "xnc: nonzero flags not supported");
        let mut hcrc_src = [0u8; HEADER_LEN];
        hcrc_src.copy_from_slice(&data[..HEADER_LEN]);
        hcrc_src[0x10..0x14].fill(0);
        let want = u32::from_le_bytes(data[0x10..0x14].try_into().unwrap());
        ensure!(
            crc32_mpeg2(&hcrc_src) == want,
            "xnc: header CRC mismatch"
        );

        let count = u32::from_le_bytes(data[0x08..0x0C].try_into().unwrap()) as usize;
        let toc_len = u32::from_le_bytes(data[0x0C..0x10].try_into().unwrap()) as usize;
        let toc_end = HEADER_LEN + toc_len;
        ensure!(data.len() >= toc_end, "xnc: truncated TOC");
        let data_area = align(toc_end, DATA_ALIGN);

        let mut entries = Vec::with_capacity(count);
        let mut p = HEADER_LEN;
        for i in 0..count {
            ensure!(p + ENTRY_FIXED_LEN <= toc_end, "xnc: truncated entry {i}");
            let f = &data[p..p + ENTRY_FIXED_LEN];
            let version = u32::from_le_bytes(f[0x00..0x04].try_into().unwrap());
            let ptype = u32::from_le_bytes(f[0x04..0x08].try_into().unwrap());
            let id = u32::from_le_bytes(f[0x08..0x0C].try_into().unwrap());
            let sub_id = u32::from_le_bytes(f[0x0C..0x10].try_into().unwrap());
            let size = u64::from_le_bytes(f[0x10..0x18].try_into().unwrap()) as usize;
            let offset = u64::from_le_bytes(f[0x18..0x20].try_into().unwrap()) as usize;
            let name_len = f[0x20] as usize;
            let method_id = f[0x21];
            let stored = u64::from_le_bytes(f[0x22..0x2A].try_into().unwrap()) as usize;
            let dcrc = u32::from_le_bytes(f[0x2A..0x2E].try_into().unwrap());
            ensure!(
                method_id == method::RAW,
                "xnc: entry {i}: compressed methods not implemented (method {method_id})"
            );
            ensure!(stored == size, "xnc: entry {i}: stored != data_size");
            let name = if name_len > 0 {
                ensure!(
                    p + ENTRY_FIXED_LEN + name_len <= toc_end,
                    "xnc: truncated name {i}"
                );
                let n = &data[p + ENTRY_FIXED_LEN..p + ENTRY_FIXED_LEN + name_len];
                let n = String::from_utf8(n.to_vec())?;
                ensure!(n.is_ascii(), "xnc: entry {i}: non-ascii name");
                Some(n)
            } else {
                None
            };
            let abs = data_area
                .checked_add(offset)
                .and_then(|s| s.checked_add(size))
                .ok_or_else(|| anyhow::anyhow!("xnc: entry {i}: range overflow"))?;
            ensure!(abs <= data.len(), "xnc: entry {i}: data out of bounds");
            let payload = &data[abs - size..abs];
            ensure!(
                crc32_mpeg2(payload) == dcrc,
                "xnc: entry {i}: data CRC mismatch"
            );
            entries.push(XncEntry {
                version,
                ptype,
                id,
                sub_id,
                name,
                data: payload.to_vec(),
            });
            p += ENTRY_FIXED_LEN + name_len;
        }
        Ok(XncFile { entries })
    }
}

/// Cheap container sniff (magic + version only).
pub fn is_xnc(data: &[u8]) -> bool {
    data.len() >= HEADER_LEN && data[0x00..0x04] == XNC_MAGIC && data[0x04] == XNC_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ptype: u32, id: u32, sub_id: u32, name: Option<&str>, len: usize) -> XncEntry {
        XncEntry {
            version: ENTRY_VERSION,
            ptype,
            id,
            sub_id,
            name: name.map(|s| s.to_string()),
            data: (0..len).map(|i| (i % 251) as u8).collect(),
        }
    }

    #[test]
    fn crc32_mpeg2_vectors() {
        assert_eq!(crc32_mpeg2(b"123456789"), 0x0376E6E7);
        assert_eq!(crc32_mpeg2(b""), 0xFFFFFFFF);
    }

    #[test]
    fn xnc_roundtrip() {
        let entries = vec![
            entry(2, 7, NO_SUB_ID, Some("HW_CARS"), 0),
            entry(17, 42, 0, Some("HW_WORLD"), 1000),
            entry(17, 43, 1, None, 37),
        ];
        let mut out = Vec::new();
        XncFile::build(&mut out, &entries).unwrap();
        assert!(is_xnc(&out));
        let parsed = XncFile::parse(&out).unwrap();
        assert_eq!(parsed.entries, entries);
    }

    #[test]
    fn header_crc_detects_tampering() {
        let entries = vec![entry(2, 7, NO_SUB_ID, None, 32)];
        let mut out = Vec::new();
        XncFile::build(&mut out, &entries).unwrap();
        out[0x08] ^= 0xFF; // corrupt count
        assert!(XncFile::parse(&out).is_err());
    }

    #[test]
    fn data_crc_detects_tampering() {
        let entries = vec![entry(2, 7, NO_SUB_ID, None, 64)];
        let mut out = Vec::new();
        XncFile::build(&mut out, &entries).unwrap();
        let last = out.len() - 1;
        out[last] ^= 0x01;
        assert!(XncFile::parse(&out).is_err());
    }
}
