//! Extract and rebuild the `.xpc` / `.xgc` / `.xps` archives of
//! Hot Wheels: Velocity X (PC, GameCube, PS2 and the PS2 prototype).
//!
//! An extracted archive is a folder:
//!
//! ```text
//! manifest.json                 archive header + one record per pack, in order
//! 00_world_0/pack.json          pack header fields, pack references
//! 00_world_0/data.bin           ...and one .bin per section
//! 01_geometry_0_q2_g17/...
//! ```
//!
//! [`extract`] produces the files in memory and [`pack`] consumes them, so the
//! command line, the tests and `verify` all share the same code.

pub mod archive;
pub mod blob;
pub mod convert;
pub mod exepatch;
pub mod format;
pub mod gltf;
pub mod hex;
pub mod pack;
pub mod render;
pub mod roster;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use archive::{Archive, Entry};
use format::{PackType, Platform, TYPE_GEOMETRY};
use hex::Hex;
use pack::{PackJson, SplitPack};

pub const MANIFEST_FILE: &str = "manifest.json";
pub const PACK_FILE: &str = "pack.json";
pub const MANIFEST_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// A folder with pack.json and one .bin per section.
    Split,
    /// The pack's bytes as a single .bin file.
    Raw,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    #[serde(rename = "type")]
    pub kind: PackType,
    pub id: u32,
    /// Geometry: quad pack id | GX object count << 16. Zero otherwise.
    pub sub_id: u32,
    pub version: u32,
    /// Folder (split) or file (raw), relative to the manifest.
    pub path: String,
    pub layout: Layout,
    /// Human-readable description; ignored when packing.
    #[serde(default, skip_deserializing)]
    pub summary: String,
    /// Non-zero bytes found in the alignment gap before this pack.
    #[serde(default, skip_serializing_if = "Hex::is_empty")]
    pub gap_fill: Hex,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub tool: String,
    pub manifest_version: u32,
    /// File name the archive is rebuilt as.
    pub source_file: String,
    pub platform: Platform,
    pub archive_version: u32,
    /// Packs in file order. Reorder, remove or add records here.
    pub entries: Vec<ManifestEntry>,
    #[serde(default, skip_serializing_if = "Hex::is_empty")]
    pub tail_fill: Hex,
}

/// An archive turned into files (paths relative to the output folder, `/`
/// separated), ready to be written out.
pub struct Extracted {
    pub manifest: Manifest,
    pub files: Vec<(String, Vec<u8>)>,
    /// Packs that could not be split and were kept raw, with the reason.
    pub warnings: Vec<String>,
}

/// Folder / file stem for an entry: `03_geometry_0_q2_g17`, `00_world_0`.
pub fn entry_stem(index: usize, e: &Entry) -> String {
    if e.kind.0 == TYPE_GEOMETRY {
        format!("{index:02}_{}_{}_q{}_g{}", e.kind, e.id, e.sub_id & 0xFFFF, e.sub_id >> 16)
    } else {
        format!("{index:02}_{}_{}", e.kind, e.id)
    }
}

/// Split a pack and check that joining it gives back the same bytes.
pub fn split_checked(platform: Platform, e: &Entry) -> Result<SplitPack> {
    let mut sp = pack::split(platform, e.kind, &e.data)?;
    let back = pack::join(platform, &sp)?;
    if back != e.data {
        bail!("rebuilding the split pack does not reproduce it");
    }
    sp.describe(platform);
    Ok(sp)
}

pub fn extract(data: &[u8], platform: Platform, source_file: &str, raw: bool) -> Result<Extracted> {
    let archive = Archive::parse(data, platform)?;
    let mut files = Vec::new();
    let mut entries = Vec::new();
    let mut warnings = Vec::new();

    for (i, e) in archive.entries.iter().enumerate() {
        let stem = entry_stem(i, e);
        let split = if raw {
            None
        } else {
            match split_checked(platform, e) {
                Ok(sp) => Some(sp),
                Err(err) => {
                    warnings.push(format!("{stem}: kept raw ({err:#})"));
                    None
                }
            }
        };
        let (path, layout, summary) = match split {
            Some(sp) => {
                files.push((format!("{stem}/{PACK_FILE}"), json_bytes(&sp.json)?));
                for (name, bytes) in &sp.sections {
                    files.push((format!("{stem}/{name}.bin"), bytes.clone()));
                }
                (stem, Layout::Split, pack::summary(&sp))
            }
            None => {
                let file = format!("{stem}.bin");
                files.push((file.clone(), e.data.clone()));
                let summary = pack::split(platform, e.kind, &e.data)
                    .map(|sp| pack::summary(&sp))
                    .unwrap_or_else(|_| format!("{} bytes", e.data.len()));
                (file, Layout::Raw, summary)
            }
        };
        entries.push(ManifestEntry {
            kind: e.kind,
            id: e.id,
            sub_id: e.sub_id,
            version: e.version,
            path,
            layout,
            summary,
            gap_fill: Hex::nonzero(&e.gap),
        });
    }

    let manifest = Manifest {
        tool: format!("hwvx-xfile {}", env!("CARGO_PKG_VERSION")),
        manifest_version: MANIFEST_VERSION,
        source_file: source_file.to_string(),
        platform,
        archive_version: archive.version,
        entries,
        tail_fill: Hex::nonzero(&archive.tail),
    };
    files.insert(0, (MANIFEST_FILE.to_string(), json_bytes(&manifest)?));
    Ok(Extracted { manifest, files, warnings })
}

pub fn json_bytes<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    let mut s = serde_json::to_string_pretty(v)?;
    s.push('\n');
    Ok(s.into_bytes())
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(bytes).context("invalid manifest.json")?;
    if m.manifest_version > MANIFEST_VERSION {
        bail!(
            "manifest version {} is newer than this tool supports ({MANIFEST_VERSION})",
            m.manifest_version
        );
    }
    Ok(m)
}

/// Rebuild an archive. `read` fetches a file by its manifest-relative path
/// and returns `None` if it does not exist (a missing section is empty).
pub fn pack<F>(manifest: &Manifest, mut read: F) -> Result<Vec<u8>>
where
    F: FnMut(&str) -> Result<Option<Vec<u8>>>,
{
    let platform = manifest.platform;
    let mut entries = Vec::with_capacity(manifest.entries.len());
    for (i, m) in manifest.entries.iter().enumerate() {
        let ctx = || format!("entry {i} ({})", m.path);
        let data = match m.layout {
            Layout::Raw => read(&m.path)
                .with_context(ctx)?
                .with_context(|| format!("{} is missing", m.path))?,
            Layout::Split => {
                let json_path = format!("{}/{PACK_FILE}", m.path);
                let json = read(&json_path)?.with_context(|| format!("{json_path} is missing"))?;
                let json: PackJson =
                    serde_json::from_slice(&json).with_context(|| format!("invalid {json_path}"))?;
                let quad = matches!(json.header, pack::PackHeader::Quad(_));
                if quad != (m.kind.0 == TYPE_GEOMETRY) {
                    bail!("{json_path}: layout does not match pack type {}", m.kind);
                }
                let mut sections = std::collections::BTreeMap::new();
                for d in pack::section_order(platform, quad) {
                    if let Some(b) = read(&format!("{}/{}.bin", m.path, d.name))? {
                        sections.insert(d.name.to_string(), b);
                    }
                }
                pack::join(platform, &SplitPack { json, sections }).with_context(ctx)?
            }
        };
        entries.push(Entry {
            version: m.version,
            kind: m.kind,
            id: m.id,
            sub_id: m.sub_id,
            data,
            gap: m.gap_fill.0.clone(),
        });
    }
    Archive { platform, version: manifest.archive_version, entries, tail: manifest.tail_fill.0.clone() }.build()
}

/// Extract and rebuild entirely in memory (JSON included) and report
/// whether the original bytes come back. Returns the number of packs that
/// had to stay raw.
pub fn verify(data: &[u8], platform: Platform, source_file: &str) -> Result<usize> {
    let ex = extract(data, platform, source_file, false)?;
    let files: std::collections::HashMap<&str, &Vec<u8>> =
        ex.files.iter().map(|(p, b)| (p.as_str(), b)).collect();
    let manifest = parse_manifest(files[MANIFEST_FILE])?;
    let rebuilt = pack(&manifest, |p| Ok(files.get(p).map(|b| (*b).clone())))?;
    if rebuilt != data {
        let at = rebuilt.iter().zip(data).position(|(a, b)| a != b).unwrap_or(rebuilt.len().min(data.len()));
        bail!(
            "rebuilt archive differs (first difference at {at:#x}; sizes {} vs {})",
            rebuilt.len(),
            data.len()
        );
    }
    Ok(ex.warnings.len())
}
