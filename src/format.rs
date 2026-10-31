//! Platform, endianness and pack-type definitions shared by the whole crate.

use std::fmt;
use std::path::Path;

use anyhow::{bail, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// First word of every archive (read in the platform's byte order).
pub const MAGIC0: u32 = 0x0201_00A0;
/// Second word of every archive.
pub const MAGIC1: u32 = 0x0401_00AE;
/// Archive / pack version used by every retail release (PC, GC, PS2).
pub const VERSION_RELEASE: u32 = 183;

/// Size of the archive header (magic, magic, version, entry count).
pub const ARCHIVE_HEADER_LEN: usize = 16;
/// Size of one table-of-contents entry.
pub const TOC_ENTRY_LEN: usize = 24;

/// Pack type 7: a world quadrant's streamed geometry ("quad pack").
pub const TYPE_GEOMETRY: u32 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

impl Endian {
    pub fn u32_at(self, buf: &[u8], off: usize) -> Option<u32> {
        let b: [u8; 4] = buf.get(off..off.checked_add(4)?)?.try_into().ok()?;
        Some(match self {
            Endian::Little => u32::from_le_bytes(b),
            Endian::Big => u32::from_be_bytes(b),
        })
    }

    pub fn bytes(self, v: u32) -> [u8; 4] {
        match self {
            Endian::Little => v.to_le_bytes(),
            Endian::Big => v.to_be_bytes(),
        }
    }

    pub fn push(self, out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&self.bytes(v));
    }
}

/// Which build of the game an archive belongs to. Each has its own byte
/// order and alignment rules, so an archive is always rebuilt for the
/// platform it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    /// PC release (.xpc, little-endian)
    Pc,
    /// GameCube release (.xgc, big-endian)
    Gc,
    /// PS2 release (.xps, little-endian, 2048-byte sectors)
    Ps2,
    /// Leaked PS2 prototype (.xps, versions 177/179)
    Ps2Beta,
}

impl Platform {
    pub fn endian(self) -> Endian {
        match self {
            Platform::Gc => Endian::Big,
            _ => Endian::Little,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Platform::Pc => "xpc",
            Platform::Gc => "xgc",
            Platform::Ps2 | Platform::Ps2Beta => "xps",
        }
    }

    pub fn is_beta(self) -> bool {
        self == Platform::Ps2Beta
    }

    /// Alignment of packs in the archive (and of the archive's length).
    pub fn sector(self) -> usize {
        match self {
            Platform::Ps2 => 2048,
            _ => 1,
        }
    }

    /// Alignment of the data blob inside a quad (geometry) pack.
    pub fn quad_data_align(self) -> usize {
        match self {
            Platform::Gc => 32,
            _ => 16,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Platform::Pc => "PC",
            Platform::Gc => "GameCube",
            Platform::Ps2 => "PS2",
            Platform::Ps2Beta => "PS2 prototype",
        }
    }

    /// Work out the platform from an archive's bytes. Byte order and version
    /// are decisive; the PC and PS2 releases are told apart by where the
    /// first pack starts (PS2 puts it on a 2048-byte sector).
    pub fn detect(data: &[u8], path: Option<&Path>) -> Result<Platform> {
        let endian = if Endian::Little.u32_at(data, 0) == Some(MAGIC0) {
            Endian::Little
        } else if Endian::Big.u32_at(data, 0) == Some(MAGIC0) {
            Endian::Big
        } else {
            bail!("not a Velocity X archive (bad magic)");
        };
        let version = endian.u32_at(data, 8).unwrap_or(0);
        if endian == Endian::Big {
            if version != VERSION_RELEASE {
                bail!("big-endian archive with unexpected version {version}");
            }
            return Ok(Platform::Gc);
        }
        if version < VERSION_RELEASE {
            return Ok(Platform::Ps2Beta);
        }
        let count = endian.u32_at(data, 12).unwrap_or(0) as usize;
        if count > 0 {
            if let Some(first) = endian.u32_at(data, ARCHIVE_HEADER_LEN + 20) {
                let start = ARCHIVE_HEADER_LEN + count * TOC_ENTRY_LEN + first as usize;
                if first != 0 && start.is_multiple_of(2048) {
                    return Ok(Platform::Ps2);
                }
                return Ok(Platform::Pc);
            }
        }
        // Empty archive: fall back to the extension.
        let ext = path
            .and_then(|p| p.extension())
            .map(|e| e.to_string_lossy().to_ascii_lowercase());
        Ok(match ext.as_deref() {
            Some("xps") => Platform::Ps2,
            _ => Platform::Pc,
        })
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Is `path` one of the archive extensions (any case)?
pub fn is_archive_path(path: &Path) -> bool {
    matches!(
        path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).as_deref(),
        Some("xpc" | "xgc" | "xps")
    )
}

const TYPE_NAMES: [&str; 11] = [
    "car",
    "interface",
    "item",
    "link",
    "world",
    "collision",
    "world_texture",
    "geometry",
    "share",
    "audio",
    "music",
];

/// The type field of a TOC entry. Serialized by name ("world", "geometry"...)
/// so manifests are readable; unknown values are written as numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackType(pub u32);

impl PackType {
    pub fn name(self) -> Option<&'static str> {
        TYPE_NAMES.get(self.0 as usize).copied()
    }

    pub fn from_name(s: &str) -> Option<PackType> {
        TYPE_NAMES
            .iter()
            .position(|n| n.eq_ignore_ascii_case(s))
            .map(|i| PackType(i as u32))
            .or_else(|| s.parse().ok().map(PackType))
    }
}

impl fmt::Display for PackType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(n) => f.write_str(n),
            None => write!(f, "type{}", self.0),
        }
    }
}

impl Serialize for PackType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.name() {
            Some(n) => s.serialize_str(n),
            None => s.serialize_u32(self.0),
        }
    }
}

impl<'de> Deserialize<'de> for PackType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(u32),
            Name(String),
        }
        match Raw::deserialize(d)? {
            Raw::Num(n) => Ok(PackType(n)),
            Raw::Name(s) => PackType::from_name(&s)
                .ok_or_else(|| serde::de::Error::custom(format!("unknown pack type {s:?}"))),
        }
    }
}

pub fn align(v: usize, a: usize) -> usize {
    if a <= 1 {
        v
    } else {
        v.div_ceil(a) * a
    }
}

/// `stored` truncated or zero-extended to `len`: how saved padding bytes are
/// reused when a rebuilt section no longer needs the same amount.
pub fn fill(stored: &[u8], len: usize) -> Vec<u8> {
    let mut v = stored[..stored.len().min(len)].to_vec();
    v.resize(len, 0);
    v
}
