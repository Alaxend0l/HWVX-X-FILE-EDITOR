//! Convert a PS2-prototype world collision pack (type 5) into a GameCube one.
//!
//! The collision pack is one `TerrainGrid` object at the base of `data.bin`
//! (`CollisionTerrain.c` in the GameCube sources): a grid of cells, each with
//! a BSP node tree, a list of polygon indices and a list of edge indices,
//! plus a table of Surface structs and tail records (sounds, textures).
//! The prototype's layout is identical to the GameCube retail one (verified:
//! the PS2-retail and GameCube `w1_col` archives differ only by byte order),
//! so the object graph converts by swapping each field with the exact claim
//! map from `Port_Swap.c` (`Claim_Collision`, `Claim_CollisionBSP`,
//! `Claim_Surface`, `Claim_Sound`).
//!
//! What actually changes:
//! * the prototype keeps its 8 terrain textures and 9 ground sounds inside
//!   the pack (reloc values pack-local, 0..8); the retail GameCube collision
//!   takes its art from the shared pack. We keep the prototype's inline art -
//!   `DataPack_RefreshTextures`/`RefreshPtrs` resolve pack-local reloc
//!   values through the pack's own id tables - so the textures are
//!   re-encoded (CMPR / RGBA8, GameCube tone) and the sounds decoded from
//!   VAG and re-stored as big-endian PCM16.
//! * the prototype's indexed textures need palettes; the GameCube pack format
//!   has none, and the surfaces' skid-texture ids are all zero, so the
//!   prototype's texture headers, pixels and palettes are dropped and the
//!   re-encoded textures are appended as fresh GameCube texture blocks.
//! * the prototype's sound records use its own voice categories; the retail
//!   remaps them (same table as the car converter).

use anyhow::{bail, ensure, Context, Result};

use crate::blob::{Blob, SwapMap};
use crate::convert::{sound, texture};
use crate::format::{align, Endian, Platform};
use crate::pack::{PackHeader, SplitPack, StdHeader};

const LE: Endian = Endian::Little;

/// Node / cell / edge layout, from `Claim_Collision*` in `Port_Swap.c`.
const CELL_LEN: u32 = 0x50;
const NODE_LEN: u32 = 0x20;
const EDGE_LEN: u32 = 0x30;
const POLY_LEN: u32 = 0x20;

/// Everything the converter needs besides the prototype pack.
pub struct Options<'a> {
    /// GameCube retail collision pack: supplies the pack references.
    pub donor: &'a SplitPack,
    /// Target build (GameCube).
    pub platform: Platform,
}

/// What happened, for the user.
#[derive(Default, Debug)]
pub struct Report {
    pub cells: usize,
    pub nodes: usize,
    pub edges: usize,
    pub materials: usize,
    pub textures: usize,
    pub sounds: usize,
    pub sound_fixes: usize,
    pub notes: Vec<String>,
}

fn std_header(sp: &SplitPack) -> Result<&StdHeader> {
    match &sp.json.header {
        PackHeader::Standard(h) => Ok(h),
        PackHeader::Quad(_) => bail!("expected a collision pack, found a geometry pack"),
    }
}

/// The layout map plus the structure counts the walk found, mirroring
/// `Claim_Collision`: every u16 field in the grid is listed there (BSP child
/// indices, edge index arrays, surface fields, texture reloc slots);
/// everything else is a 32-bit word.
pub struct Walk {
    swap: SwapMap,
    pub cells: usize,
    pub nodes: usize,
    pub edges: usize,
    pub materials: usize,
    texture_reloc_slots: Vec<u32>,
    sound_records: Vec<u32>,
}

/// The 16-bit field at `off`, read in the pack's own byte order (`Blob::u32`
/// only reads whole words, and a u16 sits in a different half depending on
/// endianness).
fn u16_at(data: &[u8], e: Endian, off: u32) -> Option<u32> {
    let o = off as usize;
    let b = data.get(o..o + 2)?;
    Some(match e {
        Endian::Little => u16::from_le_bytes([b[0], b[1]]) as u32,
        Endian::Big => u16::from_be_bytes([b[0], b[1]]) as u32,
    })
}

/// The layout map, exposed so tools can walk a converted pack again and check
/// the counts came out the same.
pub fn walk(blob: &Blob, e: Endian) -> Result<Walk> {
    let data = &blob.data;
    let mut w = Walk {
        swap: SwapMap::new(data.len()),
        cells: 0,
        nodes: 0,
        edges: 0,
        materials: 0,
        texture_reloc_slots: Vec::new(),
        sound_records: Vec::new(),
    };
    let word = |off: u32| blob.u32(e, off);
    let half = |off: u32| u16_at(data, e, off);
    // A pointer field of 0 (or off the end) means "none", as `Target` in
    // `Port_Swap.c` treats it.
    let ptr = |off: u32| word(off).filter(|&v| v != 0 && (v as usize) < data.len());

    let cells = ptr(0x20).unwrap_or(!0);
    let index = ptr(0x40).unwrap_or(!0);
    let polys = ptr(0x4C);
    let all_edges = ptr(0x54).unwrap_or(!0);
    let materials = ptr(0x5C).unwrap_or(!0);
    let total = word(0x34)
        .unwrap_or(0)
        .wrapping_mul(word(0x38).unwrap_or(0))
        .wrapping_mul(word(0x3C).unwrap_or(0));

    // The cell index table is one u32 per grid cell; the highest entry is the
    // number of real cell records.
    let mut num_cells = 0u32;
    for i in 0..total.min(0x100_000) {
        if index == !0 {
            break;
        }
        if let Some(c) = word(index.wrapping_add(i * 4)) {
            if c < 0x10000 && c + 1 > num_cells {
                num_cells = c + 1;
            }
        }
    }

    let mut max_material = 0u32;
    for i in 0..num_cells {
        let cell = cells.wrapping_add(i * CELL_LEN);
        let Some(_) = blob.u32(e, cell + CELL_LEN - 4) else { break };
        w.cells += 1;

        // BSP nodes: 0x20 each, child indices are u16 (0xFFFF ends a branch).
        if let Some(nodes) = ptr(cell + 0x30) {
            let mut stack = vec![0u32];
            while let Some(ni) = stack.pop() {
                let node = nodes.wrapping_add(ni * NODE_LEN);
                let (Some(_), Some(_)) = (half(node + 0x14), half(node + 0x16)) else { continue };
                if w.swap.kinds[node as usize + 0x14] == crate::blob::Kind::Half {
                    continue;
                }
                w.swap.half(node + 0x14);
                w.swap.half(node + 0x16);
                w.nodes += 1;
                if let Some(m) = word(node + 0x10) {
                    if m < 0x1000 && m + 1 > max_material {
                        max_material = m + 1;
                    }
                }
                for k in 0..2 {
                    if let Some(c) = half(node + 0x14 + k * 2) {
                        if c != 0xFFFF {
                            stack.push(c);
                        }
                    }
                }
            }
        }

        // Edge indices: u16 array; each edge record's material word feeds the
        // material count.
        if let (Some(edges), ne) = (ptr(cell + 0x44), word(cell + 0x40).unwrap_or(0)) {
            for j in 0..ne.min(0x10000) {
                let at = edges.wrapping_add(j * 2);
                let Some(ei) = half(at) else { break };
                w.swap.half(at);
                w.edges += 1;
                if all_edges != !0 {
                    if let Some(m) = word(all_edges.wrapping_add(ei * EDGE_LEN).wrapping_add(0x24)) {
                        if m < 0x1000 && m + 1 > max_material {
                            max_material = m + 1;
                        }
                    }
                }
            }
        }

        // Polygon indices: u32 array (words: the default swap handles them).
        // No shipped build has a polygon table (`polys` is null); the loop is
        // there so a build that does still contributes its material ids.
        if let (Some(cp), np, Some(polys)) = (ptr(cell + 0x38), word(cell + 0x3C).unwrap_or(0), polys) {
            for j in 0..np.min(0x100_000) {
                let Some(pi) = word(cp.wrapping_add(j * 4)) else { break };
                if let Some(m) = word(polys.wrapping_add(pi * POLY_LEN).wrapping_add(0x1C)) {
                    if m < 0x1000 && m + 1 > max_material {
                        max_material = m + 1;
                    }
                }
            }
        }
    }

    // Surface structs: two flag bytes, a u16 skid texture, then an f32.
    if materials != !0 {
        for i in 0..max_material {
            if let Some(surf) = ptr(materials.wrapping_add(i * 4)) {
                if blob.u32(e, surf + 4).is_some() {
                    w.swap.bytes(surf, 2);
                    w.swap.half(surf + 2);
                }
            }
        }
    }
    w.materials = max_material as usize;

    // Sound records (Sound struct): the first 8 bytes are four u8 fields; the
    // reloc slot is record + 0x14.
    for r in &blob.sound_relocs {
        if let Some(rec) = r.slot.checked_sub(0x14) {
            if blob.u32(e, rec + 4).is_some() {
                w.swap.bytes(rec, 8);
                w.sound_records.push(rec);
            }
        }
    }
    // Texture reloc slots hold u16 pairs.
    for r in &blob.texture_relocs {
        if blob.u32(e, r.slot).is_some() {
            w.swap.half(r.slot);
            w.swap.half(r.slot + 2);
            w.texture_reloc_slots.push(r.slot);
        }
    }
    Ok(w)
}

pub fn convert(beta: &SplitPack, opt: &Options) -> Result<(SplitPack, Report)> {
    let mut report = Report::default();
    let target = opt.platform;
    ensure!(target == Platform::Gc, "world collision packs convert to GameCube only");
    let te = target.endian();

    let h = std_header(beta)?.clone();
    ensure!(h.num_triggers == 0 && h.trigger_offset == 0, "collision packs with triggers are not supported");
    let mut blob = Blob::from_split(LE, beta);

    // ---- 1. map the layout, read the art that gets rebuilt --------------------
    let w = walk(&blob, LE)?;
    report.cells = w.cells;
    report.nodes = w.nodes;
    report.edges = w.edges;
    report.materials = w.materials;

    let mut images = Vec::new();
    for i in 0..h.num_textures as usize {
        let th = texture::BetaTexHeader::read(&blob.data, h.texture_data_offset as usize + i * texture::BETA_HEADER_LEN)?;
        let img = texture::decode_beta(&blob.data, &th, h.palette_offset as usize).with_context(|| format!("texture {i}"))?;
        images.push((th, img));
    }
    let region = beta.sections.get("sound_data").cloned().unwrap_or_default();
    let sounds = if h.num_sounds > 0 {
        sound::read_beta_sounds(&region, h.sfx_offset as usize, h.num_sounds as usize)?
    } else {
        vec![]
    };

    // ---- 2. byte-swap the object graph ----------------------------------------
    if te != LE {
        w.swap.swap(&mut blob.data);
    }

    // ---- 3. drop the prototype textures and palettes ---------------------------
    let le_view = Blob::from_split(LE, beta);
    let mut dead: Vec<(u32, u32)> = vec![];
    if h.num_textures > 0 {
        dead.push((h.texture_data_offset, h.texture_data_offset + h.num_textures * texture::BETA_HEADER_LEN as u32));
        for i in 0..h.num_textures as usize {
            let th = texture::BetaTexHeader::read(&le_view.data, h.texture_data_offset as usize + i * texture::BETA_HEADER_LEN)?;
            dead.push((th.pixels, th.pixels + th.stored_len() as u32));
        }
    }
    if h.num_palettes > 0 {
        dead.push((h.palette_offset, h.palette_offset + h.num_palettes * texture::PALETTE_LEN as u32));
    }
    for &(a, b) in &dead {
        blob.kill(a, b);
    }

    // ---- 4. append the GameCube textures (prototype order, pack-local ids) ----
    let mut tex = Blob::default();
    let count = images.len();
    tex.data.resize(count * texture::GC_HEADER_SLOT, 0);
    for (i, (th, img)) in images.iter().enumerate() {
        let slot = (i * texture::GC_HEADER_SLOT) as u32;
        let img = texture::gc_tone(img);
        let fmt = texture::choose_format(&img, "");
        let levels = match fmt {
            texture::GcFormat::Cmpr => texture::cmpr_levels(&img, th.mips),
            texture::GcFormat::Rgba8 => 1,
        };
        let enc = texture::encode_gc(&img, fmt, levels)?;
        let px = tex.append(&enc.pixels, 32);
        tex.data[slot as usize..slot as usize + texture::GC_HEADER_SLOT].copy_from_slice(&enc.header(px));
        tex.pointers.insert(slot + 8);
        report.textures += 1;
    }
    let tex_at = if count > 0 { blob.graft(te, &tex, 32)? } else { 0 };

    // The texture/sound relocs keep the prototype's pack-local values (0..n):
    // the GameCube loader resolves high16 == 0 through the pack's own
    // texture / sound id tables. The u16 pairs stored at the slots are zero
    // in every build; force it (the loader rewrites them at runtime anyway).
    for slot in w.texture_reloc_slots {
        for k in 0..4 {
            blob.data[(slot + k) as usize] = 0;
        }
    }

    // ---- 5. sound records: prototype voice categories -> retail ones ----------
    // Same remap the car converter uses (matched over both builds' cars and
    // the retail collision packs). The byte at +2 is 0 exactly for category
    // 9 in all 18 GameCube / PS2-retail collision records.
    let mut sound_fixes = 0;
    for rec in w.sound_records {
        let (t, c, b2) = (rec as usize, rec as usize + 1, rec as usize + 2);
        if blob.data[t] == 2 {
            blob.data[t] = 1;
            sound_fixes += 1;
        }
        let cat = match blob.data[c] {
            1 => 10,
            5 => 6,
            2 => 9,
            10 => 1,
            v => v,
        };
        if cat != blob.data[c] {
            blob.data[c] = cat;
            sound_fixes += 1;
        }
        let want = if cat == 9 { 0 } else { blob.data[b2] };
        if want != blob.data[b2] {
            blob.data[b2] = want;
            sound_fixes += 1;
        }
    }
    report.sound_fixes = sound_fixes;
    report.sounds = sounds.len();

    // ---- 6. header -------------------------------------------------------------
    blob.data.resize(align(blob.data.len(), 32), 0);
    blob.validate(te)?;
    for &s in &blob.pointers {
        let t = blob.u32(te, s).unwrap();
        if dead.iter().any(|&(a, b)| t >= a && t < b) {
            bail!("pointer at {s:#x} still aims into dropped data at {t:#x}");
        }
    }

    let (snd_region, snd_ptrs) = sound::build_gc_region(&sounds)?;

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
        hh.refs.clone_from(&std_header(opt.donor)?.refs);
        for r in hh.refs.iter_mut() {
            r.name_raw = Default::default();
            r.runtime_slot = 0;
        }
    }
    out.describe(target);
    // must survive the target's pack layout
    let bytes = crate::pack::join(target, &out)?;
    crate::pack::split(target, crate::format::PackType(5), &bytes)?;
    if w.cells == 0 {
        report.notes.push("the grid walk found no cells".into());
    }
    Ok((out, report))
}
