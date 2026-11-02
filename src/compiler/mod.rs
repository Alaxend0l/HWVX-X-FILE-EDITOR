//! The `hwvx` compiler: project JSON -> IR -> linker -> backends.
//!
//! ```text
//! hwvx.project.json ─┐
//! extracted packs  ──┼─ frontend (project.rs) ─ lowering (ir.rs)
//! raw blobs        ──┘                              │
//!                                        linker (link.rs): key checks
//!                                                   │
//!                              backends (backend.rs): xarchive | xnc
//! ```
//!
//! Vendor-first scope (roadmap M1): archives are assembled from extracted
//! vendor packs, raw blobs, and project-internal pack folders. Codegen
//! source kinds parse, validate, and fail with `E0040` until their
//! milestones land. See `docs/compiler/` for the full documentation.

pub mod backend;
pub mod diag;
pub mod ir;
pub mod link;
pub mod project;
pub mod target;

pub use diag::{Diagnostic, Diagnostics, Severity};
pub use project::{Project, PROJECT_FILE_NAME};
pub use target::{Container, Target};

use serde::Serialize;
use std::path::Path;

/// One line of the build report: a pack that went into an archive.
#[derive(Clone, Debug, Serialize)]
pub struct PackReport {
    pub r#type: String,
    pub id: u32,
    pub sub_id: u32,
    pub kind: String,
    pub bytes: usize,
    pub source: ir::SourceId,
}

/// One archive built (one target x one archive declaration).
#[derive(Clone, Debug, Serialize)]
pub struct ArchiveReport {
    pub target: String,
    pub platform: String,
    pub container: String,
    /// File name written, extension included.
    pub output: String,
    pub bytes: usize,
    pub packs: Vec<PackReport>,
}

/// Machine-readable result of a build (`hwvx-x build --json`).
#[derive(Clone, Debug, Serialize)]
pub struct BuildReport {
    pub tool: String,
    pub project: String,
    pub targets: Vec<String>,
    pub archives: Vec<ArchiveReport>,
    pub errors: usize,
    pub warnings: usize,
    /// All messages (errors and warnings), also in the `--json` output.
    pub diagnostics: Vec<Diagnostic>,
}

impl BuildReport {
    pub fn new(project_name: &str) -> Self {
        Self {
            tool: format!("hwvx-xfile {}", env!("CARGO_PKG_VERSION")),
            project: project_name.to_string(),
            targets: Vec::new(),
            archives: Vec::new(),
            errors: 0,
            warnings: 0,
            diagnostics: Vec::new(),
        }
    }
}

/// Outcome of `build_project`: report + diagnostics + process exit code.
pub struct BuildOutcome {
    pub report: BuildReport,
    pub diags: Diagnostics,
    /// 0 = success, 1 = build failed, 2 = invalid project/usage.
    pub exit: u8,
}

/// Run a whole build. Never panics on bad input; every user-level problem
/// becomes a diagnostic. IO failures are also reported as diagnostics.
/// Early exit with the diagnostics recorded so far in the report.
fn fail(diags: Diagnostics, exit: u8, project_name: &str) -> BuildOutcome {
    let mut report = BuildReport::new(project_name);
    report.errors = diags.errors();
    report.warnings = diags.warnings();
    report.diagnostics = diags.items.clone();
    BuildOutcome { report, diags, exit }
}

pub fn build_project(
    project_path: &Path,
    selects: &[String],
    out_dir: &Path,
    force: bool,
) -> BuildOutcome {
    let mut diags = Diagnostics::default();
    let project = match Project::load(project_path) {
        Ok(p) => p,
        Err(d) => {
            diags.add(d);
            return fail(diags, 2, "(invalid)");
        }
    };
    let dir = project_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let dir = if project_path.is_dir() {
        project_path.to_path_buf()
    } else {
        dir
    };
    if !project.validate(&dir, &mut diags) {
        return fail(diags, 2, &project.name);
    }
    let targets = match target::resolve_targets(&project, selects) {
        Ok(t) => t,
        Err(name) => {
            diags.error(
                diag::codes::UNKNOWN_TARGET,
                format!("unknown target {name:?} (see the `targets` list)"),
            );
            return fail(diags, 2, &project.name);
        }
    };

    let mut report = BuildReport::new(&project.name);
    report.targets = targets.iter().map(|t| t.name.clone()).collect();

    for t in &targets {
        for decl in &project.archives {
            let Some(plan) = ir::lower_archive(&dir, decl, t, &mut diags) else {
                continue;
            };
            link::check_plan(&plan, &mut diags);
            let emitted = match backend::emit(&plan, out_dir, force, &mut diags) {
                Ok(p) => p,
                Err(e) => {
                    diags.error(
                        diag::codes::IO,
                        format!("failed to write {:?}: {e:#}", plan.output),
                    );
                    None
                }
            };
            if let Some(path) = emitted {
                let bytes = std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
                report.archives.push(ArchiveReport {
                    target: t.name.clone(),
                    platform: platform_name(t.platform),
                    container: container_name(t.container),
                    output: path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                    bytes,
                    packs: plan
                        .packs
                        .iter()
                        .map(|p| PackReport {
                            r#type: ir::key_type_name(p.type_id),
                            id: p.id,
                            sub_id: p.sub_id,
                            kind: p.source.kind.clone(),
                            bytes: p.data.len(),
                            source: p.source.clone(),
                        })
                        .collect(),
                });
            }
        }
    }

    report.errors = diags.errors();
    report.warnings = diags.warnings();
    report.diagnostics = diags.items.clone();
    let exit = if diags.has_errors() { 1 } else { 0 };
    BuildOutcome { report, diags, exit }
}

/// `"pc"`, `"gc"`, `"ps2"`, `"ps2-beta"` (serde names, stable in reports).
pub fn platform_name(p: crate::format::Platform) -> String {
    use crate::format::Platform::*;
    match p {
        Pc => "pc",
        Gc => "gc",
        Ps2 => "ps2",
        Ps2Beta => "ps2-beta",
    }
    .to_string()
}

pub fn container_name(c: Container) -> String {
    match c {
        Container::XArchive => "xarchive",
        Container::Xnc => "xnc",
    }
    .to_string()
}
