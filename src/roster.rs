//! Car roster editing in the interface packs (`HW_IPack`).
//!
//! The car-select screens are interfaces (IMHeader id 2). They don't hold a
//! table of car ids: the carousel slots are interface variables with a
//! min/max range (max = highest car id), the shown car's pack id is a
//! variable, and car names / unlock hints are string lists indexed by car
//! id (36 entries in the GameCube data: 33 real cars + 3 placeholders).
//! So adding cars means raising the variables' max and writing names.
//!
//! IMHeader: u16 id 0x0, numObjects 0xC, objects 0x10 (0x6C each),
//! events 0x14/0x18, variables 0x1C -> { count, IMVariable[] (0x34 each:
//! u8 type 0x0, s32 min 0x4, s32 max 0x8) }.
//! Text object (type 1/10/12): sub 0x14 -> TextInfo { .., list 0x8 when
//! u8 0x10 == 1 -> { count, texts -> slot -> char* } }.

use std::collections::BTreeSet;

use anyhow::{ensure, Result};

use crate::blob::Blob;
use crate::format::Endian;

const BE: Endian = Endian::Big;
const CAR_SELECT_ID: u16 = 2;

#[derive(Default, Debug)]
pub struct RosterReport {
    pub screens: usize,
    pub ranges: usize,
    pub names: usize,
    pub hints: usize,
    pub notes: Vec<String>,
}

struct Walk<'a> {
    b: &'a Blob,
    seen: BTreeSet<u32>,
}

impl Walk<'_> {
    fn ptr(&self, slot: u32) -> Option<u32> {
        self.b.ptr(BE, slot)
    }
    fn u32(&self, off: u32) -> u32 {
        self.b.u32(BE, off).unwrap_or(0)
    }
    fn byte(&self, off: u32) -> u8 {
        self.b.data.get(off as usize).copied().unwrap_or(0)
    }
    fn header(&mut self, off: Option<u32>) {
        let Some(off) = off else { return };
        if !self.seen.insert(off) {
            return;
        }
        let n = self.u32(off + 0xC).min(500);
        if let Some(objs) = self.ptr(off + 0x10) {
            for i in 0..n {
                let o = objs + i * 0x6C;
                if matches!(self.byte(o + 1), 6 | 7) {
                    if let Some(h) = self.ptr(o + 0x3C) {
                        let hh = self.ptr(h);
                        self.header(hh);
                    }
                }
                if let Some(data) = self.ptr(o + 0x44) {
                    self.events(self.u32(data + 0x10), self.ptr(data + 0x14));
                }
            }
        }
        self.events(self.u32(off + 0x14), self.ptr(off + 0x18));
    }
    fn events(&mut self, n: u32, lst: Option<u32>) {
        let Some(lst) = lst else { return };
        for i in 0..n.min(0x1000) {
            if let Some(d) = self.ptr(lst + i * 0xC + 8) {
                if matches!(self.byte(d + 5), 1 | 0x20) {
                    let h = self.ptr(d);
                    self.header(h);
                }
            }
        }
    }
}

fn cstr(b: &Blob, off: u32) -> String {
    let s = &b.data[off as usize..];
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..n]).into_owned()
}

/// Patch one interface pack's blob (big-endian, GameCube).
/// `names`: (car id, display name; a trailing `*` shows as ™).
/// The carousel range is raised to the highest id given.
pub fn patch(blob: &mut Blob, names: &[(u32, String)], hint: &str) -> Result<RosterReport> {
    let mut rep = RosterReport::default();
    let Some(&new_max) = names.iter().map(|(id, _)| id).max() else { return Ok(rep) };
    let mut w = Walk { b: blob, seen: BTreeSet::new() };
    let roots = w.u32(0);
    if let Some(table) = w.ptr(4) {
        for i in 0..roots.min(64) {
            let h = w.ptr(table + 8 * i);
            w.header(h);
        }
    }
    let headers: Vec<u32> = w.seen.iter().copied().collect();
    let half = |b: &Blob, o: u32| u16::from_be_bytes([b.data[o as usize], b.data[o as usize + 1]]);

    let mut slots_done = BTreeSet::new();
    for h in headers {
        if half(blob, h) != CAR_SELECT_ID {
            continue;
        }
        rep.screens += 1;
        // carousel variables: type 0, min 0, max = old last car id
        if let Some(v) = blob.ptr(BE, h + 0x1C) {
            let n = blob.u32(BE, v).unwrap_or(0).min(0x1000);
            if let Some(lst) = blob.ptr(BE, v + 4) {
                let vars: Vec<u32> = (0..n).map(|i| lst + i * 0x34).collect();
                let car_max = vars
                    .iter()
                    .filter(|&&e| blob.data[e as usize] == 0 && blob.u32(BE, e + 4) == Some(0))
                    .filter_map(|&e| blob.u32(BE, e + 8))
                    .filter(|&m| (20..64).contains(&m))
                    .max();
                if let Some(old) = car_max {
                    for e in vars {
                        if blob.data[e as usize] == 0 && blob.u32(BE, e + 4) == Some(0) && blob.u32(BE, e + 8) == Some(old) {
                            blob.put_u32(BE, e + 8, new_max.max(old));
                            rep.ranges += 1;
                        }
                    }
                }
            }
        }
        // name / hint lists (one entry per car id)
        let n = blob.u32(BE, h + 0xC).unwrap_or(0).min(500);
        let Some(objs) = blob.ptr(BE, h + 0x10) else { continue };
        for i in 0..n {
            let o = objs + i * 0x6C;
            let Some(sub) = blob.ptr(BE, o + 0x14) else { continue };
            if !matches!(blob.data[o as usize + 1], 1 | 10 | 12) || blob.data[(sub + 0x10) as usize] != 1 {
                continue;
            }
            let Some(lst) = blob.ptr(BE, sub + 8) else { continue };
            let Some(texts) = blob.ptr(BE, lst + 4) else { continue };
            let count = blob.u32(BE, lst).unwrap_or(0);
            if count < 30 {
                continue; // not a per-car list
            }
            ensure!(count > new_max, "a car name list has only {count} entries; ids up to {new_max} need more");
            let first = blob.ptr(BE, texts).and_then(|t| blob.ptr(BE, t)).map(|s| cstr(blob, s)).unwrap_or_default();
            let is_names = first.ends_with('*');
            for (id, name) in names {
                let Some(slot) = blob.ptr(BE, texts + 4 * id) else { continue };
                if !slots_done.insert(slot) {
                    continue;
                }
                let text = if is_names { name.as_str() } else { hint };
                ensure!(text.is_ascii(), "names must be ASCII ({text:?})");
                let mut bytes = text.as_bytes().to_vec();
                bytes.push(0);
                let at = blob.append(&bytes, 4);
                blob.set_ptr(BE, slot, at);
                if is_names {
                    rep.names += 1;
                } else {
                    rep.hints += 1;
                }
            }
        }
    }
    let len = crate::format::align(blob.data.len(), 32);
    blob.data.resize(len, 0);
    Ok(rep)
}
