//! One pack (a TOC entry's payload) split into its sections and joined back.
//!
//! Standard pack (every type except geometry), as read by
//! `DataPack_LoadDataPack`:
//!
//! ```text
//! header (120 bytes: 20 words + two 20-byte pack references)
//! [PS2: pad to 2048]
//! if sound_data_size != 0:
//!     sound_pointers  (num_sound_pointers * 4)  [pad to 32]
//!     sound_data      (sound_data_size)         [PS2: pad to 2048]
//! texture_relocs (num * 8)   offset into data, texture index (| pack << 16)
//! shader_relocs  (num * 8)   offset into data, shader index
//! sound_relocs   (num * 8)   offset into data, sound index (| pack << 16)
//! [pad the three tables to 32]
//! data           (data_size) the pack's objects; pointers are offsets
//! ref_relocs     (num * 8)   pointer slots, optionally into a shared pack
//! pointers       (num * 4)   offsets of pointer slots inside data
//! ```
//!
//! The PS2 prototype has no padding and stores `ref_relocs` and `pointers`
//! before `data`.
//!
//! Geometry (quad) pack, as read by `DataPack_LoadQuadPack`:
//!
//! ```text
//! header (84 bytes: 11 words + two 20-byte pack references)
//! texture_relocs, shader_relocs, ref_relocs (num * 8 each), pointers (num * 4)
//! padding so data starts aligned (32 on GameCube, 16 elsewhere)
//! data (data_size)
//! ```
//!
//! Counts and sizes in the headers are derived from the section lengths when
//! joining, so edited sections only need to be dropped back in.

use std::collections::BTreeMap;

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};

use crate::format::{align, fill, Endian, PackType, Platform, TYPE_GEOMETRY};
use crate::hex::Hex;

pub const STD_HEADER_LEN: usize = 120;
pub const QUAD_HEADER_LEN: usize = 84;
const NAME_LEN: usize = 12;

/// A section of a pack: its name (also its file name, plus `.bin`) and the
/// size of one record in it.
pub struct SectionDef {
    pub name: &'static str,
    pub record: usize,
    pub about: &'static str,
}

const fn sec(name: &'static str, record: usize, about: &'static str) -> SectionDef {
    SectionDef { name, record, about }
}

pub const SOUND_POINTERS: SectionDef = sec("sound_pointers", 4, "offsets of pointer slots in sound_data");
pub const SOUND_DATA: SectionDef = sec("sound_data", 1, "sound descriptors, samples, music");
pub const TEXTURE_RELOCS: SectionDef = sec("texture_relocs", 8, "data offset + texture index (shared pack << 16)");
pub const SHADER_RELOCS: SectionDef = sec("shader_relocs", 8, "data offset + shader index");
pub const SOUND_RELOCS: SectionDef = sec("sound_relocs", 8, "data offset + sound index (shared pack << 16)");
pub const DATA: SectionDef = sec("data", 1, "the pack's objects (offsets in place of pointers)");
pub const REF_RELOCS: SectionDef = sec("ref_relocs", 8, "pointer slot + target (shared pack << 16 | index)");
pub const POINTERS: SectionDef = sec("pointers", 4, "offsets of pointer slots in data");

/// Section order on disk for each layout.
pub fn section_order(platform: Platform, quad: bool) -> &'static [SectionDef] {
    if quad {
        &[TEXTURE_RELOCS, SHADER_RELOCS, REF_RELOCS, POINTERS, DATA]
    } else if platform.is_beta() {
        &[SOUND_POINTERS, SOUND_DATA, TEXTURE_RELOCS, SHADER_RELOCS, SOUND_RELOCS, REF_RELOCS, POINTERS, DATA]
    } else {
        &[SOUND_POINTERS, SOUND_DATA, TEXTURE_RELOCS, SHADER_RELOCS, SOUND_RELOCS, DATA, REF_RELOCS, POINTERS]
    }
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// Reference to another pack (e.g. "HW_share") whose textures, sounds or
/// objects this pack uses. Relocation entries name it by `id << 16`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackRef {
    pub name: String,
    pub id: u32,
    /// Exact 12 name bytes, only kept when they are not a clean
    /// zero-padded string. Ignored once `name` is edited.
    #[serde(default, skip_serializing_if = "Hex::is_empty")]
    pub name_raw: Hex,
    /// Runtime pointer slot; zero in files, kept if not.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub runtime_slot: u32,
}

fn decode_name(raw: &[u8]) -> (String, bool) {
    let nul = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    let text = &raw[..nul];
    let clean = text.iter().all(|b| b.is_ascii_graphic() || *b == b' ') && raw[nul..].iter().all(|&b| b == 0);
    (String::from_utf8_lossy(text).into_owned(), clean)
}

impl PackRef {
    fn read(e: Endian, b: &[u8]) -> PackRef {
        let raw = &b[..NAME_LEN];
        let (name, clean) = decode_name(raw);
        PackRef {
            name,
            id: e.u32_at(b, NAME_LEN).unwrap_or(0),
            name_raw: if clean { Hex::default() } else { Hex(raw.to_vec()) },
            runtime_slot: e.u32_at(b, NAME_LEN + 4).unwrap_or(0),
        }
    }

    fn write(&self, e: Endian, out: &mut Vec<u8>) -> Result<()> {
        let raw_matches = self.name_raw.0.len() == NAME_LEN && decode_name(&self.name_raw.0).0 == self.name;
        if raw_matches {
            out.extend_from_slice(&self.name_raw.0);
        } else {
            ensure!(
                self.name.is_ascii() && self.name.len() <= NAME_LEN,
                "pack reference name {:?} must be ASCII and at most {NAME_LEN} characters",
                self.name
            );
            out.extend(fill(self.name.as_bytes(), NAME_LEN));
        }
        e.push(out, self.id);
        e.push(out, self.runtime_slot);
        Ok(())
    }
}

/// Header fields of a standard pack that are not derived from section sizes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StdHeader {
    pub num_textures: u32,
    /// Offset in data of the first texture.
    pub texture_data_offset: u32,
    pub num_sounds: u32,
    /// Offset in sound_data of the sound descriptors (32 bytes each).
    pub sfx_offset: u32,
    pub num_palettes: u32,
    pub palette_offset: u32,
    pub num_shaders: u32,
    pub shader_offset: u32,
    pub num_triggers: u32,
    pub trigger_offset: u32,
    /// Offset in sound_data of the song (music packs, worlds).
    pub song_offset: u32,
    pub song_bank_offset: u32,
    pub refs: [PackRef; 2],
}

/// Header fields of a geometry pack that are not derived from section sizes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuadHeader {
    /// Words 3, 4, 5, 6 and 8: runtime pointers / unused, zero in files.
    #[serde(default, skip_serializing_if = "all_zero")]
    pub unused: [u32; 5],
    pub refs: [PackRef; 2],
}

fn all_zero(v: &[u32; 5]) -> bool {
    v.iter().all(|&x| x == 0)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "layout", rename_all = "snake_case")]
pub enum PackHeader {
    Standard(StdHeader),
    Quad(QuadHeader),
}

/// Contents of `pack.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackJson {
    pub header: PackHeader,
    /// Leftover bytes found in alignment padding (keyed by position), kept
    /// so unmodified packs rebuild byte for byte. Zero padding is omitted.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub padding: BTreeMap<String, Hex>,
    /// Informational listing of the section files; ignored when packing.
    #[serde(default, skip_deserializing)]
    pub sections: Vec<SectionInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SectionInfo {
    pub file: String,
    pub bytes: usize,
    pub records: usize,
    pub about: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitPack {
    pub json: PackJson,
    pub sections: BTreeMap<String, Vec<u8>>,
}

impl SplitPack {
    pub fn is_quad(&self) -> bool {
        matches!(self.json.header, PackHeader::Quad(_))
    }

    fn section(&self, d: &SectionDef) -> Result<&[u8]> {
        let s = self.sections.get(d.name).map_or(&[][..], |v| v.as_slice());
        ensure!(
            s.len().is_multiple_of(d.record),
            "{}.bin is {} bytes, not a multiple of the {}-byte record size",
            d.name,
            s.len(),
            d.record
        );
        Ok(s)
    }

    fn count(&self, d: &SectionDef) -> Result<u32> {
        u32::try_from(self.section(d)?.len() / d.record).context("section too large")
    }

    fn pad(&self, key: &str) -> &[u8] {
        self.json.padding.get(key).map_or(&[][..], |h| h.0.as_slice())
    }

    /// Fill the informational `sections` list of the JSON.
    pub fn describe(&mut self, platform: Platform) {
        self.json.sections = section_order(platform, self.is_quad())
            .iter()
            .filter_map(|d| {
                let s = self.sections.get(d.name)?;
                Some(SectionInfo {
                    file: format!("{}.bin", d.name),
                    bytes: s.len(),
                    records: s.len() / d.record,
                    about: d.about.to_string(),
                })
            })
            .collect();
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    sections: BTreeMap<String, Vec<u8>>,
    padding: BTreeMap<String, Hex>,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.buf.len());
        let Some(end) = end else {
            bail!("{what} ({n} bytes at {:#x}) runs past the end of the pack ({:#x})", self.pos, self.buf.len());
        };
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn section(&mut self, d: &SectionDef, count: u32) -> Result<()> {
        let s = self.take(count as usize * d.record, d.name)?;
        if !s.is_empty() {
            self.sections.insert(d.name.to_string(), s.to_vec());
        }
        Ok(())
    }

    fn pad(&mut self, n: usize, key: &str) -> Result<()> {
        let s = self.take(n, key)?;
        let h = Hex::nonzero(s);
        if !h.is_empty() {
            self.padding.insert(key.to_string(), h);
        }
        Ok(())
    }

    fn pad_to(&mut self, a: usize, key: &str) -> Result<()> {
        self.pad(align(self.pos, a) - self.pos, key)
    }

    fn finish(self, header: PackHeader) -> Result<SplitPack> {
        ensure!(
            self.pos == self.buf.len(),
            "pack has {} unexplained bytes at the end",
            self.buf.len() - self.pos
        );
        Ok(SplitPack {
            json: PackJson { header, padding: self.padding, sections: vec![] },
            sections: self.sections,
        })
    }
}

fn pad32(len: usize) -> usize {
    align(len, 32) - len
}

fn words<const N: usize>(e: Endian, b: &[u8]) -> Result<[u32; N]> {
    let mut w = [0u32; N];
    for (i, v) in w.iter_mut().enumerate() {
        *v = e.u32_at(b, i * 4).context("pack header is truncated")?;
    }
    Ok(w)
}

pub fn split(platform: Platform, kind: PackType, bytes: &[u8]) -> Result<SplitPack> {
    if kind.0 == TYPE_GEOMETRY {
        split_quad(platform, bytes)
    } else {
        split_standard(platform, bytes)
    }
}

fn split_standard(p: Platform, b: &[u8]) -> Result<SplitPack> {
    let e = p.endian();
    ensure!(b.len() >= STD_HEADER_LEN, "pack is smaller than its header");
    let h: [u32; 20] = words(e, b)?;
    let refs = [PackRef::read(e, &b[80..100]), PackRef::read(e, &b[100..120])];
    let mut r = Reader { buf: b, pos: STD_HEADER_LEN, sections: BTreeMap::new(), padding: BTreeMap::new() };
    let ps2 = p == Platform::Ps2;
    let beta = p.is_beta();

    if ps2 {
        r.pad_to(2048, "after_header")?;
    }
    if h[1] != 0 {
        r.section(&SOUND_POINTERS, h[2])?;
        if !beta {
            r.pad(pad32(h[2] as usize * 4), "after_sound_pointers")?;
        }
        r.section(&SOUND_DATA, h[1])?;
        if ps2 {
            r.pad_to(2048, "after_sound_data")?;
        }
    } else if h[2] != 0 {
        bail!("sound pointers without sound data");
    }
    r.section(&TEXTURE_RELOCS, h[4])?;
    r.section(&SHADER_RELOCS, h[13])?;
    r.section(&SOUND_RELOCS, h[7])?;
    if beta {
        r.section(&REF_RELOCS, h[14])?;
        r.section(&POINTERS, h[3])?;
        r.section(&DATA, h[0])?;
    } else {
        r.pad(pad32((h[4] + h[13] + h[7]) as usize * 8), "after_relocs")?;
        r.section(&DATA, h[0])?;
        r.section(&REF_RELOCS, h[14])?;
        r.section(&POINTERS, h[3])?;
    }
    r.finish(PackHeader::Standard(StdHeader {
        num_textures: h[5],
        texture_data_offset: h[6],
        num_sounds: h[8],
        sfx_offset: h[9],
        num_palettes: h[10],
        palette_offset: h[11],
        num_shaders: h[12],
        shader_offset: h[15],
        num_triggers: h[16],
        trigger_offset: h[17],
        song_offset: h[18],
        song_bank_offset: h[19],
        refs,
    }))
}

fn split_quad(p: Platform, b: &[u8]) -> Result<SplitPack> {
    let e = p.endian();
    ensure!(b.len() >= QUAD_HEADER_LEN, "geometry pack is smaller than its header");
    let h: [u32; 11] = words(e, b)?;
    let refs = [PackRef::read(e, &b[44..64]), PackRef::read(e, &b[64..84])];
    let mut r = Reader { buf: b, pos: QUAD_HEADER_LEN, sections: BTreeMap::new(), padding: BTreeMap::new() };
    r.section(&TEXTURE_RELOCS, h[2])?;
    r.section(&SHADER_RELOCS, h[7])?;
    r.section(&REF_RELOCS, h[10])?;
    r.section(&POINTERS, h[1])?;
    let expected = align(r.pos, p.quad_data_align()) - r.pos;
    ensure!(
        h[9] as usize == expected,
        "geometry pack padding is {} bytes, expected {expected}",
        h[9]
    );
    r.pad(expected, "before_data")?;
    r.section(&DATA, h[0])?;
    r.finish(PackHeader::Quad(QuadHeader { unused: [h[3], h[4], h[5], h[6], h[8]], refs }))
}

pub fn join(platform: Platform, sp: &SplitPack) -> Result<Vec<u8>> {
    match &sp.json.header {
        PackHeader::Standard(h) => join_standard(platform, sp, h),
        PackHeader::Quad(h) => join_quad(platform, sp, h),
    }
}

fn len32(s: &[u8]) -> Result<u32> {
    u32::try_from(s.len()).context("section larger than 4 GiB")
}

fn join_standard(p: Platform, sp: &SplitPack, h: &StdHeader) -> Result<Vec<u8>> {
    let e = p.endian();
    let ps2 = p == Platform::Ps2;
    let beta = p.is_beta();
    let sound_ptrs = sp.section(&SOUND_POINTERS)?;
    let sound_data = sp.section(&SOUND_DATA)?;
    ensure!(
        sound_ptrs.is_empty() || !sound_data.is_empty(),
        "sound_pointers.bin is only read when sound_data.bin is present"
    );
    let words = [
        len32(sp.section(&DATA)?)?,
        len32(sound_data)?,
        sp.count(&SOUND_POINTERS)?,
        sp.count(&POINTERS)?,
        sp.count(&TEXTURE_RELOCS)?,
        h.num_textures,
        h.texture_data_offset,
        sp.count(&SOUND_RELOCS)?,
        h.num_sounds,
        h.sfx_offset,
        h.num_palettes,
        h.palette_offset,
        h.num_shaders,
        sp.count(&SHADER_RELOCS)?,
        sp.count(&REF_RELOCS)?,
        h.shader_offset,
        h.num_triggers,
        h.trigger_offset,
        h.song_offset,
        h.song_bank_offset,
    ];
    let mut out = Vec::new();
    for w in words {
        e.push(&mut out, w);
    }
    for r in &h.refs {
        r.write(e, &mut out)?;
    }

    let pad_to = |out: &mut Vec<u8>, a: usize, key: &str| {
        let n = align(out.len(), a) - out.len();
        out.extend(fill(sp.pad(key), n));
    };
    if ps2 {
        pad_to(&mut out, 2048, "after_header");
    }
    if !sound_data.is_empty() {
        out.extend_from_slice(sound_ptrs);
        if !beta {
            out.extend(fill(sp.pad("after_sound_pointers"), pad32(sound_ptrs.len())));
        }
        out.extend_from_slice(sound_data);
        if ps2 {
            pad_to(&mut out, 2048, "after_sound_data");
        }
    }
    let relocs = [&TEXTURE_RELOCS, &SHADER_RELOCS, &SOUND_RELOCS];
    let mut relocs_len = 0;
    for d in relocs {
        let s = sp.section(d)?;
        relocs_len += s.len();
        out.extend_from_slice(s);
    }
    if beta {
        out.extend_from_slice(sp.section(&REF_RELOCS)?);
        out.extend_from_slice(sp.section(&POINTERS)?);
        out.extend_from_slice(sp.section(&DATA)?);
    } else {
        out.extend(fill(sp.pad("after_relocs"), pad32(relocs_len)));
        out.extend_from_slice(sp.section(&DATA)?);
        out.extend_from_slice(sp.section(&REF_RELOCS)?);
        out.extend_from_slice(sp.section(&POINTERS)?);
    }
    Ok(out)
}

fn join_quad(p: Platform, sp: &SplitPack, h: &QuadHeader) -> Result<Vec<u8>> {
    let e = p.endian();
    let tables = [&TEXTURE_RELOCS, &SHADER_RELOCS, &REF_RELOCS, &POINTERS];
    let mut body_len = QUAD_HEADER_LEN;
    for d in tables {
        body_len += sp.section(d)?.len();
    }
    let pad = align(body_len, p.quad_data_align()) - body_len;
    let u = h.unused;
    let words = [
        len32(sp.section(&DATA)?)?,
        sp.count(&POINTERS)?,
        sp.count(&TEXTURE_RELOCS)?,
        u[0],
        u[1],
        u[2],
        u[3],
        sp.count(&SHADER_RELOCS)?,
        u[4],
        pad as u32,
        sp.count(&REF_RELOCS)?,
    ];
    let mut out = Vec::new();
    for w in words {
        e.push(&mut out, w);
    }
    for r in &h.refs {
        r.write(e, &mut out)?;
    }
    for d in tables {
        out.extend_from_slice(sp.section(d)?);
    }
    out.extend(fill(sp.pad("before_data"), pad));
    out.extend_from_slice(sp.section(&DATA)?);
    Ok(out)
}

/// A one-line summary of a pack for listings.
pub fn summary(sp: &SplitPack) -> String {
    let refs = |refs: &[PackRef; 2]| -> String {
        let names: Vec<String> = refs
            .iter()
            .filter(|r| !r.name.is_empty() && r.name_raw.is_empty())
            .map(|r| format!("{}#{}", r.name, r.id))
            .collect();
        if names.is_empty() {
            String::new()
        } else {
            format!(", uses {}", names.join(" + "))
        }
    };
    let n = |d: &SectionDef| sp.sections.get(d.name).map_or(0, |s| s.len() / d.record);
    match &sp.json.header {
        PackHeader::Standard(h) => {
            let mut parts = vec![format!("{} B data", n(&DATA))];
            for (count, what) in [
                (h.num_textures, "textures"),
                (h.num_palettes, "palettes"),
                (h.num_sounds, "sounds"),
                (h.num_shaders, "shaders"),
                (h.num_triggers, "triggers"),
            ] {
                if count != 0 {
                    parts.push(format!("{count} {what}"));
                }
            }
            if h.song_offset != 0 || h.song_bank_offset != 0 {
                parts.push("song".into());
            }
            format!("{}{}", parts.join(", "), refs(&h.refs))
        }
        PackHeader::Quad(h) => format!(
            "{} B geometry, {} texture relocs{}",
            n(&DATA),
            n(&TEXTURE_RELOCS),
            refs(&h.refs)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(name: &str, id: u32) -> PackRef {
        PackRef { name: name.into(), id, name_raw: Hex::default(), runtime_slot: 0 }
    }

    fn std_pack() -> SplitPack {
        let mut sections = BTreeMap::new();
        sections.insert("sound_pointers".into(), vec![1u8; 12]);
        sections.insert("sound_data".into(), vec![2u8; 77]);
        sections.insert("texture_relocs".into(), vec![3u8; 16]);
        sections.insert("data".into(), vec![4u8; 50]);
        sections.insert("ref_relocs".into(), vec![5u8; 8]);
        sections.insert("pointers".into(), vec![6u8; 20]);
        SplitPack {
            json: PackJson {
                header: PackHeader::Standard(StdHeader {
                    num_textures: 1,
                    texture_data_offset: 8,
                    num_sounds: 2,
                    sfx_offset: 0,
                    num_palettes: 0,
                    palette_offset: 0,
                    num_shaders: 0,
                    shader_offset: 0,
                    num_triggers: 0,
                    trigger_offset: 0,
                    song_offset: 0,
                    song_bank_offset: 0,
                    refs: [r("HW_share", 1), r("", 0)],
                }),
                padding: BTreeMap::new(),
                sections: vec![],
            },
            sections,
        }
    }

    #[test]
    fn standard_roundtrip() {
        for p in [Platform::Pc, Platform::Gc, Platform::Ps2, Platform::Ps2Beta] {
            let sp = std_pack();
            let bytes = join(p, &sp).unwrap();
            let back = split(p, PackType(4), &bytes).unwrap();
            assert_eq!(back, sp, "{p:?}");
        }
    }

    #[test]
    fn quad_roundtrip() {
        let mut sections = BTreeMap::new();
        sections.insert("texture_relocs".into(), vec![3u8; 8]);
        sections.insert("pointers".into(), vec![6u8; 12]);
        sections.insert("data".into(), vec![4u8; 33]);
        let sp = SplitPack {
            json: PackJson {
                header: PackHeader::Quad(QuadHeader { unused: [0; 5], refs: [r("w1_tex", 0), r("", 0)] }),
                padding: BTreeMap::new(),
                sections: vec![],
            },
            sections,
        };
        for p in [Platform::Pc, Platform::Gc, Platform::Ps2, Platform::Ps2Beta] {
            let bytes = join(p, &sp).unwrap();
            assert_eq!((bytes.len() - 33) % p.quad_data_align(), 0);
            assert_eq!(split(p, PackType(TYPE_GEOMETRY), &bytes).unwrap(), sp, "{p:?}");
        }
    }

    #[test]
    fn dirty_names_survive_and_can_be_edited() {
        let e = Endian::Little;
        let mut raw = vec![0xCCu8; 12];
        raw.extend_from_slice(&e.bytes(1));
        raw.extend_from_slice(&e.bytes(0));
        let pr = PackRef::read(e, &raw);
        assert!(!pr.name_raw.is_empty());
        let mut out = vec![];
        pr.write(e, &mut out).unwrap();
        assert_eq!(out, raw);
        let edited = PackRef { name: "HW_share".into(), ..pr };
        let mut out = vec![];
        edited.write(e, &mut out).unwrap();
        assert_eq!(&out[..9], b"HW_share\0");
    }

    #[test]
    fn rejects_bad_record_sizes() {
        let mut sp = std_pack();
        sp.sections.insert("pointers".into(), vec![0; 5]);
        assert!(join(Platform::Pc, &sp).is_err());
    }
}
