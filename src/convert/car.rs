//! Convert a PS2-prototype car pack into a GameCube car pack.
//!
//! Steps (see `docs/beta-cars/FORMATS.md`):
//! 1. Read the prototype models, textures and sounds (little-endian).
//! 2. Byte-swap the object graph in place using the walker's field map
//!    (every object type has the same layout in both builds).
//! 3. Drop the prototype models/textures and append GameCube ones
//!    (models: GX display lists; textures: CMPR / RGBA8; sounds: PCM16).
//! 4. Re-point every reference to a converted model.
//! 5. Remap references into the shared packs (HW_share, HW_Cars shared pack)
//!    with the import map.
//! 6. Take the HUD and camera setup from a GameCube donor car (the
//!    prototype's do not work on GameCube), keeping the prototype's stat bars
//!    and bone camera.
//! 7. Give the car its new pack id.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;

use crate::blob::{Blob, Reloc};
use crate::convert::{carwalk, geometry, sound, texture};
use crate::format::{align, Endian, Platform};
use crate::pack::{PackHeader, SplitPack, StdHeader};

const LE: Endian = Endian::Little;
/// Set once the PC model / texture / sound writers exist.
const PC_READY: bool = true;

/// Shared-pack reference map (`data/beta-imports.json`).
#[derive(Clone, Debug, Deserialize)]
pub struct ImportMap {
    pub texture: BTreeMap<String, Option<String>>,
    pub sound: BTreeMap<String, Option<String>>,
    pub model: BTreeMap<String, Option<String>>,
}

fn parse_ref(s: &str) -> Result<u32> {
    let (p, i) = s.split_once(':').with_context(|| format!("bad reference {s:?} (expected pack:index)"))?;
    Ok(p.trim().parse::<u32>()? << 16 | i.trim().parse::<u32>()?)
}

impl ImportMap {
    pub fn parse(bytes: &[u8]) -> Result<ImportMap> {
        serde_json::from_slice(bytes).context("invalid import map")
    }

    /// Look up a shared reference value (`index | pack << 16`).
    /// `Ok(None)`: known, no counterpart. Error: not in the map.
    fn get(table: &BTreeMap<String, Option<String>>, what: &str, value: u32) -> Result<Option<u32>> {
        let key = format!("{}:{}", value >> 16, value & 0xFFFF);
        match table.get(&key) {
            Some(Some(v)) => Ok(Some(parse_ref(v)?)),
            Some(None) => Ok(None),
            None => bail!("the import map has no entry for {what} {key}"),
        }
    }
}

/// Everything the converter needs besides the prototype pack.
pub struct Options<'a> {
    pub new_id: u32,
    pub donor: &'a SplitPack,
    pub imports: &'a ImportMap,
    /// CarAInfo.viewOffset: how far the car is shifted when an interface
    /// shows it (car select turntable). The prototype has 0 everywhere; GC
    /// cars use y = 0.5 .. 3.5 (median 2.0), tuned per car.
    pub view_offset: [f32; 3],
    /// Target build: GameCube or PC.
    pub platform: Platform,
}

/// What happened, for the user.
#[derive(Default, Debug)]
pub struct Report {
    pub models: usize,
    pub shadow: bool,
    pub textures: Vec<String>,
    pub sounds: usize,
    pub remapped: usize,
    pub dropped: Vec<String>,
    pub notes: Vec<String>,
}

fn std_header(sp: &SplitPack) -> Result<&StdHeader> {
    match &sp.json.header {
        PackHeader::Standard(h) => Ok(h),
        PackHeader::Quad(_) => bail!("expected a car pack, found a geometry pack"),
    }
}

/// Byte ranges reachable from `root` by following pointer slots (and local
/// ref relocs), cut at pointer targets. Used to lift a self-contained
/// subtree (the donor's HUD) out of a pack.
fn closure(blob: &Blob, e: Endian, root: u32) -> Vec<(u32, u32)> {
    let mut slots: BTreeSet<u32> = blob.pointers.clone();
    slots.extend(blob.ref_relocs.iter().filter(|r| r.pack() == 0).map(|r| r.slot));
    let n = blob.data.len() as u32;
    let targets: BTreeSet<u32> = slots.iter().filter_map(|&s| blob.u32(e, s)).filter(|&t| t < n).collect();
    let mut seen = BTreeSet::new();
    let mut stack = vec![root];
    let mut chunks = vec![];
    while let Some(a) = stack.pop() {
        if a >= n || !seen.insert(a) {
            continue;
        }
        let end = targets.range(a + 1..).next().copied().unwrap_or(n);
        chunks.push((a, end));
        for s in slots.range(a & !3..end) {
            if let Some(t) = blob.u32(e, *s) {
                stack.push(t);
            }
        }
    }
    chunks.sort();
    chunks
}

/// Copy `start..end` of `src` into a new blob (offsets from 0), with its
/// pointer slots and relocations. Pointers must stay inside the range.
fn copy_span(src: &Blob, e: Endian, start: u32, end: u32) -> Result<Blob> {
    let mut b = Blob { data: src.data[start as usize..end as usize].to_vec(), ..Default::default() };
    for &s in src.pointers.range(start..end) {
        let t = src.u32(e, s).unwrap_or(0);
        ensure!((start..=end).contains(&t), "pointer at {s:#x} leaves the copied range");
        b.set_ptr(e, s - start, t - start);
    }
    let inside = |r: &Reloc| (start..end).contains(&r.slot);
    let shift = |r: &Reloc| Reloc { slot: r.slot - start, value: r.value };
    b.texture_relocs = src.texture_relocs.iter().filter(|r| inside(r)).map(shift).collect();
    b.sound_relocs = src.sound_relocs.iter().filter(|r| inside(r)).map(shift).collect();
    b.shader_relocs = src.shader_relocs.iter().filter(|r| inside(r)).map(shift).collect();
    for r in src.ref_relocs.iter().filter(|r| inside(r)) {
        if r.pack() == 0 {
            let t = src.u32(e, r.slot).unwrap_or(0);
            ensure!((start..=end).contains(&t), "local reference at {:#x} leaves the copied range", r.slot);
            b.put_u32(e, r.slot - start, t - start);
        }
        b.ref_relocs.push(shift(r));
    }
    Ok(b)
}

/// The donor's HUD as a stand-alone blob (it is one contiguous,
/// self-contained block in every GameCube car).
fn donor_hud(donor: &Blob, e: Endian) -> Result<Blob> {
    let car = donor.u32(e, donor.u32(e, 4).context("donor has no root table")?).context("donor has no car")?;
    let hud = donor.ptr(e, car + 0x84).context("donor car has no HUD")?;
    let chunks = closure(donor, e, hud);
    let start = chunks.first().unwrap().0;
    let end = chunks.iter().map(|c| c.1).max().unwrap();
    let covered: u32 = chunks.iter().map(|(a, b)| b - a).sum();
    ensure!(start == hud && covered == end - start, "the donor's HUD is not one contiguous block");
    copy_span(donor, e, start, end)
}

/// Build the camera list at `engine + list` from the donor's list,
/// taking bone cameras (type 4) from the prototype's list when it has one.
/// Returns the list blob (entries first, then parameters) and its length.
fn camera_list(donor: &Blob, d_engine: u32, ours: &Blob, o_engine: u32, list: u32, en: Endian) -> Result<(Blob, u32)> {
    let read = |b: &Blob, eng: u32| -> Vec<(Vec<u8>, Vec<u8>)> {
        let n = b.u32(en, eng + list).unwrap_or(0).min(0x40);
        let Some(entries) = b.ptr(en, eng + list + 4) else { return vec![] };
        (0..n)
            .map(|i| {
                let e = entries + i * 0xC;
                let entry = b.data[e as usize..e as usize + 0xC].to_vec();
                // A parameter block runs up to the next object (type 0 and 4
                // blocks are 0x18 bytes, zoom/orbit 0x10-0x1C).
                let params = b.ptr(en, e + 4).map(|p| b.data[p as usize..(p + object_len(b, en, p, 0x40)) as usize].to_vec()).unwrap_or_default();
                (entry, params)
            })
            .collect()
    };
    let donor_list = read(donor, d_engine);
    let our_bone = read(ours, o_engine).into_iter().find(|(e, p)| e[0] == 4 && !p.is_empty());
    let mut b = Blob::default();
    b.data.resize(donor_list.len() * 0xC, 0);
    for (i, (entry, params)) in donor_list.iter().enumerate() {
        let at = (i * 0xC) as u32;
        b.data[at as usize..at as usize + 0xC].copy_from_slice(entry);
        let params = match (&our_bone, entry[0]) {
            (Some((_, p)), 4) => p,
            _ => params,
        };
        if params.is_empty() {
            b.put_u32(en, at + 4, 0);
        } else {
            let p = b.append(params, 16);
            b.set_ptr(en, at + 4, p);
        }
    }
    Ok((b, donor_list.len() as u32))
}

/// Bytes from `at` to the next pointer target (the next object), capped.
fn object_len(b: &Blob, e: Endian, at: u32, cap: u32) -> u32 {
    let next = b
        .pointers
        .iter()
        .chain(b.ref_relocs.iter().filter(|r| r.pack() == 0).map(|r| &r.slot))
        .filter_map(|&s| b.u32(e, s))
        .filter(|&t| t > at)
        .min()
        .unwrap_or(b.data.len() as u32);
    (next - at).min(cap).min(b.data.len() as u32 - at)
}

/// Every prototype geometry header in the blob (shape scan).
fn find_beta_geometries(blob: &Blob) -> Vec<u32> {
    (0..blob.data.len().saturating_sub(0x40) as u32)
        .step_by(16)
        .filter(|&o| geometry::is_beta_geometry(blob, o))
        .collect()
}

pub fn convert(beta: &SplitPack, opt: &Options) -> Result<(SplitPack, Report)> {
    let mut report = Report::default();
    let target = opt.platform;
    let te = target.endian();
    ensure!(matches!(target, Platform::Gc | Platform::Pc), "cars can be converted to GameCube or PC");
    // PC models, textures and sounds: see pc_models / pc_textures / pc_sounds
    ensure!(target == Platform::Gc || PC_READY, "converting to PC is not implemented yet");
    let h = std_header(beta)?.clone();
    let mut blob = Blob::from_split(LE, beta);
    let donor = Blob::from_split(te, opt.donor);

    // ---- 1. read what gets rebuilt -------------------------------------------
    let walk = carwalk::walk(&blob, LE);
    let car = walk.car_info.context("the pack has no CarAInfo root object")?;
    let shadow_at = blob.u32(LE, car + 0x54).filter(|&g| geometry::is_beta_geometry(&blob, g));
    let geoms = find_beta_geometries(&blob);
    for &r in &walk.geometry_roots {
        if blob.u32(LE, r).is_some() && !geoms.contains(&r) && geometry::is_beta_geometry(&blob, r) {
            report.notes.push(format!("geometry root {r:#x} missed by the scan"));
        }
    }
    let mut models = Vec::new();
    for &g in &geoms {
        models.push((g, geometry::read_beta(&blob, g).with_context(|| format!("model at {g:#x}"))?));
    }

    let mut images = Vec::new();
    for i in 0..h.num_textures as usize {
        let th = texture::BetaTexHeader::read(&blob.data, h.texture_data_offset as usize + i * texture::BETA_HEADER_LEN)?;
        let img = texture::decode_beta(&blob.data, &th, h.palette_offset as usize).with_context(|| format!("texture {i}"))?;
        images.push((th, img));
    }
    let region = beta.sections.get("sound_data").cloned().unwrap_or_default();
    let sounds = if h.num_sounds > 0 { sound::read_beta_sounds(&region, h.sfx_offset as usize, h.num_sounds as usize)? } else { vec![] };

    // ---- 2. byte-swap the object graph (GameCube is big-endian; PC is
    // little-endian like the prototype) -------------------------------------
    if te != LE {
        walk.swap.swap(&mut blob.data);
    }

    // ---- 3a. drop the prototype models and textures ----------------------
    // (extents are computed on the original little-endian data)
    let le_view = Blob::from_split(LE, beta);
    let mut dead: Vec<(u32, u32)> = vec![];
    for &g in &geoms {
        dead.extend(geometry::beta_extent(&le_view, g)?);
    }
    let tex_hdrs = (h.texture_data_offset, h.texture_data_offset + h.num_textures * texture::BETA_HEADER_LEN as u32);
    if h.num_textures > 0 {
        dead.push(tex_hdrs);
        for (th, _) in &images {
            dead.push((th.pixels, th.pixels + th.stored_len() as u32));
        }
    }
    if h.num_palettes > 0 {
        dead.push((h.palette_offset, h.palette_offset + h.num_palettes * texture::PALETTE_LEN as u32));
    }
    for &(a, b) in &dead {
        blob.kill(a, b);
    }

    // ---- 5. remap shared references (before anything GameCube is grafted) --
    let mut white_needed = false;
    let mut tex_relocs = vec![];
    for r in std::mem::take(&mut blob.texture_relocs) {
        if r.pack() == 0 {
            tex_relocs.push(Reloc { slot: r.slot, value: r.value & 0xFFFF });
            blob.put_u32(te, r.slot, 0);
            continue;
        }
        match ImportMap::get(&opt.imports.texture, "texture", r.value)? {
            Some(v) => {
                report.remapped += 1;
                tex_relocs.push(Reloc { slot: r.slot, value: v });
            }
            None => {
                white_needed = true;
                tex_relocs.push(Reloc { slot: r.slot, value: h.num_textures });
            }
        }
        blob.put_u32(te, r.slot, 0);
    }
    blob.texture_relocs = tex_relocs;
    let mut snd_relocs = vec![];
    for r in std::mem::take(&mut blob.sound_relocs) {
        if r.pack() == 0 {
            snd_relocs.push(r);
            continue;
        }
        match ImportMap::get(&opt.imports.sound, "sound", r.value)? {
            Some(v) => {
                report.remapped += 1;
                snd_relocs.push(Reloc { slot: r.slot, value: v });
            }
            None => report.dropped.push(format!("sound {}:{}", r.pack(), r.index())),
        }
    }
    blob.sound_relocs = snd_relocs;
    let mut ref_relocs = vec![];
    for r in std::mem::take(&mut blob.ref_relocs) {
        if r.pack() == 0 {
            ref_relocs.push(r);
            continue;
        }
        match ImportMap::get(&opt.imports.model, "model", r.value)? {
            Some(v) => {
                report.remapped += 1;
                ref_relocs.push(Reloc { slot: r.slot, value: v });
            }
            None => {
                report.dropped.push(format!("model {}:{}", r.pack(), r.index()));
                blob.put_u32(te, r.slot, 0);
            }
        }
    }
    blob.ref_relocs = ref_relocs;
    let white_index = h.num_textures;

    // ---- 3b. models ----------------------------------------------------------
    let mut moved: HashMap<u32, u32> = HashMap::new();
    for (g, model) in &models {
        let mut remap = |v: u32| -> Option<u32> {
            if v >> 16 == 0 {
                return Some(v & 0xFFFF);
            }
            match ImportMap::get(&opt.imports.texture, "texture", v) {
                Ok(Some(x)) => Some(x),
                Ok(None) => {
                    white_needed = true;
                    Some(white_index)
                }
                Err(_) => None,
            }
        };
        let built = if Some(*g) == shadow_at {
            report.shadow = true;
            if target == Platform::Pc { geometry::build_pc_shadow(model)? } else { geometry::build_gc_shadow(model)? }
        } else {
            if target == Platform::Pc { geometry::build_pc(model, &mut remap)? } else { geometry::build_gc(model, &mut remap)? }
        };
        let at = blob.graft(te, &built, 32)?;
        moved.insert(*g, at);
        report.models += 1;
    }
    // ---- 4. re-point references to models -----------------------------------
    let local_refs: Vec<u32> = blob.ref_relocs.iter().filter(|r| r.pack() == 0).map(|r| r.slot).collect();
    let slots: Vec<u32> = blob.pointers.iter().copied().chain(local_refs).collect();
    for s in slots {
        if let Some(t) = blob.u32(te, s) {
            if let Some(&n) = moved.get(&t) {
                blob.put_u32(te, s, n);
            }
        }
    }

    // ---- 3c. textures ---------------------------------------------------------
    let mut tex = Blob::default();
    let count = images.len() + white_needed as usize;
    let slot_len = if target == Platform::Pc { texture::PC_HEADER_LEN } else { texture::GC_HEADER_SLOT };
    tex.data.resize(count * slot_len, 0);
    let white = texture::Image { w: 16, h: 16, rgba: vec![255; 16 * 16 * 4] };
    for (i, (th, img)) in images.iter().map(|(t, i)| (Some(t), i)).chain(white_needed.then_some((None, &white))).enumerate() {
        let slot = (i * slot_len) as u32;
        let mipmapped = th.is_some_and(|t| t.mips > 0);
        let (header, desc): (Vec<u8>, String) = if target == Platform::Pc {
            // PC art has the prototype's brightness; DXT1 or DXT5, full mip chain
            let enc = texture::encode_pc(img, mipmapped)?;
            let px = tex.append(&enc.pixels, 32);
            let fmt = if enc.format == texture::PC_FMT_DXT1 { "DXT1" } else { "DXT5" };
            let mips = if enc.levels > 1 { format!(" ({} levels)", enc.levels) } else { String::new() };
            (enc.header(px).to_vec(), format!("{fmt}{mips}"))
        } else {
            // GameCube textures are stored darker (each channel squared)
            let img = texture::gc_tone(img);
            let fmt = texture::choose_format(&img, "");
            let levels = match fmt {
                texture::GcFormat::Cmpr => texture::cmpr_levels(&img, th.map_or(0, |t| t.mips)),
                texture::GcFormat::Rgba8 => 1,
            };
            let enc = texture::encode_gc(&img, fmt, levels)?;
            let px = tex.append(&enc.pixels, 32);
            let f = if enc.format == texture::GC_FMT_CMPR { "CMPR" } else { "RGBA8" };
            let mips = if enc.mips > 1 { format!(" ({} mips)", enc.mips) } else { String::new() };
            (enc.header(px).to_vec(), format!("{f}{mips}"))
        };
        tex.data[slot as usize..slot as usize + header.len()].copy_from_slice(&header);
        tex.pointers.insert(slot + 8);
        report.textures.push(format!("{}{}x{} -> {}", if th.is_none() { "white " } else { "" }, img.w, img.h, desc));
    }
    let tex_at = if count > 0 { blob.graft(te, &tex, 32)? } else { 0 };

    // ---- 6. HUD and cameras from the donor -------------------------------------
    let d_car = donor.u32(te, donor.u32(te, 4).context("donor root")?).context("donor car")?;
    if let Some(our_hud) = blob.ptr(te, car + 0x84) {
        let stats = blob.data[(our_hud + 0x34) as usize..(our_hud + 0x38) as usize].to_vec();
        let hud = donor_hud(&donor, te)?;
        let at = blob.graft(te, &hud, 32)?;
        blob.data[(at + 0x34) as usize..(at + 0x38) as usize].copy_from_slice(&stats);
        blob.set_ptr(te, car + 0x84, at);
    }
    if let (Some(d_eng), Some(o_eng)) = (donor.ptr(te, d_car + 0x80), blob.ptr(te, car + 0x80)) {
        for list in [0x80, 0x88] {
            let (cams, n) = camera_list(&donor, d_eng, &blob, o_eng, list, te)?;
            let at = blob.graft(te, &cams, 16)?;
            blob.put_u32(te, o_eng + list, n);
            blob.set_ptr(te, o_eng + list + 4, at);
        }
    }
    // Damage tuning: the prototype breaks parts off at every hit (level 1,
    // 99 %); every GameCube car uses 20 hits / 75 %. Take the donor's.
    blob.data[(car + 0x10) as usize] = donor.data[(d_car + 0x10) as usize];
    let level = donor.u32(te, d_car + 0x18).unwrap_or(20);
    blob.put_u32(te, car + 0x18, level);
    for (k, v) in opt.view_offset.iter().enumerate() {
        blob.put_u32(te, car + 4 * k as u32, v.to_bits());
    }

    // ---- 7. new pack id ----------------------------------------------------------
    let table = blob.u32(te, 4).context("root table")?;
    ensure!(blob.u32(te, 0) == Some(1), "expected one root object");
    blob.put_u32(te, table + 4, opt.new_id);

    // ---- sounds ---------------------------------------------------------------------
    let (snd_region, snd_ptrs) =
        if target == Platform::Pc { sound::build_pc_region(&sounds)? } else { sound::build_gc_region(&sounds)? };
    report.sounds = sounds.len();

    blob.data.resize(align(blob.data.len(), 32), 0);
    blob.validate(te)?;
    // no pointer may still aim into dropped data
    for &s in &blob.pointers {
        let t = blob.u32(te, s).unwrap();
        if dead.iter().any(|&(a, b)| t >= a && t < b) {
            bail!("pointer at {s:#x} still aims into dropped data at {t:#x}");
        }
    }

    // ---- assemble the GameCube pack ------------------------------------------------
    let mut out = beta.clone();
    out.json.padding.clear();
    out.sections.clear();
    blob.store(te, &mut out);
    if !sounds.is_empty() {
        out.sections.insert("sound_data".into(), snd_region);
        let mut p = vec![];
        for v in snd_ptrs {
            te.push(&mut p, v);
        }
        out.sections.insert("sound_pointers".into(), p);
    }
    if let PackHeader::Standard(hh) = &mut out.json.header {
        hh.num_textures = count as u32;
        hh.texture_data_offset = if count > 0 { tex_at } else { 0 };
        hh.num_palettes = 0;
        hh.palette_offset = 0;
        hh.sfx_offset = 0;
        hh.song_offset = 0;
        hh.song_bank_offset = 0;
        for r in hh.refs.iter_mut() {
            r.name_raw = Default::default();
            r.runtime_slot = 0;
        }
    }
    out.describe(target);
    // must survive the target's pack layout
    let bytes = crate::pack::join(target, &out)?;
    crate::pack::split(target, crate::format::PackType(0), &bytes)?;
    if white_needed {
        report.notes.push("added a plain white texture for a shared texture the retail game does not have".into());
    }
    Ok((out, report))
}
