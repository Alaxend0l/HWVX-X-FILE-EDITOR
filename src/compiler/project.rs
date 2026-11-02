//! The `hwvx.project.json` project file: data model, loading, validation.
//!
//! Schema (draft 1): every archive in `archives` is built once per selected
//! target; the target selects the platform and output container. The JSON
//! shape is mirrored by `schemas/project.schema.json`.

use crate::compiler::diag::{codes, Diagnostics};
use crate::compiler::target::{Container, Target};
use crate::format::{is_archive_path, PackType, Platform};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Default project file name inside a project directory.
pub const PROJECT_FILE_NAME: &str = "hwvx.project.json";
/// The only schema version this tool reads.
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// A whole project.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// JSON-schema pointer for editor tooling; ignored by the compiler.
    #[serde(default, rename = "$schema")]
    pub schema: Option<String>,
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Platform for targets that do not name one.
    #[serde(default)]
    pub default_platform: Option<Platform>,
    /// Container for targets that do not name one (default `xarchive`).
    #[serde(default)]
    pub container: Option<Container>,
    #[serde(default)]
    pub targets: Vec<TargetDecl>,
    /// Target names built when `--target` is not given (default: all).
    #[serde(default)]
    pub default_targets: Vec<String>,
    pub archives: Vec<ArchiveDecl>,
}

/// A target declaration.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetDecl {
    pub name: String,
    #[serde(default)]
    pub platform: Option<Platform>,
    #[serde(default)]
    pub container: Option<Container>,
}

/// An archive to build (without extension; the target appends it).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDecl {
    pub output: String,
    #[serde(default)]
    pub base: Option<BaseRef>,
    #[serde(default)]
    pub packs: Vec<PackSource>,
}

/// Reuse the packs of an extracted archive folder, in manifest order.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseRef {
    /// Folder containing `manifest.json`, relative to the project file.
    pub from: String,
    /// Base pack keys to drop: `"world:0"`, `"cars:3"`, `"geometry:2/q17"`,
    /// or a split folder stem such as `"01_geometry_0_q2_g17"`.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Where one pack in an archive comes from.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackSource {
    /// Vendor-first path: a split-pack folder already in the project.
    /// `type`/`id`/`sub_id` default to what the folder stem encodes
    /// (`03_geometry_0_q2_g17`-style).
    HwvxPack {
        path: String,
        #[serde(default, rename = "type")]
        ptype: Option<PackType>,
        #[serde(default)]
        id: Option<u32>,
        #[serde(default)]
        sub_id: Option<u32>,
        #[serde(default)]
        version: Option<u32>,
        #[serde(default)]
        output: Option<String>,
    },
    /// Pre-built pack bytes inserted verbatim, with an explicit key.
    Raw {
        path: String,
        #[serde(rename = "type")]
        ptype: PackType,
        id: u32,
        #[serde(default)]
        sub_id: u32,
        #[serde(default)]
        version: Option<u32>,
        #[serde(default)]
        output: Option<String>,
    },
    /// Codegen kinds, not implemented yet (roadmap milestones M2-M4).
    #[serde(rename = "interface")]
    Interface { items: serde_json::Value },
    /// Car description source (roadmap M2).
    #[serde(rename = "car")]
    Car { file: String },
    /// Items/cheats table source (roadmap M2).
    #[serde(rename = "items")]
    Items { file: String },
    /// World description source (roadmap M4).
    #[serde(rename = "world")]
    World { file: String },
    /// Geometry/track source (roadmap M4).
    #[serde(rename = "geometry")]
    Geometry { file: String },
    /// Audio source (roadmap M4).
    #[serde(rename = "audio")]
    Audio { file: String },
}

impl Project {
    /// Read and parse `<dir>/hwvx.project.json` (or `dir` itself when it is
    /// a file). `Err(Diagnostic)` for parse/version problems (exit code 2).
    pub fn load(project_dir: &Path) -> std::result::Result<Project, crate::compiler::diag::Diagnostic> {
        let path = if project_dir.is_dir() {
            project_dir.join(PROJECT_FILE_NAME)
        } else {
            project_dir.to_path_buf()
        };
        let text = std::fs::read_to_string(&path).map_err(|e| {
            let d = crate::compiler::diag::Diagnostic::error(
                codes::PARSE,
                format!("cannot read project file: {e}"),
            );
            d.at(path.display().to_string())
        })?;
        let project: Project = serde_json::from_str(&text).map_err(|e| {
            let d = crate::compiler::diag::Diagnostic::error(
                codes::PARSE,
                format!("invalid project JSON: {e}"),
            );
            d.at(path.display().to_string())
                .hint("check field names (unknown fields are rejected) and `schema_version`")
        })?;
        if project.schema_version != SUPPORTED_SCHEMA_VERSION {
            let d = crate::compiler::diag::Diagnostic::error(
                codes::SCHEMA_VERSION,
                format!(
                    "unsupported schema_version {} (this tool implements v{SUPPORTED_SCHEMA_VERSION})",
                    project.schema_version
                ),
            );
            return Err(d.at(path.display().to_string()));
        }
        Ok(project)
    }

    /// The project directory (folder containing the project file).
    pub fn dir_of(project_file: &Path) -> PathBuf {
        project_file.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
    }

    /// Resolve target declarations; `Err(message)` for user-level problems.
    pub fn declared_targets(&self) -> std::result::Result<Vec<Target>, String> {
        let mut out: Vec<Target> = Vec::new();
        for t in &self.targets {
            let platform = t
                .platform
                .or(self.default_platform)
                .ok_or_else(|| format!("target {:?}: no platform and no default_platform", t.name))?;
            if out.iter().any(|d: &Target| d.name == t.name) {
                return Err(format!("duplicate target name {:?}", t.name));
            }
            out.push(Target {
                name: t.name.clone(),
                platform,
                container: t.container.or(self.container).unwrap_or(Container::XArchive),
            });
        }
        Ok(out)
    }

    /// Semantic validation beyond serde parsing. Returns success; errors are
    /// reported through `diags`.
    pub fn validate(&self, project_dir: &Path, diags: &mut Diagnostics) -> bool {
        if self.name.trim().is_empty() {
            diags.warning("W0001", "project name is empty");
        }
        if let Err(msg) = self.declared_targets() {
            diags.error(codes::DUPLICATE_TARGET, msg);
        }
        for name in &self.default_targets {
            if !self.targets.iter().any(|t| &t.name == name) {
                diags.error(codes::UNKNOWN_TARGET, format!("default_targets names unknown target {name:?}"));
            }
        }
        if self.targets.is_empty() {
            diags.error(codes::UNKNOWN_TARGET, "project declares no targets");
        }
        for a in &self.archives {
            self.validate_output(&a.output, diags);
            if let Some(base) = &a.base {
                let dir = project_dir.join(&base.from);
                if !dir.is_dir() {
                    diags.error(
                        codes::INVALID_BASE,
                        format!("base folder {:?} does not exist", base.from),
                    ).at(format!("archives.{}.base.from", a.output));
                } else if !dir.join(crate::MANIFEST_FILE).is_file() {
                    diags.error(
                        codes::INVALID_BASE,
                        format!("base folder {:?} has no {}", base.from, crate::MANIFEST_FILE),
                    );
                }
            }
            for src in &a.packs {
                if let Some(out) = src.output() {
                    self.validate_output(out, diags);
                }
                self.validate_source(project_dir, src, diags);
            }
        }
        !diags.has_errors()
    }

    fn validate_output(&self, output: &str, diags: &mut Diagnostics) {
        let bad = output.is_empty()
            || output.contains('/')
            || output.contains('\\')
            || output == "."
            || output == ".."
            || Path::new(output).is_absolute();
        if bad {
            diags.error(codes::INVALID_OUTPUT, format!("invalid archive output name {output:?}"));
            return;
        }
        let p = Path::new(output);
        if p.extension().is_some_and(|e| {
            let e = e.to_string_lossy().to_ascii_lowercase();
            matches!(e.as_str(), "xpc" | "xgc" | "xps" | "xnc")
        }) || is_archive_path(p)
        {
            diags.error(
                codes::INVALID_OUTPUT,
                format!("archive output {output:?} must not carry an extension; the target appends it"),
            );
        }
    }

    fn validate_source(&self, project_dir: &Path, src: &PackSource, diags: &mut Diagnostics) {
        let missing = |diags: &mut Diagnostics, kind: &str, path: &str| {
            diags.error(codes::MISSING_FILE, format!("pack source {kind:?} not found: {path}"));
        };
        match src {
            PackSource::HwvxPack { path, .. } => {
                let d = project_dir.join(path);
                if !d.join(crate::PACK_FILE).is_file() {
                    missing(diags, "hwvx_pack", path);
                }
            }
            PackSource::Raw { path, .. } => {
                let d = project_dir.join(path);
                if !d.is_file() {
                    missing(diags, "raw", path);
                }
            }
            _ => {} // not-implemented kinds are diagnosed during lowering
        }
    }
}

impl PackSource {
    fn output(&self) -> Option<&str> {
        match self {
            PackSource::HwvxPack { output, .. } | PackSource::Raw { output, .. } => {
                output.as_deref()
            }
            _ => None,
        }
    }
}
