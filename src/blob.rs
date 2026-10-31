//! A pack's object data as an editable unit.
//!
//! In a pack, `data.bin` is a blob of objects that point at each other by
//! blob offset. `pointers.bin` lists every pointer slot; the relocation tables
//! list slots that receive texture / shader / sound ids or pointers into
//! shared packs. [`Blob`] keeps all of that together so objects can be
//! appended, re-pointed or dropped without breaking the tables.
//!
//! Byte order is changed with a [`SwapMap`]: one [`Kind`] per byte, saying
//! how that byte belongs to a field (u32/f32 word, u16, single byte...).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, ensure, Result};

use crate::format::{align, Endian};
use crate::pack::{self, SplitPack};

/// One relocation: a slot in the blob and the value the loader puts there
/// (`index | shared_pack_id << 16`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reloc {
    pub slot: u32,
    pub value: u32,
}

impl Reloc {
    pub fn pack(&self) -> u32 {
        self.value >> 16
    }
    pub fn index(&self) -> u32 {
        self.value & 0xFFFF
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Blob {
    pub data: Vec<u8>,
    /// Offsets of pointer slots (each holds a blob offset).
    pub pointers: BTreeSet<u32>,
    pub texture_relocs: Vec<Reloc>,
    pub shader_relocs: Vec<Reloc>,
    pub sound_relocs: Vec<Reloc>,
    pub ref_relocs: Vec<Reloc>,
}

fn relocs(e: Endian, bytes: Option<&Vec<u8>>) -> Vec<Reloc> {
    let Some(b) = bytes else { return vec![] };
    b.chunks_exact(8)
        .map(|c| Reloc { slot: e.u32_at(c, 0).unwrap(), value: e.u32_at(c, 4).unwrap() })
        .collect()
}

fn reloc_bytes(e: Endian, r: &[Reloc]) -> Vec<u8> {
    let mut out = Vec::with_capacity(r.len() * 8);
    for x in r {
        e.push(&mut out, x.slot);
        e.push(&mut out, x.value);
    }
    out
}

impl Blob {
    /// Take the object data and tables out of a split pack (byte order `e`).
    pub fn from_split(e: Endian, sp: &SplitPack) -> Blob {
        let get = |n: &str| sp.sections.get(n);
        let pointers = get(pack::POINTERS.name)
            .map(|b| b.chunks_exact(4).map(|c| e.u32_at(c, 0).unwrap()).collect())
            .unwrap_or_default();
        Blob {
            data: get(pack::DATA.name).cloned().unwrap_or_default(),
            pointers,
            texture_relocs: relocs(e, get(pack::TEXTURE_RELOCS.name)),
            shader_relocs: relocs(e, get(pack::SHADER_RELOCS.name)),
            sound_relocs: relocs(e, get(pack::SOUND_RELOCS.name)),
            ref_relocs: relocs(e, get(pack::REF_RELOCS.name)),
        }
    }

    /// Write the object data and tables back into a split pack (byte order
    /// `e`), leaving its header and sound sections alone.
    pub fn store(&self, e: Endian, sp: &mut SplitPack) {
        let mut put = |name: &str, bytes: Vec<u8>| {
            if bytes.is_empty() {
                sp.sections.remove(name);
            } else {
                sp.sections.insert(name.to_string(), bytes);
            }
        };
        put(pack::DATA.name, self.data.clone());
        let mut p = Vec::with_capacity(self.pointers.len() * 4);
        for &slot in &self.pointers {
            e.push(&mut p, slot);
        }
        put(pack::POINTERS.name, p);
        put(pack::TEXTURE_RELOCS.name, reloc_bytes(e, &self.texture_relocs));
        put(pack::SHADER_RELOCS.name, reloc_bytes(e, &self.shader_relocs));
        put(pack::SOUND_RELOCS.name, reloc_bytes(e, &self.sound_relocs));
        put(pack::REF_RELOCS.name, reloc_bytes(e, &self.ref_relocs));
    }

    pub fn u32(&self, e: Endian, off: u32) -> Option<u32> {
        e.u32_at(&self.data, off as usize)
    }

    pub fn put_u32(&mut self, e: Endian, off: u32, v: u32) {
        let o = off as usize;
        self.data[o..o + 4].copy_from_slice(&e.bytes(v));
    }

    /// Target of the pointer slot at `slot`, if it is one.
    pub fn ptr(&self, e: Endian, slot: u32) -> Option<u32> {
        if self.pointers.contains(&slot) {
            self.u32(e, slot)
        } else {
            None
        }
    }

    /// Make `slot` a pointer slot aimed at `target`.
    pub fn set_ptr(&mut self, e: Endian, slot: u32, target: u32) {
        self.put_u32(e, slot, target);
        self.pointers.insert(slot);
    }

    /// Append `bytes` at the next `alignment` boundary; returns the offset.
    pub fn append(&mut self, bytes: &[u8], alignment: usize) -> u32 {
        let at = align(self.data.len(), alignment);
        self.data.resize(at, 0);
        self.data.extend_from_slice(bytes);
        at as u32
    }

    /// Forget every pointer slot and relocation inside `start..end` (an
    /// object that has been replaced) and zero its bytes.
    pub fn kill(&mut self, start: u32, end: u32) {
        let inside = |s: u32| (start..end).contains(&s);
        self.pointers.retain(|&s| !inside(s));
        for list in [&mut self.texture_relocs, &mut self.shader_relocs, &mut self.sound_relocs, &mut self.ref_relocs] {
            list.retain(|r| !inside(r.slot));
        }
        self.data[start as usize..end as usize].fill(0);
    }

    /// Append another blob (e.g. a converted object built with its own
    /// offsets starting at 0) and rebase its pointers and relocations.
    pub fn graft(&mut self, e: Endian, other: &Blob, alignment: usize) -> Result<u32> {
        let base = self.append(&other.data, alignment);
        for &slot in &other.pointers {
            let t = other.u32(e, slot).unwrap_or(0);
            ensure!(t as usize <= other.data.len(), "grafted pointer at {slot:#x} points outside its blob");
            self.set_ptr(e, base + slot, base + t);
        }
        let shift = |v: &Vec<Reloc>| v.iter().map(|r| Reloc { slot: r.slot + base, value: r.value }).collect::<Vec<_>>();
        self.texture_relocs.extend(shift(&other.texture_relocs));
        self.shader_relocs.extend(shift(&other.shader_relocs));
        self.sound_relocs.extend(shift(&other.sound_relocs));
        self.ref_relocs.extend(shift(&other.ref_relocs));
        Ok(base)
    }

    /// Check that every pointer slot holds an in-range offset and every
    /// relocation slot is inside the blob.
    pub fn validate(&self, e: Endian) -> Result<()> {
        let n = self.data.len() as u32;
        for &s in &self.pointers {
            let Some(t) = self.u32(e, s) else { bail!("pointer slot {s:#x} is outside the blob ({n:#x})") };
            ensure!(t <= n, "pointer slot {s:#x} points to {t:#x}, past the blob end {n:#x}");
        }
        for (name, list) in [
            ("texture", &self.texture_relocs),
            ("shader", &self.shader_relocs),
            ("sound", &self.sound_relocs),
            ("ref", &self.ref_relocs),
        ] {
            for r in list {
                ensure!(r.slot + 4 <= n, "{name} reloc slot {:#x} is outside the blob", r.slot);
            }
        }
        Ok(())
    }
}

/// How a byte of the blob is laid out, for byte-order conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Kind {
    /// Not classified yet: swapped as part of an aligned 32-bit word.
    #[default]
    Unknown,
    /// Part of a 32-bit field (u32, s32, f32, pointer).
    Word,
    /// Part of a 16-bit field.
    Half,
    /// A byte-sized field or raw bytes (strings, pixels, display lists):
    /// never swapped.
    Byte,
}

/// Per-byte layout of a blob. Claims are first-come: a byte claimed once
/// keeps its kind, so the most specific walkers should run first.
#[derive(Clone, Debug)]
pub struct SwapMap {
    pub kinds: Vec<Kind>,
    /// Conflicting claims (offset, existing, requested), for diagnostics.
    pub conflicts: Vec<(u32, Kind, Kind)>,
}

impl SwapMap {
    pub fn new(len: usize) -> SwapMap {
        SwapMap { kinds: vec![Kind::Unknown; len], conflicts: vec![] }
    }

    pub fn claim(&mut self, off: u32, len: u32, kind: Kind) {
        let (s, e) = (off as usize, (off + len) as usize);
        if e > self.kinds.len() {
            return;
        }
        for i in s..e {
            match self.kinds[i] {
                Kind::Unknown => self.kinds[i] = kind,
                k if k != kind => {
                    if self.conflicts.len() < 64 {
                        self.conflicts.push((i as u32, k, kind));
                    }
                }
                _ => {}
            }
        }
    }

    pub fn word(&mut self, off: u32) {
        self.claim(off, 4, Kind::Word)
    }
    pub fn half(&mut self, off: u32) {
        self.claim(off, 2, Kind::Half)
    }
    pub fn bytes(&mut self, off: u32, len: u32) {
        self.claim(off, len, Kind::Byte)
    }

    /// Fraction of bytes that were explicitly claimed.
    pub fn coverage(&self) -> f64 {
        if self.kinds.is_empty() {
            return 1.0;
        }
        self.kinds.iter().filter(|&&k| k != Kind::Unknown).count() as f64 / self.kinds.len() as f64
    }

    /// Reverse the byte order of every field in `data` (in place). Unknown
    /// bytes are treated as parts of aligned 32-bit words.
    pub fn swap(&self, data: &mut [u8]) {
        let n = data.len().min(self.kinds.len());
        let mut i = 0;
        while i < n {
            match self.kinds[i] {
                Kind::Byte => i += 1,
                Kind::Half if i + 2 <= n => {
                    data.swap(i, i + 1);
                    i += 2;
                }
                Kind::Word | Kind::Unknown if i % 4 == 0 && i + 4 <= n => {
                    // A word only if all four bytes agree on being word-ish.
                    let all = self.kinds[i..i + 4].iter().all(|k| matches!(k, Kind::Word | Kind::Unknown));
                    if all {
                        data[i..i + 4].reverse();
                        i += 4;
                    } else {
                        i += 1;
                    }
                }
                _ => i += 1,
            }
        }
    }
}

/// Group consecutive `kinds` into runs, for reports.
pub fn runs(map: &SwapMap) -> BTreeMap<Kind, usize> {
    let mut m = BTreeMap::new();
    for &k in &map.kinds {
        *m.entry(k).or_insert(0) += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_mixed_fields() {
        let mut data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let mut m = SwapMap::new(data.len());
        m.half(4);
        m.bytes(6, 2);
        m.swap(&mut data);
        assert_eq!(data, vec![4, 3, 2, 1, 6, 5, 7, 8, 12, 11, 10, 9]);
    }

    #[test]
    fn graft_rebases_pointers() {
        let e = Endian::Big;
        let mut a = Blob { data: vec![0; 6], ..Default::default() };
        let mut b = Blob { data: vec![0; 8], ..Default::default() };
        b.set_ptr(e, 0, 4);
        b.texture_relocs.push(Reloc { slot: 4, value: 1 });
        let base = a.graft(e, &b, 32).unwrap();
        assert_eq!(base, 32);
        assert_eq!(a.ptr(e, 32), Some(36));
        assert_eq!(a.texture_relocs[0].slot, 36);
        a.validate(e).unwrap();
        a.kill(32, 40);
        assert!(a.pointers.is_empty() && a.texture_relocs.is_empty());
    }
}
