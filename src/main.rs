/// `println!` that exits quietly when stdout is closed (e.g. piped into `head`).
macro_rules! out {
    ($($t:tt)*) => {{
        use std::io::Write;
        if writeln!(std::io::stdout(), $($t)*).is_err() {
            std::process::exit(0);
        }
    }};
}

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use hwvx_xfile::archive::Archive;
use hwvx_xfile::format::{is_archive_path, Platform};
use hwvx_xfile::{entry_stem, extract, pack, parse_manifest, verify, Layout, MANIFEST_FILE};

#[derive(Parser)]
#[command(
    name = "hwvx-x",
    version,
    about = "Extract and rebuild Hot Wheels: Velocity X archives (.xpc PC, .xgc GameCube, .xps PS2 / PS2 prototype)",
    after_help = "Typical round trip:\n  \
        hwvx-x extract orig/gc/files work/gc\n  \
        (edit files under work/gc)\n  \
        hwvx-x pack work/gc build/gc/files"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List the packs inside archives (files or folders, searched recursively)
    Info {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Override platform detection
        #[arg(long, value_enum)]
        platform: Option<Platform>,
    },
    /// Extract an archive, or every archive under a folder, to folders with a manifest.json
    Extract {
        /// Archive file or folder of archives
        input: PathBuf,
        /// Output folder (default for a single file: next to it, named after it)
        output: Option<PathBuf>,
        /// Keep each pack as one .bin instead of splitting it into sections
        #[arg(long)]
        raw: bool,
        /// Override platform detection
        #[arg(long, value_enum)]
        platform: Option<Platform>,
        /// Replace existing extractions (folders containing a manifest.json)
        #[arg(short, long)]
        force: bool,
    },
    /// Rebuild an archive from an extracted folder, or every extracted folder under a tree
    Pack {
        /// Extracted folder (or its manifest.json), or a tree of them
        input: PathBuf,
        /// Output archive file (single) or output folder (tree). Default for a
        /// single folder: the manifest's source_file next to the folder
        output: Option<PathBuf>,
        /// Overwrite existing archives
        #[arg(short, long)]
        force: bool,
    },
    /// Check that extract + pack reproduces archives byte for byte (in memory)
    Verify {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Override platform detection
        #[arg(long, value_enum)]
        platform: Option<Platform>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.cmd {
        Cmd::Info { paths, platform } => {
            for file in collect(&paths, is_archive_path)? {
                info(&file, platform)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Extract { input, output, raw, platform, force } => cmd_extract(&input, output, raw, platform, force),
        Cmd::Pack { input, output, force } => cmd_pack(&input, output, force),
        Cmd::Verify { paths, platform } => cmd_verify(&paths, platform),
    }
}

/// Files under `paths` (recursing into folders) accepted by `keep`, sorted.
fn collect(paths: &[PathBuf], keep: fn(&Path) -> bool) -> Result<Vec<PathBuf>> {
    fn walk(dir: &Path, keep: fn(&Path) -> bool, out: &mut Vec<PathBuf>) -> Result<()> {
        let mut items: Vec<_> = fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .collect::<Result<_, _>>()?;
        items.sort_by_key(|e| e.file_name());
        for item in items {
            let path = item.path();
            if item.file_type()?.is_dir() {
                walk(&path, keep, out)?;
            } else if keep(&path) {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            walk(p, keep, &mut out)?;
        } else if p.exists() {
            out.push(p.clone());
        } else {
            bail!("{} does not exist", p.display());
        }
    }
    Ok(out)
}

fn load(file: &Path, platform: Option<Platform>) -> Result<(Vec<u8>, Platform)> {
    let data = fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let platform = match platform {
        Some(p) => p,
        None => Platform::detect(&data, Some(file)).with_context(|| file.display().to_string())?,
    };
    Ok((data, platform))
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn info(file: &Path, platform: Option<Platform>) -> Result<()> {
    let (data, platform) = load(file, platform)?;
    let archive = Archive::parse(&data, platform).with_context(|| file.display().to_string())?;
    out!(
        "{}: {platform} archive, version {}, {} pack(s), {} bytes",
        file.display(),
        archive.version,
        archive.entries.len(),
        data.len()
    );
    for (i, e) in archive.entries.iter().enumerate() {
        let what = hwvx_xfile::pack::split(platform, e.kind, &e.data)
            .map(|sp| hwvx_xfile::pack::summary(&sp))
            .unwrap_or_else(|err| format!("unrecognised layout: {err:#}"));
        out!("  {:<30} {:>9} B  {what}", entry_stem(i, e), e.data.len());
    }
    Ok(())
}

/// Default extraction folder for `file`: its name without the extension.
fn default_dir(file: &Path) -> PathBuf {
    file.with_extension("")
}

fn cmd_extract(input: &Path, output: Option<PathBuf>, raw: bool, platform: Option<Platform>, force: bool) -> Result<ExitCode> {
    let jobs: Vec<(PathBuf, PathBuf)> = if input.is_dir() {
        let Some(out) = output else {
            bail!("extracting a folder needs an output folder");
        };
        collect(&[input.to_path_buf()], is_archive_path)?
            .into_iter()
            .map(|f| {
                let rel = f.strip_prefix(input).unwrap_or(&f).to_path_buf();
                let dest = out.join(default_dir(&rel));
                (f, dest)
            })
            .collect()
    } else {
        let dest = output.unwrap_or_else(|| default_dir(input));
        vec![(input.to_path_buf(), dest)]
    };
    if jobs.is_empty() {
        bail!("no .xpc / .xgc / .xps files found under {}", input.display());
    }

    let mut warnings = 0;
    for (file, dest) in &jobs {
        let (data, platform) = load(file, platform)?;
        let ex = extract(&data, platform, &file_name(file), raw).with_context(|| file.display().to_string())?;
        prepare_dir(dest, force)?;
        for (rel, bytes) in &ex.files {
            let path = dest.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        }
        out!(
            "{} -> {} ({platform}, {} packs)",
            file.display(),
            dest.display(),
            ex.manifest.entries.len()
        );
        for w in &ex.warnings {
            out!("  warning: {w}");
        }
        warnings += ex.warnings.len();
    }
    if jobs.len() > 1 {
        out!("extracted {} archive(s), {warnings} pack(s) kept raw", jobs.len());
    }
    Ok(ExitCode::SUCCESS)
}

/// Make sure `dir` can receive an extraction: it must be new or empty, or
/// (with --force) a previous extraction, which is removed.
fn prepare_dir(dir: &Path, force: bool) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    if !dir.is_dir() {
        bail!("{} exists and is not a folder", dir.display());
    }
    if fs::read_dir(dir)?.next().is_none() {
        return Ok(());
    }
    if !dir.join(MANIFEST_FILE).is_file() {
        bail!("{} is not empty and is not a previous extraction; choose another folder", dir.display());
    }
    if !force {
        bail!("{} already holds an extraction; pass --force to replace it", dir.display());
    }
    fs::remove_dir_all(dir).with_context(|| format!("removing {}", dir.display()))
}

fn cmd_pack(input: &Path, output: Option<PathBuf>, force: bool) -> Result<ExitCode> {
    let single = if input.is_file() {
        Some(input.parent().unwrap_or(Path::new(".")).to_path_buf())
    } else if input.join(MANIFEST_FILE).is_file() {
        Some(input.to_path_buf())
    } else {
        None
    };

    let tree = single.is_none();
    let jobs: Vec<(PathBuf, Option<PathBuf>)> = match single {
        Some(dir) => vec![(dir, output)],
        None => {
            let Some(out) = output else {
                bail!("{} has no {MANIFEST_FILE}; to pack a whole tree, give an output folder", input.display());
            };
            let manifests = collect(&[input.to_path_buf()], |p| p.file_name().is_some_and(|n| n == MANIFEST_FILE))?;
            if manifests.is_empty() {
                bail!("no {MANIFEST_FILE} found under {}", input.display());
            }
            manifests
                .into_iter()
                .map(|m| {
                    let dir = m.parent().unwrap().to_path_buf();
                    let rel_parent = dir.strip_prefix(input).ok().and_then(Path::parent).unwrap_or(Path::new("")).to_path_buf();
                    (dir, Some(out.join(rel_parent)))
                })
                .collect()
        }
    };

    for (dir, out) in &jobs {
        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest = parse_manifest(&fs::read(&manifest_path).with_context(|| format!("reading {}", manifest_path.display()))?)
            .with_context(|| manifest_path.display().to_string())?;
        let dest = match out {
            Some(o) if tree => o.join(&manifest.source_file),
            Some(o) => o.clone(),
            None => dir.parent().unwrap_or(Path::new(".")).join(&manifest.source_file),
        };
        if dest.exists() && !force {
            bail!("{} already exists; pass --force to overwrite it", dest.display());
        }
        let bytes = pack(&manifest, |rel| {
            let path = dir.join(rel);
            match fs::read(&path) {
                Ok(b) => Ok(Some(b)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
            }
        })
        .with_context(|| dir.display().to_string())?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &bytes).with_context(|| format!("writing {}", dest.display()))?;
        let raw = manifest.entries.iter().filter(|e| e.layout == Layout::Raw).count();
        out!(
            "{} -> {} ({}, {} packs{}, {} bytes)",
            dir.display(),
            dest.display(),
            manifest.platform,
            manifest.entries.len(),
            if raw > 0 { format!(", {raw} raw") } else { String::new() },
            bytes.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_verify(paths: &[PathBuf], platform: Option<Platform>) -> Result<ExitCode> {
    let files = collect(paths, is_archive_path)?;
    let (mut ok, mut failed, mut raw) = (0, 0, 0);
    for file in &files {
        let result = load(file, platform).and_then(|(data, p)| verify(&data, p, &file_name(file)).map(|r| (p, r)));
        match result {
            Ok((p, kept_raw)) => {
                ok += 1;
                raw += kept_raw;
                let note = if kept_raw > 0 { format!(" ({kept_raw} pack(s) kept raw)") } else { String::new() };
                out!("ok    {} [{p}]{note}", file.display());
            }
            Err(e) => {
                failed += 1;
                out!("FAIL  {}: {e:#}", file.display());
            }
        }
    }
    out!("{ok} ok, {failed} failed, {raw} pack(s) could not be split");
    Ok(if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
