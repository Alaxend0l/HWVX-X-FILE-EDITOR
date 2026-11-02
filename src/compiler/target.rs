//! Build targets: a named (platform, container) pair to build for.

use crate::compiler::project::Project;
use crate::format::Platform;
use serde::{Deserialize, Serialize};

/// Output container format for a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Container {
    /// Classic `.xpc` / `.xgc` / `.xps` archive the game reads directly.
    XArchive,
    /// `XNC` next container (see `docs/compiler/custom-format.md`).
    Xnc,
}

impl Container {
    pub fn extension(self, platform: Platform) -> &'static str {
        match self {
            Container::XArchive => platform.extension(),
            Container::Xnc => "xnc",
        }
    }
}

/// A resolved target: name + platform + container to build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub platform: Platform,
    pub container: Container,
}

/// Names accepted for a platform (kebab-case serde names plus aliases).
pub fn platform_from_name(s: &str) -> Option<Platform> {
    let s = s.trim().to_ascii_lowercase();
    match s.as_str() {
        "pc" | "windows" | "win" | "xpc" => Some(Platform::Pc),
        "gc" | "gamecube" | "gcn" | "ngc" | "xgc" => Some(Platform::Gc),
        "ps2" | "playstation2" | "xps" => Some(Platform::Ps2),
        "ps2-beta" | "ps2beta" | "beta" => Some(Platform::Ps2Beta),
        _ => None,
    }
}

/// Resolve the targets to build.
///
/// `selects` are `--target` values: a target name, optionally suffixed with
/// `:<container>` to build one target into a different container
/// (e.g. `pc:xnc`). With no selection: `default_targets` if non-empty,
/// otherwise every declared target.
///
/// Returns `Err(unknown-name)` so the CLI can exit with code 2.
pub fn resolve_targets(project: &Project, selects: &[String]) -> Result<Vec<Target>, String> {
    let declared = project.declared_targets()?;
    let sel: Vec<Target> = if selects.is_empty() {
        if project.default_targets.is_empty() {
            declared.clone()
        } else {
            let mut out = Vec::new();
            for name in &project.default_targets {
                let t = declared
                    .iter()
                    .find(|t| &t.name == name)
                    .ok_or_else(|| name.clone())?;
                out.push(t.clone());
            }
            out
        }
    } else {
        let mut out = Vec::new();
        for s in selects {
            let (name, container) = match s.split_once(':') {
                Some((n, c)) => {
                    let c = match c.to_ascii_lowercase().as_str() {
                        "xarchive" => Container::XArchive,
                        "xnc" => Container::Xnc,
                        _ => return Err(s.clone()),
                    };
                    (n, Some(c))
                }
                None => (s.as_str(), None),
            };
            let mut t = declared
                .iter()
                .find(|t| t.name == name)
                .ok_or_else(|| s.clone())?
                .clone();
            if let Some(c) = container {
                t.container = c;
            }
            out.push(t);
        }
        out
    };
    // Deduplicate, keep order.
    let mut seen: Vec<(String, Container)> = Vec::new();
    let mut uniq = Vec::new();
    for t in sel {
        let key = (t.name.clone(), t.container);
        if !seen.contains(&key) {
            seen.push(key);
            uniq.push(t);
        }
    }
    Ok(uniq)
}
