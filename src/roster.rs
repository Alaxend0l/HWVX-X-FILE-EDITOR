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
    e: Endian,
    seen: BTreeSet<u32>,
}

impl Walk<'_> {
    fn ptr(&self, slot: u32) -> Option<u32> {
        self.b.ptr(self.e, slot)
    }
    fn u32(&self, off: u32) -> u32 {
        self.b.u32(self.e, off).unwrap_or(0)
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

fn read16(e: Endian, b: &[u8], off: u32) -> u16 {
    let x = [b[off as usize], b[off as usize + 1]];
    match e {
        Endian::Big => u16::from_be_bytes(x),
        Endian::Little => u16::from_le_bytes(x),
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
pub fn patch(blob: &mut Blob, en: Endian, names: &[(u32, String)], hint: &str) -> Result<RosterReport> {
    let mut rep = RosterReport::default();
    let Some(&new_max) = names.iter().map(|(id, _)| id).max() else { return Ok(rep) };
    let (walked, shaped) = all_headers(blob, en);
    let half = |b: &Blob, o: u32| read16(en, &b.data, o);

    let mut slots_done = BTreeSet::new();
    for h in walked.iter().chain(shaped.iter()).copied() {
        // car-select screens live in the interface packs' header trees
        let car_select = walked.contains(&h) && half(blob, h) == CAR_SELECT_ID;
        let vars = blob.ptr(en, h + 0x1C).and_then(|v| {
            let n = blob.u32(en, v).unwrap_or(0).min(0x1000);
            blob.ptr(en, v + 4).map(|lst| (0..n).map(|i| lst + i * 0x34).collect::<Vec<u32>>())
        });
        if car_select {
            rep.screens += 1;
            // carousel variables: type 0, min 0, max = old last car id
            if let Some(vars) = &vars {
                let car_max = vars
                    .iter()
                    .filter(|&&e| blob.data[e as usize] == 0 && blob.u32(en, e + 4) == Some(0))
                    .filter_map(|&e| blob.u32(en, e + 8))
                    .filter(|&m| (20..64).contains(&m))
                    .max();
                if let Some(old) = car_max {
                    for &e in vars {
                        if blob.data[e as usize] == 0 && blob.u32(en, e + 4) == Some(0) && blob.u32(en, e + 8) == Some(old) && old < new_max {
                            blob.put_u32(en, e + 8, new_max);
                            rep.ranges += 1;
                        }
                    }
                }
            }
        }
        // per-car string lists (names everywhere; unlock hints on car select)
        let n = blob.u32(en, h + 0xC).unwrap_or(0).min(500);
        let Some(objs) = blob.ptr(en, h + 0x10) else { continue };
        for i in 0..n {
            let o = objs + i * 0x6C;
            if (o + 0x6C) as usize > blob.data.len() || !matches!(blob.data[o as usize + 1], 1 | 10 | 12) {
                continue;
            }
            let Some(sub) = blob.ptr(en, o + 0x14) else { continue };
            if blob.data.get((sub + 0x10) as usize) != Some(&1) {
                continue;
            }
            let Some(lst) = blob.ptr(en, sub + 8) else { continue };
            let Some(texts) = blob.ptr(en, lst + 4) else { continue };
            let count = blob.u32(en, lst).unwrap_or(0);
            if !(30..0x400).contains(&count) {
                continue; // not a per-car list
            }
            let strings: Vec<String> = (0..count)
                .map(|k| blob.ptr(en, texts + 4 * k).and_then(|t| blob.ptr(en, t)).map(|s| cstr(blob, s)).unwrap_or_default())
                .collect();
            let is_names = strings.iter().all(|x| x.ends_with('*'));
            if !is_names && !car_select {
                continue;
            }
            ensure!(count > new_max, "a car name list has only {count} entries; ids up to {new_max} need more");
            // The list is indexed by an interface variable (u16 at 0xA); if its
            // range stops at the old last car, raise it (results screens).
            let var = read16(en, &blob.data, lst + 0xA) as usize;
            if let Some(e) = vars.as_ref().and_then(|v| v.get(var)).copied() {
                if let (Some(0), Some(max)) = (blob.u32(en, e + 4), blob.u32(en, e + 8)) {
                    if (20..new_max).contains(&max) {
                        blob.put_u32(en, e + 8, new_max);
                        rep.ranges += 1;
                    }
                }
            }
            for (id, name) in names {
                let Some(slot) = blob.ptr(en, texts + 4 * id) else { continue };
                if !slots_done.insert(slot) {
                    continue;
                }
                let text = if is_names { name.as_str() } else { hint };
                ensure!(text.is_ascii(), "names must be ASCII ({text:?})");
                let mut bytes = text.as_bytes().to_vec();
                bytes.push(0);
                let at = blob.append(&bytes, 4);
                blob.set_ptr(en, slot, at);
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

/// Interface headers of a pack: everything reachable from the root objects
/// of an interface pack, plus anything shaped like an IMHeader (world packs
/// embed their HUD / results screens without a root table entry).
/// Returns (headers reached from the root objects, other header-shaped
/// objects).
fn all_headers(blob: &Blob, en: Endian) -> (BTreeSet<u32>, BTreeSet<u32>) {
    let mut w = Walk { b: blob, e: en, seen: BTreeSet::new() };
    if let Some(table) = w.ptr(4) {
        for i in 0..w.u32(0).min(64) {
            let h = w.ptr(table + 8 * i);
            w.header(h);
        }
    }
    let walked = w.seen;
    let mut shaped = BTreeSet::new();
    let n = blob.data.len() as u32;
    let targets: BTreeSet<u32> = blob.pointers.iter().filter_map(|&s| blob.u32(en, s)).filter(|&t| t + 0x40 <= n).collect();
    for h in targets {
        let count = blob.u32(en, h + 0xC).unwrap_or(0);
        if !(1..=500).contains(&count) || !blob.pointers.contains(&(h + 0x10)) {
            continue;
        }
        let Some(objs) = blob.ptr(en, h + 0x10) else { continue };
        if objs + count * 0x6C > n {
            continue;
        }
        // object types (byte 1) are small
        let types_ok = (0..count).all(|i| blob.data[(objs + i * 0x6C + 1) as usize] < 16);
        if types_ok && !walked.contains(&h) {
            shaped.insert(h);
        }
    }
    (walked, shaped)
}

/// Local texture indices a car-select screen shows as car `id`'s thumbnail:
/// texture key animations (ObjSub type 3 -> { numKeys, keys, .., u8 0xC = 1
/// "frame from variable" }) whose key `id` is a texture-reloc slot.
pub fn thumbnail_textures(blob: &Blob, en: Endian, id: u32) -> BTreeSet<u32> {
    let mut w = Walk { b: blob, e: en, seen: BTreeSet::new() };
    if let Some(table) = w.ptr(4) {
        for i in 0..w.u32(0).min(64) {
            let h = w.ptr(table + 8 * i);
            w.header(h);
        }
    }
    let tex: std::collections::HashMap<u32, u32> = blob.texture_relocs.iter().map(|r| (r.slot, r.value)).collect();
    let mut out = BTreeSet::new();
    for &h in &w.seen {
        if read16(en, &blob.data, h) != CAR_SELECT_ID {
            continue;
        }
        let n = w.u32(h + 0xC).min(500);
        let Some(objs) = w.ptr(h + 0x10) else { continue };
        for i in 0..n {
            let o = objs + i * 0x6C;
            if w.byte(o + 1) != 0 {
                continue;
            }
            let Some(sub) = w.ptr(o + 0x14) else { continue };
            if w.byte(sub) != 3 {
                continue;
            }
            let Some(anim) = w.ptr(sub + 0x10) else { continue };
            if w.byte(anim + 0xC) != 1 || w.u32(anim) <= id {
                continue;
            }
            if let Some(keys) = w.ptr(anim + 4) {
                if let Some(v) = tex.get(&(keys + 4 * id)) {
                    if v >> 16 == 0 {
                        out.insert(v & 0xFFFF);
                    }
                }
            }
        }
    }
    out
}

/// Replace texture `index` of a GameCube pack with `img` (stored as RGBA8,
/// level 0; the new pixels are appended and the header re-pointed).
pub fn replace_texture_gc(blob: &mut Blob, texture_data_offset: u32, index: u32, img: &crate::convert::texture::Image) -> Result<()> {
    use crate::convert::texture as t;
    let hdr = texture_data_offset + index * t::GC_HEADER_SLOT as u32;
    ensure!((hdr as usize + 0x10) <= blob.data.len(), "texture {index} is outside the pack");
    let enc = t::encode_gc(img, t::GcFormat::Rgba8, 1)?;
    let px = blob.append(&enc.pixels, 32);
    let h = enc.header(px);
    blob.data[hdr as usize..hdr as usize + 0x10].copy_from_slice(&h[..0x10]);
    blob.set_ptr(Endian::Big, hdr + 8, px);
    Ok(())
}

/// Replace texture `index` of a PC pack with `img` (DXT5 or DXT1 with a
/// full mip chain, like the PC thumbnails; new pixels appended, the packed
/// 0x10-byte header updated and re-pointed).
pub fn replace_texture_pc(blob: &mut Blob, texture_data_offset: u32, index: u32, img: &crate::convert::texture::Image) -> Result<()> {
    use crate::convert::texture as t;
    let hdr = texture_data_offset + index * t::PC_HEADER_LEN as u32;
    ensure!((hdr as usize + t::PC_HEADER_LEN) <= blob.data.len(), "texture {index} is outside the pack");
    let enc = t::encode_pc(img, true)?;
    let px = blob.append(&enc.pixels, 32);
    let h = enc.header(px);
    blob.data[hdr as usize..hdr as usize + t::PC_HEADER_LEN].copy_from_slice(&h);
    blob.set_ptr(Endian::Little, hdr + 8, px);
    Ok(())
}
