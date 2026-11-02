//! Structured compiler diagnostics: stable codes, friendly rendering.
//!
//! Codes are part of the tool's contract (see `docs/compiler/project-format.md`);
//! never renumber them. A build that emits any `Error` produces no output.

use serde::Serialize;
use std::fmt;

/// Diagnostic severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

/// Stable diagnostic codes.
pub mod codes {
    /// The project file could not be parsed.
    pub const PARSE: &str = "E0001";
    /// Unsupported project `schema_version`.
    pub const SCHEMA_VERSION: &str = "E0002";
    /// `--target` or `default_targets` names an unknown target.
    pub const UNKNOWN_TARGET: &str = "E0010";
    /// Two targets share a name.
    pub const DUPLICATE_TARGET: &str = "E0011";
    /// Invalid archive `output` name.
    pub const INVALID_OUTPUT: &str = "E0020";
    /// Two packs in one archive would share a `(type, id, sub_id)` key.
    pub const DUPLICATE_PACK: &str = "E0030";
    /// A referenced file or split-pack folder does not exist.
    pub const MISSING_FILE: &str = "E0031";
    /// A pack source has no way to determine its type or id.
    pub const UNRESOLVABLE_KEY: &str = "E0032";
    /// A source kind reserved for future codegen was used.
    pub const NOT_IMPLEMENTED: &str = "E0040";
    /// A base archive folder is missing, invalid or unreadable.
    pub const INVALID_BASE: &str = "E0041";
    /// The output file exists and `--force` was not given.
    pub const OUTPUT_EXISTS: &str = "E0033";
    /// Reading or writing files failed.
    pub const IO: &str = "E0090";
}

/// One compiler message with a stable code and a source location.
#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: code.to_string(),
            message: message.into(),
            location: None,
            hint: None,
        }
    }
    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.to_string(),
            message: message.into(),
            location: None,
            hint: None,
        }
    }
    pub fn at(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{sev}[{}]: {}", self.code, self.message)?;
        if let Some(l) = &self.location {
            write!(f, "\n  --> {l}")?;
        }
        if let Some(h) = &self.hint {
            write!(f, "\n  = hint: {h}")?;
        }
        Ok(())
    }
}

/// Ordered diagnostic collector.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn add(&mut self, d: Diagnostic) {
        self.items.push(d);
    }
    pub fn error(&mut self, code: &str, message: impl Into<String>) -> Diagnostic {
        let d = Diagnostic::error(code, message);
        self.items.push(d.clone());
        d
    }
    pub fn warning(&mut self, code: &str, message: impl Into<String>) {
        self.items.push(Diagnostic::warning(code, message));
    }
    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }
    pub fn errors(&self) -> usize {
        self.items.iter().filter(|d| d.severity == Severity::Error).count()
    }
    pub fn warnings(&self) -> usize {
        self.items.iter().filter(|d| d.severity == Severity::Warning).count()
    }
}
