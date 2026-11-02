//! Lowering: project + target -> archive plans with resolved pack bytes.
//!
//! The IR is deliberately tiny: an archive is a list of resolved packs
//! (bytes plus a key), produced from a `BaseRef` (the vendor packs, taken
//! from an extracted archive folder) and explicit `PackSource` entries
//! appended after it. Vendor-first: everything a build needs today comes
//! from extracted packs or raw blobs; codegen kinds report `E0040`.

use crate::compiler::diag::{codes, Diagnostics};
use crate::compiler::project::{ArchiveDecl, BaseRef, PackSource};
use crate::compiler::target::{Container, Target};
use crate::format::{PackType, Platform, TYPE_GEOMETRY, VERSION_RELEASE};
use crate::pack::{self, PackJson, SplitPack};
use crate::{Manifest, MANIFEST_FILE, PACK_FILE};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Where a resolved pack came from (for reports and diagnostics).
#[derive(Clone, Debug, Serialize)]
pub struct SourceId {
    /// `"base"`, `"hwvx_pack"`, `"raw"`, ...
    pub kind: String,
    /// Path or base key, relative to the project.
    pub detail: String,
}

/// Coarse pack families used by later lowering passes (SwapMap, codegen).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PackKind {
    Cars,
    World,
    Geometry,
    Interface,
    Generic(u32),
}

impl PackKind {
    pub fn from_type(t: PackType) -> PackKind {
        match t.name() {
            Some("cars") => PackKind::Cars,
            Some("world") => PackKind::World,
            Some("geometry") => PackKind::Geometry,
            Some("interface") => PackKind::Interface,
            _ => PackKind::Generic(t.0),
        }
    }
    /// Does this key imply a quad (geometry) pack layout?
    pub fn is_quad(&self) -> bool {
        matches!(self, PackKind::Geometry)
    }
}

/// A pack ready to be written into an archive.
#[derive(Clone, Debug)]
pub struct ResolvedPack {
    pub kind: PackKind,
    pub type_id: u32,
    pub id: u32,
    pub sub_id: u32,
    pub version: u32,
    /// Complete pack bytes in the target's byte order.
    pub data: Vec<u8>,
    pub source: SourceId,
    /// File name (xarchive) or TOC name (xnc) override.
    pub output: Option<String>,
}

impl ResolvedPack {
    pub fn ptype(&self) -> PackType {
        PackType(self.type_id)
    }
    /// The key this pack takes in the archive TOC.
    pub fn key(&self) -> (u32, u32, u32) {
        (self.type_id, self.id, self.sub_id)
    }
    pub fn key_string(&self) -> String {
        key_string(self.type_id, self.id, self.sub_id)
    }
}

pub fn key_string(type_id: u32, id: u32, sub_id: u32) -> String {
    let t = PackType(type_id);
    if type_id == TYPE_GEOMETRY {
        format!("{}:{}:q{}_g{}", t, id, sub_id & 0xFFFF, sub_id >> 16)
    } else {
        format!("{}:{}", t, id)
    }
}

/// Report name of a type id: `cars`, or `type53` when unnamed.
pub fn key_type_name(type_id: u32) -> String {
    PackType(type_id).to_string()
}

/// One archive, lowered for one target: everything a backend needs.
#[derive(Clone, Debug)]
pub struct ArchivePlan {
    /// Output base name, without extension.
    pub output: String,
    pub target_name: String,
    pub platform: Platform,
    pub container: Container,
    pub packs: Vec<ResolvedPack>,
}

/// Lower one archive declaration for one target.
pub fn lower_archive(
    project_dir: &Path,
    decl: &ArchiveDecl,
    target: &Target,
    diags: &mut Diagnostics,
) -> Option<ArchivePlan> {
    let mut packs: Vec<ResolvedPack> = Vec::new();
    if let Some(base) = &decl.base {
        packs.extend(load_base(project_dir, base, target.platform, diags));
    }
    for (i, src) in decl.packs.iter().enumerate() {
        let where_ = format!("archives[{}].packs[{}]", decl.output, i);
        if let Some(rp) = source_pack(project_dir, src, target.platform, &where_, diags) {
            packs.push(rp);
        }
    }
    if packs.is_empty() {
        diags.error(
            codes::MISSING_FILE,
            format!("archive {:?} resolves to no packs", decl.output),
        );
        return None;
    }
    Some(ArchivePlan {
        output: decl.output.clone(),
        target_name: target.name.clone(),
        platform: target.platform,
        container: target.container,
        packs,
    })
}

/// Read the base folder: manifest, then each pack (split folder or raw
/// file), skipping excluded keys. Entries keep their order and version.
pub fn load_base(
    project_dir: &Path,
    base: &BaseRef,
    platform: Platform,
    diags: &mut Diagnostics,
) -> Vec<ResolvedPack> {
    let dir = project_dir.join(&base.from);
    let text = match std::fs::read(dir.join(MANIFEST_FILE)) {
        Ok(t) => t,
        Err(e) => {
            diags.error(
                codes::INVALID_BASE,
                format!("cannot read {}/{}: {e}", base.from, MANIFEST_FILE),
            );
            return Vec::new();
        }
    };
    let manifest: Manifest = match crate::parse_manifest(&text) {
        Ok(m) => m,
        Err(e) => {
            diags.error(
                codes::INVALID_BASE,
                format!("invalid manifest in {:?}: {e:#}", base.from),
            );
            return Vec::new();
        }
    };
    if manifest.platform != platform {
        diags.error(
            codes::INVALID_BASE,
            format!(
                "base {:?} is a {} archive but the target is {}; cross-platform vendor reuse is planned for M3 (SwapMap lowering)",
                base.from, manifest.platform, platform
            ),
        );
        return Vec::new();
    }
    let excludes: Vec<String> = base.exclude.clone();
    let mut out = Vec::new();
    for m in &manifest.entries {
        if excludes.iter().any(|x| exclude_matches(x, m)) {
            continue;
        }
        if let Some(data) = read_base_pack(platform, project_dir, &base.from, m, diags) {
            let detail = format!("{}/{}", base.from, m.path);
            out.push(ResolvedPack {
                kind: PackKind::from_type(m.kind),
                type_id: m.kind.0,
                id: m.id,
                sub_id: m.sub_id,
                version: m.version,
                data,
                source: SourceId {
                    kind: "base".into(),
                    detail,
                },
                output: None,
            });
        }
    }
    out
}

/// Rebuild one base pack's bytes (the same join `hwvx-x pack` performs).
fn read_base_pack(
    platform: Platform,
    project_dir: &Path,
    base_from: &str,
    m: &crate::ManifestEntry,
    diags: &mut Diagnostics,
) -> Option<Vec<u8>> {
    let rel = |p: String| format!("{base_from}/{p}");
    // Manifest paths are relative to the manifest's own folder, like
    // `hwvx-x extract` writes them.
    let base_dir = project_dir.join(base_from);
    match m.layout {
        crate::Layout::Raw => match std::fs::read(base_dir.join(&m.path)) {
            Ok(b) => Some(b),
            Err(e) => {
                diags.error(
                    codes::MISSING_FILE,
                    format!("raw pack {}: {e}", rel(m.path.clone())),
                );
                None
            }
        },
        crate::Layout::Split => {
            let json_path = base_dir.join(format!("{}/{PACK_FILE}", m.path));
            let text = match std::fs::read(&json_path) {
                Ok(t) => t,
                Err(e) => {
                    diags.error(
                        codes::MISSING_FILE,
                        format!("cannot read {}: {e}", rel(format!("{}/{PACK_FILE}", m.path))),
                    );
                    return None;
                }
            };
            let json: PackJson = match serde_json::from_slice(&text) {
                Ok(j) => j,
                Err(e) => {
                    diags.error(
                        codes::INVALID_BASE,
                        format!("invalid {}: {e}", rel(format!("{}/{PACK_FILE}", m.path))),
                    );
                    return None;
                }
            };
            let quad = matches!(json.header, pack::PackHeader::Quad(_));
            if quad != (m.kind.0 == TYPE_GEOMETRY) {
                diags.error(
                    codes::INVALID_BASE,
                    format!(
                        "{}: pack.json layout does not match manifest type {}",
                        rel(m.path.clone()),
                        m.kind
                    ),
                );
                return None;
            }
            let mut sections = BTreeMap::new();
            for d in pack::section_order(platform, quad) {
                let p = base_dir.join(format!("{}/{}.bin", m.path, d.name));
                if let Ok(b) = std::fs::read(&p) {
                    sections.insert(d.name.to_string(), b);
                }
            }
            match pack::join(platform, &SplitPack { json, sections }) {
                Ok(b) => Some(b),
                Err(e) => {
                    diags.error(
                        codes::INVALID_BASE,
                        format!("cannot join {:?}: {e:#}", rel(m.path.clone())),
                    );
                    None
                }
            }
        }
    }
}

/// Resolve one explicit pack source.
pub fn source_pack(
    project_dir: &Path,
    src: &PackSource,
    platform: Platform,
    where_: &str,
    diags: &mut Diagnostics,
) -> Option<ResolvedPack> {
    match src {
        PackSource::HwvxPack { path, ptype, id, sub_id, version, output } => {
            let dir = project_dir.join(path);
            let json_path = dir.join(PACK_FILE);
            let text = match std::fs::read(&json_path) {
                Ok(t) => t,
                Err(e) => {
                    diags.error(
                        codes::MISSING_FILE,
                        format!("cannot read {}: {e}", json_path.display()),
                    );
                    return None;
                }
            };
            let json: PackJson = match serde_json::from_slice(&text) {
                Ok(j) => j,
                Err(e) => {
                    diags.error(
                        codes::INVALID_BASE,
                        format!("invalid {}: {e}", json_path.display()),
                    );
                    return None;
                }
            };
            let stem = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let (auto_type, auto_id, auto_sub) = match parse_stem(&stem) {
                Some(v) => v,
                None => match (ptype, id) {
                    (Some(t), Some(i)) => (*t, *i, sub_id.unwrap_or(0)),
                    _ => {
                        diags
                            .error(
                                codes::UNRESOLVABLE_KEY,
                                format!("folder stem {stem:?} does not encode type/id; set \"type\" and \"id\""),
                            )
                            .at(where_.to_string());
                        return None;
                    }
                },
            };
            let final_type = ptype.unwrap_or(auto_type);
            let final_id = id.unwrap_or(auto_id);
            let final_sub = sub_id.unwrap_or(auto_sub);
            let quad = matches!(json.header, pack::PackHeader::Quad(_));
            let mut sections = BTreeMap::new();
            for d in pack::section_order(platform, quad) {
                if let Ok(b) = std::fs::read(dir.join(format!("{}.bin", d.name))) {
                    sections.insert(d.name.to_string(), b);
                }
            }
            let data = match pack::join(platform, &SplitPack { json, sections }) {
                Ok(b) => b,
                Err(e) => {
                    diags
                        .error(
                            codes::INVALID_BASE,
                            format!("cannot join {:?}: {e:#}", dir),
                        )
                        .at(where_.to_string());
                    return None;
                }
            };
            Some(ResolvedPack {
                kind: PackKind::from_type(final_type),
                type_id: final_type.0,
                id: final_id,
                sub_id: final_sub,
                version: version.unwrap_or(VERSION_RELEASE),
                data,
                source: SourceId { kind: "hwvx_pack".into(), detail: path.clone() },
                output: output.clone(),
            })
        }
        PackSource::Raw { path, ptype, id, sub_id, version, output } => {
            let file = project_dir.join(path);
            let data = match std::fs::read(&file) {
                Ok(b) => b,
                Err(e) => {
                    diags.error(codes::MISSING_FILE, format!("cannot read {}: {e}", file.display()));
                    return None;
                }
            };
            Some(ResolvedPack {
                kind: PackKind::from_type(*ptype),
                type_id: ptype.0,
                id: *id,
                sub_id: *sub_id,
                version: version.unwrap_or(VERSION_RELEASE),
                data,
                source: SourceId { kind: "raw".into(), detail: path.clone() },
                output: output.clone(),
            })
        }
        PackSource::Interface { .. }
        | PackSource::Car { .. }
        | PackSource::Items { .. }
        | PackSource::World { .. }
        | PackSource::Geometry { .. }
        | PackSource::Audio { .. } => {
            let name = source_kind_name(src);
            let milestone = match name {
                "car" | "items" => "roadmap M2",
                "world" | "geometry" | "audio" => "roadmap M4",
                _ => "the roadmap",
            };
            diags
                .error(
                    codes::NOT_IMPLEMENTED,
                    format!(
                        "pack source kind {name:?} is not implemented yet ({milestone}); use hwvx_pack or raw for now"
                    ),
                )
                .at(where_.to_string());
            None
        }
    }
}

/// `"car"`, `"hwvx_pack"`, ... for diagnostics and reports.
pub fn source_kind_name(src: &PackSource) -> &'static str {
    match src {
        PackSource::HwvxPack { .. } => "hwvx_pack",
        PackSource::Raw { .. } => "raw",
        PackSource::Interface { .. } => "interface",
        PackSource::Car { .. } => "car",
        PackSource::Items { .. } => "items",
        PackSource::World { .. } => "world",
        PackSource::Geometry { .. } => "geometry",
        PackSource::Audio { .. } => "audio",
    }
}

/// Parse a split-folder stem: `03_geometry_0_q2_g17` -> (geometry, 0,
/// `g<<16|q`), `00_world_0` -> (world, 0, 0). Returns `None` when the
/// shape is unknown (then the source must set `type`/`id` explicitly).
pub fn parse_stem(stem: &str) -> Option<(PackType, u32, u32)> {
    let (_index, rest) = stem.split_once('_')?;
    let parts: Vec<&str> = rest.split('_').collect();
    let (g, q, core): (Option<u32>, Option<u32>, &[&str]) = if parts.len() >= 4 {
        let g = parts[parts.len() - 1].strip_prefix('g')?.parse().ok()?;
        let q = parts[parts.len() - 2].strip_prefix('q')?.parse().ok()?;
        (Some(g), Some(q), &parts[..parts.len() - 2])
    } else {
        (None, None, parts.as_slice())
    };
    if core.len() != 2 {
        return None;
    }
    let ptype = PackType::from_name(core[0])?;
    let id: u32 = core[1].parse().ok()?;
    let sub = match (g, q) {
        (Some(g), Some(q)) => (q & 0xFFFF) | ((g & 0xFFFF) << 16),
        _ => 0,
    };
    Some((ptype, id, sub))
}

/// Accept `world:0`, `cars:3`, `geometry:2/q17`, `geometry:2/q17_g5`,
/// `interface`, or a folder stem, as an exclude pattern.
fn exclude_matches(pattern: &str, m: &crate::ManifestEntry) -> bool {
    let pattern = pattern.trim();
    // Folder stem (or raw file name), verbatim.
    if pattern == m.path {
        return true;
    }
    let (tname, rest) = match pattern.split_once(':') {
        Some(v) => v,
        // No colon: a bare type name (`interface`) excludes that whole
        // type; anything else was caught by the verbatim path check.
        None => (pattern, ""),
    };
    let Some(ptype) = PackType::from_name(tname) else {
        return false;
    };
    if ptype.0 != m.kind.0 {
        return false;
    }
    match rest.split_once('/') {
        Some((id_str, qs)) => {
            if id_str.parse::<u32>().ok() != Some(m.id) {
                return false;
            }
            let mut q = None;
            let mut g = None;
            for tok in qs.split('_') {
                if let Some(v) = tok.strip_prefix('q') {
                    q = v.parse::<u32>().ok();
                } else if let Some(v) = tok.strip_prefix('g') {
                    g = v.parse::<u32>().ok();
                }
            }
            q.is_none_or(|v| v & 0xFFFF == m.sub_id & 0xFFFF)
                && g.is_none_or(|v| v & 0xFFFF == m.sub_id >> 16)
        }
        None => match rest {
            // `type` alone matches every pack of that type.
            "" => true,
            // `type:id` matches every pack of that type and id (any quad).
            r => r.parse::<u32>().ok() == Some(m.id),
        },
    }
}
