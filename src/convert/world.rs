//! Port a PS2-prototype world level (CH1_L2 beta) into the GameCube retail
//! world host: quad geometry packs, the world texture pack, and the world
//! pack's quadrant table that glues them together.
//!
//! The runtime finds quad packs through the world pack and the archive's
//! table of contents (`FileStreaming.c:538-552`): a quad pack is loaded for a
//! `WorldQuad` record when `entry id == quad packId` and the entry's high
//! 16 bits equal `quad numGeoms`.  The `pack` command writes the TOC from the
//! extract folder's manifest (`archive.rs`), so the manifest `sub_id` and the
//! world record must carry the same new pack id:
//!
//! * world record `WorldQuad` (0x50 bytes, GC): `numGeoms` @+0x0C, `packId`
//!   @+0x38;  `packId = (geom count << 16) | quad id` (quad id = low half of
//!   the GameCube pack id, i.e. the slot's quad index).
//! * geometry pack manifest `sub_id` = the same pack id.
//!
//! Because the prototype packs are paired with the GameCube packs slot by
//! slot (see `tests/orig_worlds.rs` - same quadrant order in both archives,
//! and the CH1_L2 archive lists them in that order), the conversion keeps
//! each GameCube record's quad id, replaces only the geom count and pack id
//! high half, and hands the GameCube geometry ids out to the rebuilt models
//! positionally so world trigger lookups keep working.
//!
//! The quad packs themselves convert with [`crate::convert::geometry`]
//! (`read_beta` + `build_gc`, identity texture remap: the prototype texture
//! ids are indexes into the world's own texture pack, and we rebuild that
//! pack from the same prototype textures in the same order).  The texture
//! pack is rebuilt GameCube-style: 0x20-byte texture blocks (little-endian
//! u16 fields, big-endian pixel pointer, per `Claim_Textures` in
//! `Port_Swap.c:162`), CMPR/RGBA8 pixels, and the GameCube pack's own tail
//! (its two shader entries, which the world pack's shader relocations point
//! at) grafted on with its internal pointers rebased.

use anyhow::{Context, Result, bail, ensure};

use crate::blob::{Blob, Reloc};
use crate::convert::{geometry, texture};
use crate::format::{Endian, Platform, align};
use crate::pack::{self, PackHeader, SplitPack, StdHeader};

const LE: Endian = Endian::Little;
const BE: Endian = Endian::Big;

/// Prototype texture header stride (`DataPack` v179 world textures).
const BETA_TEX_STRIDE: usize = 0x10;
/// GameCube texture header stride (BTF header + pad, `RefreshTextures`).
const GC_TEX_STRIDE: usize = 0x20;
/// `WorldQuad` record size in a v183 (GameCube) world pack.
const WORLD_QUAD_LEN: usize = 0x50;

fn std_header<'a>(sp: &'a SplitPack, what: &'a str) -> Result<&'a StdHeader> {
    match &sp.json.header {
        PackHeader::Standard(h) => Ok(h),
        PackHeader::Quad(_) => bail!("expected a {what} pack, found a geometry pack"),
    }
}

// ---------------------------------------------------------------------------
// world texture pack (type 6, `worlds_\tex_\w1_tex`)
// ---------------------------------------------------------------------------

/// What happened, for the user.
#[derive(Default, Debug)]
pub struct TexReport {
    pub textures: usize,
    pub palettes_dropped: usize,
    pub tail_bytes: usize,
    pub notes: Vec<String>,
}

/// Rebuild the GameCube world texture pack from the prototype's.
///
/// `beta` is `22_world_texture_0` from the CH1_L2 archive: 153 prototype
/// texture headers (0x10 stride) with their pixels and 45 palettes.
/// `donor` is the GameCube `00_world_texture_0`: supplies the pack layout -
/// the shader table at the base of the blob, its two shader entries at the
/// end (what the world pack's shader relocations resolve through), the pack
/// refs and the seven texture relocations the shader entries need.
///
/// The result is the same shader table and tail (with pointers and reloc
/// slots rebased), a fresh 0x20-stride block of 153 GameTexture headers
/// pointing at CMPR/RGBA8 pixels re-encoded from the prototype art, and no
/// palettes: the prototype indexes its textures into the new block by
/// position (identity), which is exactly what the converted quad packs'
/// texture relocations expect.
pub fn convert_textures(beta: &SplitPack, donor: &SplitPack) -> Result<(SplitPack, TexReport)> {
    let bh = std_header(beta, "world texture")?;
    let dh = std_header(donor, "world texture")?;
    let bb = Blob::from_split(LE, beta);
    let gb = Blob::from_split(BE, donor);

    // ---- 1. re-encode every prototype texture ------------------------------
    let count = bh.num_textures as usize;
    ensure!(count > 0, "prototype world texture pack has no textures");
    let mut encoded = Vec::with_capacity(count);
    for i in 0..count {
        let off = bh.texture_data_offset as usize + i * BETA_TEX_STRIDE;
        let hdr = texture::BetaTexHeader::read(&bb.data, off).with_context(|| format!("prototype texture {i}"))?;
        let img = texture::decode_beta(&bb.data, &hdr, bh.palette_offset as usize)
            .with_context(|| format!("prototype texture {i} pixels"))?;
        let img = texture::gc_tone(&img);
        let fmt = texture::choose_format(&img, &format!("w1_tex{i:03}"));
        let levels = texture::cmpr_levels(&img, hdr.mips);
        let t = texture::encode_gc(&img, fmt, levels).with_context(|| format!("encoding GameCube texture {i}"))?;
        encoded.push(t);
    }

    // ---- 2. new blob: shader table, texture blocks, pixels, donor tail ----
    let head_len = dh.texture_data_offset as usize;
    ensure!(head_len >= 0x20 && head_len % 4 == 0, "GameCube texture pack shader table is at {head_len:#x}");
    ensure!(
        gb.texture_relocs.iter().all(|r| r.slot as usize >= head_len),
        "GameCube world texture pack relocates inside its texture block area"
    );
    for list in [&gb.shader_relocs, &gb.sound_relocs, &gb.ref_relocs] {
        ensure!(list.is_empty(), "GameCube world texture pack has relocations this converter does not handle");
    }

    // The shader table's pointer slots aim at the shader entries at the end
    // of the blob; the tail starts at the first one they hit.
    let head_slots: Vec<u32> = gb.pointers.range(0..head_len as u32).copied().collect();
    ensure!(!head_slots.is_empty(), "GameCube world texture pack has no shader table pointers");
    let tail_at = head_slots
        .iter()
        .map(|&s| gb.u32(BE, s).with_context(|| format!("shader table pointer at {s:#x}")))
        .collect::<Result<Vec<u32>>>()?
        .into_iter()
        .min()
        .context("shader table has no pointers")? as usize;
    ensure!((tail_at + 0x20) < gb.data.len(), "GameCube texture pack tail is only {} bytes", gb.data.len() - tail_at);

    let mut nb = Blob::default();
    nb.data.extend_from_slice(&gb.data[..head_len]);
    let defs_at = nb.data.len() as u32; // the shader table region keeps the pack aligned
    let mut pix_at = align(defs_at as usize + count * GC_TEX_STRIDE, 32) as u32;
    let mut pix_offs = Vec::with_capacity(count);
    for t in &encoded {
        pix_offs.push(pix_at);
        pix_at = align(pix_at as usize + t.pixels.len(), 32) as u32;
    }
    let mut defs = Vec::with_capacity(count * GC_TEX_STRIDE);
    for (t, &po) in encoded.iter().zip(&pix_offs) {
        ensure!(t.header(po).len() == GC_TEX_STRIDE, "GameCube texture header is not {GC_TEX_STRIDE:#x} bytes");
        defs.extend_from_slice(&t.header(po));
    }
    ensure!(nb.append(&defs, 32) == defs_at, "texture blocks did not land at the shader table end");
    for (i, t) in encoded.iter().enumerate() {
        ensure!(nb.append(&t.pixels, 32) == pix_offs[i], "texture {i} pixels moved while packing");
        nb.set_ptr(BE, defs_at + (i * GC_TEX_STRIDE + 8) as u32, pix_offs[i]);
    }
    let tail_at_new = nb.append(&gb.data[tail_at..], 32);
    let delta = tail_at_new - tail_at as u32;

    // Rebase the pointer slots and reloc slots the new blob keeps: the
    // shader table's own pointers and everything inside the tail.
    for &s in &head_slots {
        let v = gb.u32(BE, s).context("shader table pointer")?;
        ensure!(v as usize >= tail_at, "shader table pointer at {s:#x} aims into replaced data at {v:#x}");
        nb.put_u32(BE, s, v + delta);
        nb.pointers.insert(s);
    }
    for &s in gb.pointers.range(tail_at as u32..) {
        let v = gb.u32(BE, s).with_context(|| format!("tail pointer at {s:#x}"))?;
        ensure!(v as usize >= tail_at, "tail pointer at {s:#x} aims into replaced data at {v:#x}");
        nb.put_u32(BE, s + delta, v + delta);
        nb.pointers.insert(s + delta);
    }
    for r in &gb.texture_relocs {
        ensure!(r.slot as usize >= tail_at, "GameCube texture reloc at {:#x} is inside replaced data", r.slot);
        nb.texture_relocs.push(Reloc { slot: r.slot + delta, value: r.value });
    }

    // ---- 3. the split pack -------------------------------------------------
    nb.validate(BE)?;
    let mut out = donor.clone();
    out.json.padding.clear();
    out.sections.clear();
    nb.store(BE, &mut out);
    if let PackHeader::Standard(h) = &mut out.json.header {
        h.num_textures = count as u32;
        h.texture_data_offset = defs_at;
        h.num_palettes = 0;
        h.palette_offset = 0;
    }
    out.describe(Platform::Gc);
    // must survive the GameCube pack layout
    let bytes = pack::join(Platform::Gc, &out)?;
    pack::split(Platform::Gc, crate::format::PackType(crate::format::TYPE_TEXTURE), &bytes)?;

    Ok((
        out,
        TexReport {
            textures: count,
            palettes_dropped: bh.num_palettes as usize,
            tail_bytes: gb.data.len() - tail_at,
            notes: vec![],
        },
    ))
}

// ---------------------------------------------------------------------------
// world quad geometry packs (type 7, `worlds_\geo_\w1_geo`)
// ---------------------------------------------------------------------------

/// What happened, for the user.
#[derive(Default, Debug)]
pub struct QuadReport {
    pub models: usize,
    pub texture_relocs: usize,
    pub new_geom_ids: usize,
    pub notes: Vec<String>,
}

/// The geometry ids a GameCube quad pack's table carries, in table order
/// (the first word of each 0xC-byte entry at the offset in word 0).
pub fn donor_geom_ids(donor: &SplitPack) -> Result<Vec<u32>> {
    ensure!(donor.is_quad(), "expected a GameCube geometry pack");
    let blob = Blob::from_split(BE, donor);
    let table = blob.u32(BE, 0).context("geometry pack has no table offset")? as usize;
    ensure!(blob.data.len() >= table, "geometry table at {table:#x} is past the data end");
    blob.ref_relocs
        .iter()
        .map(|r| {
            let entry = r.slot.checked_sub(8).context("geometry table reloc not on an entry")?;
            blob.u32(BE, entry).with_context(|| format!("geometry entry at {entry:#x}"))
        })
        .collect()
}

/// Prototype geometry heads in a quad pack's blob, in blob order.
///
/// This is the runtime's own walk (`Claim_GeometryScan`): the geometry magic
/// with its LOD-list and distance-list pointer slots registered in the pack's
/// pointer table.  The count it finds is what the world record's `numGeoms`
/// and the TOC's high half must say.
pub fn scan_heads(blob: &Blob) -> Vec<u32> {
    (0..blob.data.len().saturating_sub(0x40))
        .step_by(4)
        .map(|off| off as u32)
        .filter(|&off| geometry::is_beta_geometry(blob, off))
        .collect()
}

/// Convert one prototype quad pack into a GameCube one.
///
/// `beta` is the prototype `geometry_0_qNNNN` pack; `donor` is the GameCube
/// quad pack for the same world quadrant slot (supplies the pack refs and
/// json shape).  `geom_ids` must hold one id per model the scan finds, in
/// scan order; callers keep them stable by reusing the donor table's ids
/// positionally and allocating fresh ones above the GameCube high-water mark
/// (see [`donor_geom_ids`] and `new_geom_id_base`).
pub fn convert_quad(beta: &SplitPack, donor: &SplitPack, geom_ids: &[u32]) -> Result<(SplitPack, QuadReport)> {
    ensure!(beta.is_quad(), "expected a prototype geometry pack");
    ensure!(donor.is_quad(), "expected a GameCube geometry pack");
    let bb = Blob::from_split(LE, beta);
    let heads = scan_heads(&bb);
    ensure!(
        heads.len() == geom_ids.len(),
        "found {} prototype models, was given {} geometry ids",
        heads.len(),
        geom_ids.len()
    );

    // GameCube quad layout: 0x20-byte header (word 0 points at the table),
    // then one (geometry id, 0, data offset) entry per model, 0xC apart.  The
    // third word is a relocation slot (`texRelocs` in `DataPack_LoadQuadPack`,
    // `MarkSlots(ref_relocs stride 2)` in `PortSwap_QuadPack`); the file's
    // ref_relocs table carries the entry index beside it.  The table area is
    // reserved up front so grafted models start after it.
    let mut nb = Blob::default();
    nb.data.resize(align(0x20 + 12 * heads.len(), 32), 0);
    nb.set_ptr(BE, 0, 0x20);
    let mut report = QuadReport { models: heads.len(), ..Default::default() };
    for (k, &head) in heads.iter().enumerate() {
        let model = geometry::read_beta(&bb, head).with_context(|| format!("prototype model at {head:#x}"))?;
        // Identity texture remap: the converted w1_tex pack keeps the
        // prototype texture order, so the prototype ids stay valid.
        let gc = geometry::build_gc(&model, &mut |t| Some(t)).with_context(|| format!("building GameCube model at {head:#x}"))?;
        report.texture_relocs += gc.texture_relocs.len();
        let at = nb.graft(BE, &gc, 32).context("grafting model")?;
        let entry = 0x20 + 12 * k as u32;
        nb.put_u32(BE, entry, geom_ids[k]);
        nb.put_u32(BE, entry + 4, 0);
        nb.put_u32(BE, entry + 8, at);
        nb.ref_relocs.push(Reloc { slot: entry + 8, value: k as u32 });
    }

    nb.validate(BE)?;
    let mut out = donor.clone();
    out.json.padding.clear();
    out.sections.clear();
    nb.store(BE, &mut out);
    out.describe(Platform::Gc);
    let bytes = pack::join(Platform::Gc, &out)?;
    pack::split(Platform::Gc, crate::format::PackType(crate::format::TYPE_GEOMETRY), &bytes)?;

    Ok((out, report))
}

// ---------------------------------------------------------------------------
// world pack quadrant table (type 4, `ch1_l2` host)
// ---------------------------------------------------------------------------

/// A quadrant record to install: the new geom count and pack id (the record's
/// own quad id is kept).
#[derive(Debug)]
pub struct QuadPatch {
    pub num_geoms: u32,
    pub pack_id: u32,
}

/// The quad ids of a v183 world pack's quadrant records, in record order.
pub fn world_quad_ids(world: &SplitPack) -> Result<Vec<u32>> {
    let w = world.sections.get("data").context("world pack has no data.bin")?;
    let n = BE.u32_at(w, 0x20).context("world pack shorter than its tables")? as usize;
    let list = BE.u32_at(w, 0x24).context("world pack has no quad list")? as usize;
    ensure!(n > 0 && n < 0x100 && list > 0 && list + n * WORLD_QUAD_LEN <= w.len(), "bad world quad table {n} @ {list:#x}");
    Ok((0..n).map(|i| BE.u32_at(w, list + i * WORLD_QUAD_LEN).unwrap()).collect())
}

/// Point the GameCube world pack's quadrant records at the converted packs:
/// each record's `numGeoms` (+0x0C) and `packId` (+0x38) are replaced with
/// the values the converted archive's TOC will carry, so the streaming
/// loader's match (`FileStreaming.c:538-552`) finds the new geometry.
pub fn patch_world(world: &SplitPack, patches: &[QuadPatch]) -> Result<SplitPack> {
    std_header(world, "world")?;
    let mut out = world.clone();
    let w = out.sections.get_mut("data").context("world pack has no data.bin")?;
    let n = BE.u32_at(w, 0x20).context("world pack shorter than its tables")? as usize;
    let list = BE.u32_at(w, 0x24).context("world pack has no quad list")? as usize;
    ensure!(n == patches.len(), "world pack has {n} quad records, given {} patches", patches.len());
    ensure!(list > 0 && list + n * WORLD_QUAD_LEN <= w.len(), "bad world quad table {n} @ {list:#x}");
    for (i, p) in patches.iter().enumerate() {
        let r = list + i * WORLD_QUAD_LEN;
        let old = BE.u32_at(w, r + 0x38).unwrap();
        ensure!(
            p.pack_id & 0xFFFF == old & 0xFFFF && p.pack_id >> 16 == p.num_geoms,
            "quad {i} (pack id {old:#x}): patch pack id {:#x} / geoms {} is not self-consistent",
            p.pack_id,
            p.num_geoms
        );
        w[r + 0x0C..r + 0x10].copy_from_slice(&p.num_geoms.to_be_bytes());
        w[r + 0x38..r + 0x3C].copy_from_slice(&p.pack_id.to_be_bytes());
    }
    Ok(out)
}

/// Start allocating fresh geometry ids above a set of GameCube tables.
///
/// The prototype's ids (0x1400s in CH1_L2) collide with nothing the retail
/// GameCube build uses for other worlds' lookups *within this archive*, but
/// the retail ids appear in world trigger and progression data; keeping the
/// donor table's ids where the counts agree (which `cmd_port_world` does)
/// preserves those, and only genuinely new models need ids from here.
pub fn new_geom_id_base(all_donor_ids: &[u32]) -> u32 {
    all_donor_ids.iter().copied().max().map_or(0x1000, |m| m + 1)
}
