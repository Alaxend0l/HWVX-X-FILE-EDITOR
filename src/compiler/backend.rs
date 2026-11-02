//! Backends: write an `ArchivePlan` as `.xpc`/`.xgc`/`.xps` or `.xnc`.

use crate::archive::{Archive, Entry};
use crate::compiler::diag::{codes, Diagnostics};
use crate::compiler::ir::ArchivePlan;
use crate::format::{PackType, VERSION_RELEASE};
use crate::xnc::{XncEntry, XncFile};
use anyhow::Result;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Write the plan's archive; returns the output path. An existing file is
/// refused unless `force` (diagnostic `E0033`).
pub fn emit(plan: &ArchivePlan, out_dir: &Path, force: bool, diags: &mut Diagnostics) -> Result<Option<PathBuf>> {
    let ext = plan.container.extension(plan.platform);
    let file = format!("{}.{}", plan.output, ext);
    let path = out_dir.join(&file);
    if path.exists() && !force {
        diags.error(
            codes::OUTPUT_EXISTS,
            format!("{} exists; pass --force to overwrite", path.display()),
        );
        return Ok(None);
    }
    let bytes = match plan.container {
        crate::compiler::target::Container::XArchive => build_xarchive(plan)?,
        crate::compiler::target::Container::Xnc => build_xnc(plan)?,
    };
    std::fs::create_dir_all(out_dir)?;
    std::fs::write(&path, &bytes)?;
    Ok(Some(path))
}

/// Classic archive the game reads: TOC + packs in target byte order.
pub fn build_xarchive(plan: &ArchivePlan) -> Result<Vec<u8>> {
    let entries = plan
        .packs
        .iter()
        .map(|p| Entry {
            version: p.version,
            kind: PackType(p.type_id),
            id: p.id,
            sub_id: p.sub_id,
            data: p.data.clone(),
            gap: Vec::new(),
        })
        .collect();
    Archive {
        platform: plan.platform,
        version: VERSION_RELEASE,
        entries,
        tail: Vec::new(),
    }
    .build()
}

/// `XNC` next container (docs/compiler/custom-format.md).
pub fn build_xnc(plan: &ArchivePlan) -> Result<Vec<u8>> {
    let entries = plan
        .packs
        .iter()
        .map(|p| XncEntry {
            version: p.version,
            ptype: p.type_id,
            id: p.id,
            sub_id: p.sub_id,
            name: p.output.clone().or_else(|| xnc_name(p.type_id)),
            data: p.data.clone(),
        })
        .collect::<Vec<_>>();
    let mut out: Vec<u8> = Vec::new();
    XncFile::build(&mut Cursor::new(&mut out), &entries)?;
    Ok(out)
}

/// Default TOC name: `HW_CARS`, `HW_WORLD`, `HW_GEOMETRY`... `None` for
/// anonymous type ids (the field is simply omitted).
fn xnc_name(type_id: u32) -> Option<String> {
    PackType(type_id).name().map(|n| format!("HW_{}", n.to_ascii_uppercase()))
}
