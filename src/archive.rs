//! The outer container: header, table of contents, packs.
//!
//! ```text
//! 0x00  u32 magic 0x020100A0
//! 0x04  u32 magic 0x040100AE
//! 0x08  u32 version (183 retail, 177/179 PS2 prototype)
//! 0x0C  u32 entry count
//! 0x10  entries, 24 bytes each:
//!         u32 version, u32 type, u32 id, u32 sub_id, u32 size, u32 offset
//! ...   packs
//! ```
//!
//! `offset` is relative to the end of the TOC in retail builds and to the end
//! of the entry itself in the prototype. The PS2 release puts every pack on a
//! 2048-byte sector and pads the file to a whole sector; the other builds
//! pack them back to back. The game finds packs only through the TOC (quad
//! pack offsets are recomputed from it at load time), so packs can change
//! size freely as long as the TOC is rebuilt.

use anyhow::{bail, ensure, Context, Result};

use crate::format::{align, fill, Endian, PackType, Platform, ARCHIVE_HEADER_LEN, MAGIC0, MAGIC1, TOC_ENTRY_LEN};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub version: u32,
    pub kind: PackType,
    /// Pack id (for geometry: the world id).
    pub id: u32,
    /// Geometry only: quad pack id in the low 16 bits, GX object count above.
    pub sub_id: u32,
    pub data: Vec<u8>,
    /// Bytes between the previous pack (or the TOC) and this one, as found.
    /// Only their content is kept; the length comes from the layout rules.
    pub gap: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Archive {
    pub platform: Platform,
    pub version: u32,
    pub entries: Vec<Entry>,
    /// Bytes after the last pack (PS2 sector padding).
    pub tail: Vec<u8>,
}

fn toc_end(count: usize) -> usize {
    ARCHIVE_HEADER_LEN + count * TOC_ENTRY_LEN
}

/// Where the offset field of entry `index` is measured from.
fn offset_base(platform: Platform, count: usize, index: usize) -> usize {
    if platform.is_beta() {
        ARCHIVE_HEADER_LEN + (index + 1) * TOC_ENTRY_LEN
    } else {
        toc_end(count)
    }
}

impl Archive {
    pub fn parse(data: &[u8], platform: Platform) -> Result<Archive> {
        let e = platform.endian();
        let word = |off: usize| e.u32_at(data, off).context("archive is truncated");
        ensure!(
            word(0)? == MAGIC0 && word(4)? == MAGIC1,
            "bad magic for a {platform} archive (wrong --platform?)"
        );
        let version = word(8)?;
        let count = word(12)? as usize;
        ensure!(
            toc_end(count) <= data.len(),
            "table of contents ({count} entries) runs past the end of the file"
        );

        let mut entries = Vec::with_capacity(count);
        let mut cursor = toc_end(count);
        for i in 0..count {
            let at = ARCHIVE_HEADER_LEN + i * TOC_ENTRY_LEN;
            let f: Vec<u32> = (0..6).map(|k| word(at + k * 4)).collect::<Result<_>>()?;
            let (size, offset) = (f[4] as usize, f[5] as usize);
            let start = offset_base(platform, count, i) + offset;
            let end = start + size;
            ensure!(end <= data.len(), "entry {i}: pack at {start:#x}+{size:#x} runs past the end of the file");
            if start < cursor {
                bail!("entry {i}: pack at {start:#x} overlaps the previous data (ends at {cursor:#x}); out-of-order archives are not supported");
            }
            entries.push(Entry {
                version: f[0],
                kind: PackType(f[1]),
                id: f[2],
                sub_id: f[3],
                data: data[start..end].to_vec(),
                gap: data[cursor..start].to_vec(),
            });
            cursor = end;
        }
        Ok(Archive {
            platform,
            version,
            entries,
            tail: data[cursor..].to_vec(),
        })
    }

    /// Lay the archive out again following the platform's rules. Gap and tail
    /// bytes are reused (zero-extended or cut) to fill whatever space the
    /// alignment now requires.
    pub fn build(&self) -> Result<Vec<u8>> {
        let e: Endian = self.platform.endian();
        let count = self.entries.len();
        let sector = self.platform.sector();

        let mut out = Vec::new();
        for v in [MAGIC0, MAGIC1, self.version, count as u32] {
            e.push(&mut out, v);
        }
        out.resize(toc_end(count), 0);

        for (i, entry) in self.entries.iter().enumerate() {
            let start = align(out.len(), sector);
            out.extend(fill(&entry.gap, start - out.len()));
            let offset = start - offset_base(self.platform, count, i);
            let fields = [
                entry.version,
                entry.kind.0,
                entry.id,
                entry.sub_id,
                u32::try_from(entry.data.len()).context("pack larger than 4 GiB")?,
                u32::try_from(offset).context("archive larger than 4 GiB")?,
            ];
            let at = ARCHIVE_HEADER_LEN + i * TOC_ENTRY_LEN;
            for (k, v) in fields.into_iter().enumerate() {
                out[at + k * 4..at + k * 4 + 4].copy_from_slice(&e.bytes(v));
            }
            out.extend_from_slice(&entry.data);
        }
        let end = align(out.len(), sector);
        out.extend(fill(&self.tail, end - out.len()));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(platform: Platform) -> Archive {
        Archive {
            platform,
            version: if platform.is_beta() { 179 } else { 183 },
            entries: vec![
                Entry { version: 183, kind: PackType(4), id: 0, sub_id: 0, data: vec![1; 100], gap: vec![] },
                Entry { version: 183, kind: PackType(7), id: 3, sub_id: 0x0005_0002, data: vec![2; 37], gap: vec![] },
            ],
            tail: vec![],
        }
    }

    #[test]
    fn roundtrip_all_platforms() {
        for p in [Platform::Pc, Platform::Gc, Platform::Ps2, Platform::Ps2Beta] {
            let a = sample(p);
            let bytes = a.build().unwrap();
            if p == Platform::Ps2 {
                assert_eq!(bytes.len() % 2048, 0);
            }
            let mut b = Archive::parse(&bytes, p).unwrap();
            for e in &mut b.entries {
                e.gap.clear();
            }
            b.tail.clear();
            assert_eq!(a, b, "{p:?}");
            assert_eq!(Platform::detect(&bytes, None).unwrap(), p, "{p:?}");
        }
    }

    #[test]
    fn beta_offsets_are_relative_to_entry_end() {
        let bytes = sample(Platform::Ps2Beta).build().unwrap();
        let e = Endian::Little;
        // Entry 0 starts right after the 2-entry TOC (0x40) = entry end 0x28 + 0x18.
        assert_eq!(e.u32_at(&bytes, 16 + 20), Some(0x18));
        // Entry 1 starts at 0x40 + 100; its own end is 0x40.
        assert_eq!(e.u32_at(&bytes, 40 + 20), Some(100));
    }
}
