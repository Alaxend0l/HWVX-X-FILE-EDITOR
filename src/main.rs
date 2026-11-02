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
use hwvx_xfile::format::{is_archive_path, Endian, Platform};
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
    /// Export the textures of an extracted pack folder as .tga images
    Textures {
        /// Extracted pack folder (contains pack.json and data.bin)
        pack: PathBuf,
        /// Output folder for the images
        output: PathBuf,
    },
    /// Convert a PS2-prototype car pack and add it to an extracted GameCube
    /// HW_Cars folder as a new car
    PortCar {
        /// Extracted prototype car pack folder (e.g. work/beta/HW_CARS/02_car_1)
        beta: PathBuf,
        /// Extracted GameCube HW_Cars folder to add the car to
        #[arg(long)]
        into: PathBuf,
        /// Pack id for the new car (33, 34, 35 ... must be unused)
        #[arg(long)]
        id: u32,
        /// GameCube car pack to take the HUD and cameras from (default: car 0
        /// in the --into folder)
        #[arg(long)]
        donor: Option<PathBuf>,
        /// Shared-pack reference map
        #[arg(long, default_value = "data/beta-imports.json")]
        imports: PathBuf,
        /// Height the car is lifted by on interface screens (car select);
        /// GameCube cars use 0.5 - 3.5
        #[arg(long, default_value_t = 2.0)]
        view_offset: f32,
        /// Replace an existing car with the same id
        #[arg(short, long)]
        force: bool,
    },
    /// Add cars to the car lists of extracted GameCube archives (HW_IPack car
    /// select: range, names, thumbnails; worlds: results-screen names)
    Roster {
        /// Extracted HW_IPack folder, world folder, or a tree of them
        ipack: PathBuf,
        /// Car id and name, e.g. --name '33=Fandango*' (a trailing * shows ™)
        #[arg(long = "name", required = true)]
        names: Vec<String>,
        /// Text shown under a locked car
        #[arg(long, default_value = " ")]
        hint: String,
        /// Extracted GameCube HW_Cars folder: render a thumbnail of each
        /// named car from its pack
        #[arg(long)]
        cars: Option<PathBuf>,
        /// Thumbnail image for a car, e.g. --thumb 33=fandango.tga (TGA,
        /// any size; drawn upright, transparent background)
        #[arg(long = "thumb")]
        thumbs: Vec<String>,
        /// Squash rendered thumbnails towards square the way the game's own
        /// are (default: keep the car's proportions)
        #[arg(long)]
        stretch: bool,
        /// Menu pack (mcp.xgc / mcp.xpc, or its extracted folder) to take
        /// the chrome sphere maps from; default: a procedural stand-in
        #[arg(long)]
        env: Option<PathBuf>,
    },
    /// Render a preview of a GameCube or PC car pack (3/4 view, transparent background)
    RenderCar {
        /// Extracted GameCube or PC car pack folder
        pack: PathBuf,
        /// Output image (.tga)
        output: PathBuf,
        #[arg(long, default_value_t = 256)]
        size: usize,
        /// Viewing angle around the car in degrees (0 = from the front)
        #[arg(long, default_value_t = hwvx_xfile::render::THUMB_YAW, allow_hyphen_values = true)]
        yaw: f32,
        /// Viewing angle above the car in degrees
        #[arg(long, default_value_t = hwvx_xfile::render::THUMB_PITCH, allow_hyphen_values = true)]
        pitch: f32,
        /// Squash the car towards square the way the game's car-select
        /// thumbnails are (default: keep the car's proportions)
        #[arg(long)]
        stretch: bool,
        /// Menu pack (mcp.xgc / mcp.xpc, or its extracted folder) to take
        /// the chrome sphere maps from; default: a procedural stand-in
        #[arg(long)]
        env: Option<PathBuf>,
    },
    /// Export the models of an extracted GameCube pack (or of every pack in an
    /// extracted archive) to glTF binary (.glb), with embedded textures.
    /// Cars are posed on their skeleton; other packs export every geometry.
    ExportGltf {
        /// Extracted GameCube pack folder, or an extracted archive folder
        /// (one .glb per pack with models)
        input: PathBuf,
        /// Output .glb (pack) or folder (archive). Default: next to the
        /// input, named after it
        output: Option<PathBuf>,
        /// Extracted archive (or a tree of them) to find shared packs in, e.g.
        /// an extracted HW_share.xgc. The input's own archive is always searched
        #[arg(long = "share", value_name = "FOLDER")]
        share: Vec<PathBuf>,
        /// Texture pack for geometry (quad) packs: the extracted world texture
        /// pack (wN_tex), or its archive folder
        #[arg(long, value_name = "FOLDER")]
        textures: Option<PathBuf>,
        /// Export every LOD, not just the most detailed one
        #[arg(long)]
        all_lods: bool,
        /// Cars: also export the damaged versions of breakable body parts
        #[arg(long)]
        damaged: bool,
        /// Cars: export every geometry in the pack at the origin instead of
        /// the posed car
        #[arg(long)]
        scan: bool,
        /// Overwrite existing output files
        #[arg(long)]
        force: bool,
    },
    /// Patch the PC Hot Wheels.exe so added cars (ids 33-35) are unlocked
    PatchExe {
        /// Hot Wheels.exe to patch in place (a backup is written next to it
        /// as "Hot Wheels.exe.orig" unless one exists)
        exe: PathBuf,
        /// Undo the patch instead
        #[arg(long)]
        revert: bool,
    },
    /// Check that extract + pack reproduces archives byte for byte (in memory)
    Verify {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Override platform detection
        #[arg(long, value_enum)]
        platform: Option<Platform>,
    },
    /// Build archives from a project (hwvx.project.json): vendor packs,
    /// split-pack folders and raw blobs into .xpc/.xgc/.xps or .xnc files
    Build {
        /// Project folder, or the hwvx.project.json file itself
        project: PathBuf,
        /// Target to build: a name from the project, or name:xarchive|xnc.
        /// Repeatable. Default: default_targets, else all declared targets
        #[arg(long = "target")]
        targets: Vec<String>,
        /// Output folder (default: <project>/build)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Overwrite existing archives
        #[arg(short, long)]
        force: bool,
        /// Print the build report as JSON (diagnostics included)
        #[arg(long)]
        json: bool,
    },
    /// Print a built-in JSON schema (project)
    Schema {
        /// Schema name: project
        name: String,
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
        Cmd::Textures { pack, output } => cmd_textures(&pack, &output),
        Cmd::ExportGltf { input, output, share, textures, all_lods, damaged, scan, force } => {
            let opts = hwvx_xfile::gltf::Options { all_lods, damaged, scan };
            cmd_export_gltf(&input, output, &share, textures.as_deref(), &opts, force)
        }
        Cmd::PatchExe { exe, revert } => cmd_patch_exe(&exe, revert),
        Cmd::RenderCar { pack, output, size, yaw, pitch, stretch, env } => cmd_render_car(&pack, &output, size, yaw, pitch, stretch, env.as_deref()),
        Cmd::Roster { ipack, names, hint, cars, thumbs, stretch, env } => cmd_roster(&ipack, &names, &hint, cars.as_deref(), &thumbs, stretch, env.as_deref()),
        Cmd::PortCar { beta, into, id, donor, imports, view_offset, force } => cmd_port_car(&beta, &into, id, donor, &imports, view_offset, force),
        Cmd::Build { project, targets, out, force, json } => cmd_build(&project, &targets, out, force, json),
        Cmd::Schema { name } => cmd_schema(&name),
    }
}

/// Build every archive of a project for the selected targets.
fn cmd_build(project: &Path, targets: &[String], out: Option<PathBuf>, force: bool, json: bool) -> Result<ExitCode> {
    let dir = out.unwrap_or_else(|| {
        let root = if project.is_file() { project.parent().unwrap_or(Path::new(".")).to_path_buf() } else { project.to_path_buf() };
        root.join("build")
    });
    let outcome = hwvx_xfile::compiler::build_project(project, targets, &dir, force);
    if json {
        println!("{}", serde_json::to_string_pretty(&outcome.report)?);
    } else {
        for d in &outcome.diags.items {
            eprintln!("{d}");
        }
        for a in &outcome.report.archives {
            println!("wrote {} ({} packs, {} bytes)", a.output, a.packs.len(), a.bytes);
        }
        let (e, w) = (outcome.report.errors, outcome.report.warnings);
        if outcome.exit == 0 {
            println!(
                "build ok: {} archive(s) for {} target(s) of project {:?} into {}",
                outcome.report.archives.len(),
                outcome.report.targets.len(),
                outcome.report.project,
                dir.display()
            );
        } else {
            eprintln!("build failed: {e} error(s), {w} warning(s)");
        }
    }
    Ok(ExitCode::from(outcome.exit))
}

/// Print one of the embedded JSON schemas.
fn cmd_schema(name: &str) -> Result<ExitCode> {
    let text = match name {
        "project" | "project.schema.json" => include_str!("../schemas/project.schema.json"),
        other => {
            let known = ["project"];
            anyhow::bail!("unknown schema {other:?} (known: {})", known.join(", "));
        }
    };
    print!("{text}");
    Ok(ExitCode::SUCCESS)
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

/// Platform of an extracted pack folder, from its archive's manifest.json.
fn pack_platform(pack: &Path) -> Result<Platform> {
    let manifest = pack.parent().unwrap_or(Path::new(".")).join(MANIFEST_FILE);
    let m = parse_manifest(&fs::read(&manifest).with_context(|| format!("reading {}", manifest.display()))?)?;
    Ok(m.platform)
}

fn cmd_textures(pack: &Path, output: &Path) -> Result<ExitCode> {
    use hwvx_xfile::convert::texture;
    use hwvx_xfile::pack::{PackHeader, PackJson};
    let platform = pack_platform(pack)?;
    let json: PackJson = serde_json::from_slice(&fs::read(pack.join(hwvx_xfile::PACK_FILE))?)?;
    let PackHeader::Standard(h) = json.header else {
        bail!("geometry packs have no textures of their own");
    };
    let blob = fs::read(pack.join("data.bin")).unwrap_or_default();
    let textures = texture::decode_pack_textures(platform, &blob, h.num_textures, h.texture_data_offset, h.palette_offset)?;
    fs::create_dir_all(output)?;
    for (i, (info, img)) in textures.iter().enumerate() {
        let path = output.join(format!("tex{i:03}.tga"));
        fs::write(&path, texture::tga(img))?;
        out!("{}  {info}", path.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn read_split(dir: &Path) -> Result<hwvx_xfile::pack::SplitPack> {
    use hwvx_xfile::pack::{PackJson, SplitPack};
    let json_path = dir.join(hwvx_xfile::PACK_FILE);
    let json: PackJson = serde_json::from_slice(&fs::read(&json_path).with_context(|| format!("reading {}", json_path.display()))?)
        .with_context(|| json_path.display().to_string())?;
    let mut sections = std::collections::BTreeMap::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "bin") {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            sections.insert(name, fs::read(&path)?);
        }
    }
    Ok(SplitPack { json, sections })
}

fn write_split(dir: &Path, sp: &hwvx_xfile::pack::SplitPack) -> Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)?;
    fs::write(dir.join(hwvx_xfile::PACK_FILE), hwvx_xfile::json_bytes(&sp.json)?)?;
    for (name, bytes) in &sp.sections {
        fs::write(dir.join(format!("{name}.bin")), bytes)?;
    }
    Ok(())
}

fn cmd_port_car(beta: &Path, into: &Path, id: u32, donor: Option<PathBuf>, imports: &Path, view_offset: f32, force: bool) -> Result<ExitCode> {
    use hwvx_xfile::convert::car;
    use hwvx_xfile::format::PackType;
    use hwvx_xfile::{Layout, ManifestEntry};

    let beta_platform = pack_platform(beta)?;
    if !beta_platform.is_beta() {
        bail!("{} is a {beta_platform} pack; port-car converts PS2-prototype car packs", beta.display());
    }
    let manifest_path = into.join(MANIFEST_FILE);
    let mut manifest = parse_manifest(&fs::read(&manifest_path).with_context(|| format!("reading {}", manifest_path.display()))?)?;
    if !matches!(manifest.platform, Platform::Gc | Platform::Pc) {
        bail!("{} is not an extracted GameCube or PC archive", into.display());
    }
    let existing = manifest.entries.iter().position(|e| e.kind == PackType(0) && e.id == id);
    if existing.is_some() && !force {
        bail!("{} already has a car with id {id}; pass --force to replace it", into.display());
    }
    let donor_dir = match donor {
        Some(d) => d,
        None => into.join(
            &manifest.entries.iter().find(|e| e.kind == PackType(0)).context("no GameCube car in the --into folder to use as donor")?.path,
        ),
    };
    let donor_pack = read_split(&donor_dir)?;
    let map = car::ImportMap::parse(&fs::read(imports).with_context(|| format!("reading {}", imports.display()))?)?;
    let beta_pack = read_split(beta)?;
    let (out, report) = car::convert(&beta_pack, &car::Options { new_id: id, donor: &donor_pack, imports: &map, view_offset: [0.0, view_offset, 0.0], platform: manifest.platform })
        .with_context(|| format!("converting {}", beta.display()))?;

    let index = existing.unwrap_or(manifest.entries.len());
    let folder = format!("{index:02}_car_{id}");
    let old_path = existing.map(|i| manifest.entries[i].path.clone());
    if let Some(p) = &old_path {
        let old = into.join(p);
        if old.is_dir() {
            fs::remove_dir_all(&old)?;
        } else if old.is_file() {
            fs::remove_file(&old)?;
        }
    }
    write_split(&into.join(&folder), &out)?;
    let entry = ManifestEntry {
        kind: PackType(0),
        id,
        sub_id: 0,
        version: manifest.archive_version,
        path: folder.clone(),
        layout: Layout::Split,
        summary: hwvx_xfile::pack::summary(&out),
        gap_fill: Default::default(),
    };
    match existing {
        Some(i) => manifest.entries[i] = entry,
        None => manifest.entries.push(entry),
    }
    fs::write(&manifest_path, hwvx_xfile::json_bytes(&manifest)?)?;

    out!("{} -> {} (car id {id}, donor {})", beta.display(), into.join(&folder).display(), donor_dir.display());
    out!("  models: {}{}", report.models, if report.shadow { " (incl. shadow volume)" } else { "" });
    out!("  textures: {}", report.textures.join(", "));
    out!("  sounds: {}", report.sounds);
    out!("  shared references remapped: {}", report.remapped);
    for d in &report.dropped {
        out!("  dropped (no GameCube counterpart): {d}");
    }
    for n in &report.notes {
        out!("  note: {n}");
    }
    out!("now rebuild the archive: hwvx-x pack {} <output .xgc>", into.display());
    Ok(ExitCode::SUCCESS)
}

fn cmd_roster(ipack: &Path, names: &[String], hint: &str, cars: Option<&Path>, thumbs: &[String], stretch: bool, env: Option<&Path>) -> Result<ExitCode> {
    use hwvx_xfile::blob::Blob;

    let mut list = vec![];
    for n in names {
        let (id, name) = n.split_once('=').with_context(|| format!("--name {n:?}: expected ID=NAME"))?;
        list.push((id.trim().parse::<u32>().with_context(|| format!("--name {n:?}: bad id"))?, name.to_string()));
    }
    // thumbnails: explicit images first, else rendered from the car packs
    let mut images = std::collections::BTreeMap::new();
    for t in thumbs {
        let (id, file) = t.split_once('=').with_context(|| format!("--thumb {t:?}: expected ID=FILE"))?;
        let img = hwvx_xfile::convert::texture::read_tga(&fs::read(file).with_context(|| format!("reading {file}"))?)
            .with_context(|| file.to_string())?;
        images.insert(id.trim().parse::<u32>()?, img);
    }
    if let Some(cars) = cars {
        let env = load_env(env)?;
        let m = parse_manifest(&fs::read(cars.join(MANIFEST_FILE)).with_context(|| format!("reading {}", cars.join(MANIFEST_FILE).display()))?)?;
        for (id, _) in &list {
            if images.contains_key(id) {
                continue;
            }
            let e = m.entries.iter().find(|e| e.kind == hwvx_xfile::format::PackType(0) && e.id == *id)
                .with_context(|| format!("{} has no car with id {id}", cars.display()))?;
            let (blob, en, tex) = load_car(&cars.join(&e.path))?;
            images.insert(*id, hwvx_xfile::render::render_car(&blob, en, &tex, &env, 128, hwvx_xfile::render::THUMB_YAW, hwvx_xfile::render::THUMB_PITCH, stretch)?);
        }
    }
    // every extracted GameCube archive at or under `ipack` (HW_IPack, worlds)
    let manifests = if ipack.join(MANIFEST_FILE).is_file() {
        vec![ipack.join(MANIFEST_FILE)]
    } else {
        collect(&[ipack.to_path_buf()], |p| p.file_name().is_some_and(|n| n == MANIFEST_FILE))?
    };
    let mut changed = 0;
    for mpath in manifests {
        let root = mpath.parent().unwrap().to_path_buf();
        let manifest = parse_manifest(&fs::read(&mpath)?)?;
        let endian = match manifest.platform {
            Platform::Gc => Endian::Big,
            Platform::Pc => Endian::Little,
            _ => continue, // PS2 builds are not targets
        };
        for e in &manifest.entries {
            // interface packs, and worlds (their HUD / results screens list car names)
            if e.layout != hwvx_xfile::Layout::Split || !matches!(e.kind.0, 1 | 4) {
                continue;
            }
            let dir = root.join(&e.path);
            let mut sp = read_split(&dir)?;
            let mut blob = Blob::from_split(endian, &sp);
            let rep = hwvx_xfile::roster::patch(&mut blob, endian, &list, hint).with_context(|| dir.display().to_string())?;
            let mut thumbs_set = 0;
            if rep.screens > 0 {
                if let hwvx_xfile::pack::PackHeader::Standard(h) = &sp.json.header {
                    let tdo = h.texture_data_offset;
                    for (id, img) in &images {
                        // stored upside down, 128x128 like the game's own thumbnails
                        let img = img.resized(128, 128).flipped();
                        for idx in hwvx_xfile::roster::thumbnail_textures(&blob, endian, *id) {
                            if manifest.platform == Platform::Pc {
                                hwvx_xfile::roster::replace_texture_pc(&mut blob, tdo, idx, &img)?;
                            } else {
                                hwvx_xfile::roster::replace_texture_gc(&mut blob, tdo, idx, &img)?;
                            }
                            thumbs_set += 1;
                        }
                    }
                }
            }
            if rep.ranges + rep.names + rep.hints + thumbs_set == 0 {
                continue;
            }
            blob.validate(endian)?;
            blob.store(endian, &mut sp);
            sp.describe(manifest.platform);
            write_split(&dir, &sp)?;
            changed += 1;
            out!(
                "{}: {} car-select screen(s), {} range(s), {} name(s), {} hint(s), {} thumbnail(s)",
                dir.strip_prefix(ipack).unwrap_or(&dir).display(),
                rep.screens,
                rep.ranges,
                rep.names,
                rep.hints,
                thumbs_set
            );
        }
    }
    if changed == 0 {
        bail!("no car lists found under {}", ipack.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// A GameCube or PC car pack folder: its object data, byte order and
/// decoded textures (for rendering).
fn load_car(pack: &Path) -> Result<(hwvx_xfile::blob::Blob, Endian, Vec<hwvx_xfile::convert::texture::Image>)> {
    use hwvx_xfile::pack::PackHeader;
    let platform = pack_platform(pack)?;
    if !matches!(platform, Platform::Gc | Platform::Pc) {
        bail!("{} is a {platform} pack; render GameCube or PC car packs (convert prototype cars with port-car first)", pack.display());
    }
    let sp = read_split(pack)?;
    let PackHeader::Standard(h) = &sp.json.header else { bail!("{} is not a car pack", pack.display()) };
    let e = platform.endian();
    let blob = hwvx_xfile::blob::Blob::from_split(e, &sp);
    let tex = hwvx_xfile::render::pack_textures(&blob, platform, h.num_textures, h.texture_data_offset)?;
    Ok((blob, e, tex))
}

/// Chrome sphere maps from the menu pack at `path` (archive file, extracted
/// archive folder or extracted pack folder), else the procedural stand-ins.
fn load_env(path: Option<&Path>) -> Result<hwvx_xfile::render::EnvMaps> {
    use hwvx_xfile::pack::PackHeader;
    use hwvx_xfile::render::EnvMaps;
    let Some(path) = path else { return Ok(EnvMaps::procedural()) };
    let ctx = || format!("reading sphere maps from {}", path.display());
    let (sp, platform) = if path.is_file() {
        let platform = match path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).as_deref() {
            Some("xgc") => Platform::Gc,
            Some("xpc") => Platform::Pc,
            _ => bail!("{}: expected a .xgc or .xpc menu pack", path.display()),
        };
        let data = fs::read(path).with_context(ctx)?;
        let a = Archive::parse(&data, platform).with_context(ctx)?;
        let e = a.entries.first().with_context(|| format!("{} is empty", path.display()))?;
        (hwvx_xfile::split_checked(platform, e).with_context(ctx)?, platform)
    } else if path.join(MANIFEST_FILE).is_file() {
        let m = parse_manifest(&fs::read(path.join(MANIFEST_FILE)).with_context(ctx)?)?;
        let e = m.entries.first().with_context(|| format!("{} is empty", path.display()))?;
        (read_split(&path.join(&e.path)).with_context(ctx)?, m.platform)
    } else {
        (read_split(path).with_context(ctx)?, pack_platform(path).with_context(ctx)?)
    };
    let PackHeader::Standard(h) = &sp.json.header else { bail!("{}: not a menu pack", path.display()) };
    let blob = hwvx_xfile::blob::Blob::from_split(platform.endian(), &sp);
    let tex = hwvx_xfile::render::pack_textures(&blob, platform, h.num_textures, h.texture_data_offset).with_context(ctx)?;
    EnvMaps::from_mcp(&blob, &tex).with_context(ctx)
}

fn cmd_render_car(pack: &Path, output: &Path, size: usize, yaw: f32, pitch: f32, stretch: bool, env: Option<&Path>) -> Result<ExitCode> {
    let (blob, e, tex) = load_car(pack)?;
    let env = load_env(env)?;
    let img = hwvx_xfile::render::render_car(&blob, e, &tex, &env, size, yaw, pitch, stretch)?;
    fs::write(output, hwvx_xfile::convert::texture::tga(&img))?;
    out!("{}", output.display());
    Ok(ExitCode::SUCCESS)
}

/// Decoded textures of the extracted pack folder `dir` (none for quad packs).
fn pack_texture_images(dir: &Path, platform: Platform) -> Result<Vec<hwvx_xfile::convert::texture::Image>> {
    let sp = read_split(dir)?;
    let hwvx_xfile::pack::PackHeader::Standard(h) = &sp.json.header else { return Ok(vec![]) };
    let blob = hwvx_xfile::blob::Blob::from_split(platform.endian(), &sp);
    hwvx_xfile::render::pack_textures(&blob, platform, h.num_textures, h.texture_data_offset)
}

/// Extracted GameCube archives (manifest.json files) at or under `path`.
fn manifests_under(path: &Path) -> Result<Vec<PathBuf>> {
    if path.join(MANIFEST_FILE).is_file() {
        return Ok(vec![path.join(MANIFEST_FILE)]);
    }
    if path.file_name().is_some_and(|n| n == MANIFEST_FILE) {
        return Ok(vec![path.to_path_buf()]);
    }
    collect(&[path.to_path_buf()], |p| p.file_name().is_some_and(|n| n == MANIFEST_FILE))
}

/// Shared-pack textures needed by `blob`, looked up by pack id among the share
/// and world texture packs of the archives in `roots`.
fn shared_textures(
    blob: &hwvx_xfile::blob::Blob,
    roots: &[PathBuf],
    cache: &mut std::collections::HashMap<u32, Option<Vec<hwvx_xfile::convert::texture::Image>>>,
) -> Result<std::collections::HashMap<u32, Vec<hwvx_xfile::convert::texture::Image>>> {
    let mut out = std::collections::HashMap::new();
    let ids: std::collections::BTreeSet<u32> = blob.texture_relocs.iter().map(|r| r.pack()).filter(|&p| p != 0).collect();
    for id in ids {
        if !cache.contains_key(&id) {
            let mut found = None;
            'search: for root in roots {
                for mpath in manifests_under(root)? {
                    let m = parse_manifest(&fs::read(&mpath)?)?;
                    if m.platform != Platform::Gc {
                        continue;
                    }
                    for e in &m.entries {
                        // share packs (HW_share, HW_Cars, HW_Items...)
                        if e.kind == hwvx_xfile::format::PackType(8) && e.id == id && e.layout == Layout::Split {
                            found = Some(pack_texture_images(&mpath.parent().unwrap().join(&e.path), m.platform)?);
                            break 'search;
                        }
                    }
                }
            }
            cache.insert(id, found);
        }
        if let Some(Some(t)) = cache.get(&id) {
            out.insert(id, t.clone());
        }
    }
    Ok(out)
}

fn cmd_export_gltf(
    input: &Path,
    output: Option<PathBuf>,
    share: &[PathBuf],
    textures: Option<&Path>,
    opts: &hwvx_xfile::gltf::Options,
    force: bool,
) -> Result<ExitCode> {
    use hwvx_xfile::format::PackType;
    let input = if input.file_name().is_some_and(|n| n == MANIFEST_FILE) { input.parent().unwrap_or(Path::new(".")) } else { input };
    // (pack folder, its manifest entry type) to export, and the archive root
    let archive = input.join(MANIFEST_FILE).is_file();
    let root = if archive { input.to_path_buf() } else { input.parent().unwrap_or(Path::new(".")).to_path_buf() };
    let manifest_path = root.join(MANIFEST_FILE);
    let manifest = parse_manifest(&fs::read(&manifest_path).with_context(|| {
        format!("reading {} (expected an extracted pack folder or archive folder)", manifest_path.display())
    })?)?;
    if manifest.platform != Platform::Gc {
        bail!("{} is a {} archive; export-gltf reads GameCube packs", root.display(), manifest.platform);
    }
    let packs: Vec<(PathBuf, PackType)> = manifest
        .entries
        .iter()
        .filter(|e| e.layout == Layout::Split)
        .map(|e| (root.join(&e.path), e.kind))
        .filter(|(p, _)| archive || p.file_name() == input.file_name())
        .collect();
    if packs.is_empty() {
        bail!("{} is not a split pack of {}", input.display(), root.display());
    }
    let out_dir = if archive {
        let d = output.clone().unwrap_or_else(|| PathBuf::from(format!("{}_gltf", input.display())));
        fs::create_dir_all(&d)?;
        Some(d)
    } else {
        None
    };

    // textures for quad (geometry) packs: a world texture pack folder, or the
    // first one in an archive folder
    let world_tex = match textures {
        None => vec![],
        Some(t) => {
            let dir = if t.join(MANIFEST_FILE).is_file() {
                let m = parse_manifest(&fs::read(t.join(MANIFEST_FILE))?)?;
                let e = m.entries.iter().find(|e| e.kind == PackType(6) || e.kind == PackType(4))
                    .with_context(|| format!("{} has no world texture pack", t.display()))?;
                t.join(&e.path)
            } else {
                t.to_path_buf()
            };
            pack_texture_images(&dir, pack_platform(&dir)?)?
        }
    };

    let mut roots = vec![root.clone()];
    roots.extend(share.iter().cloned());
    let mut cache = std::collections::HashMap::new();
    let mut written = 0;
    for (dir, kind) in &packs {
        let sp = read_split(dir)?;
        let blob = hwvx_xfile::blob::Blob::from_split(Endian::Big, &sp);
        let local = if sp.is_quad() { world_tex.clone() } else { pack_texture_images(dir, Platform::Gc)? };
        let tex = hwvx_xfile::gltf::Textures { local, shared: shared_textures(&blob, &roots, &mut cache)? };
        let name = file_name(dir);
        let path = match &out_dir {
            Some(d) => d.join(format!("{name}.glb")),
            None => output.clone().unwrap_or_else(|| dir.with_extension("glb")),
        };
        let (glb, rep) = match hwvx_xfile::gltf::export(&blob, &name, kind.0 == 0, &tex, opts) {
            Ok(r) => r,
            Err(e) if archive => {
                let _ = e; // packs without models (sounds, interfaces...)
                continue;
            }
            Err(e) => return Err(e.context(dir.display().to_string())),
        };
        if path.exists() && !force {
            bail!("{} exists (pass --force to overwrite)", path.display());
        }
        fs::write(&path, glb).with_context(|| format!("writing {}", path.display()))?;
        written += 1;
        out!("{}: {} meshes, {} triangles, {} textures", path.display(), rep.meshes, rep.triangles, rep.textures);
        for w in &rep.warnings {
            out!("  warning: {w}");
        }
    }
    if written == 0 {
        bail!("no models found in {}", input.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_patch_exe(exe: &Path, revert: bool) -> Result<ExitCode> {
    use hwvx_xfile::exepatch;
    let mut bytes = fs::read(exe).with_context(|| format!("reading {}", exe.display()))?;
    if revert {
        exepatch::revert(&mut bytes)?;
        fs::write(exe, &bytes)?;
        out!("{}: patch removed", exe.display());
        return Ok(ExitCode::SUCCESS);
    }
    if exepatch::state(&bytes)? == exepatch::State::Patched {
        out!("{}: already patched", exe.display());
        return Ok(ExitCode::SUCCESS);
    }
    let mut backup = exe.as_os_str().to_owned();
    backup.push(".orig");
    let backup = PathBuf::from(backup);
    if !backup.exists() {
        fs::write(&backup, &bytes).with_context(|| format!("writing {}", backup.display()))?;
        out!("backup: {}", backup.display());
    }
    for what in exepatch::apply(&mut bytes)? {
        out!("  {what}");
    }
    fs::write(exe, &bytes).with_context(|| format!("writing {}", exe.display()))?;
    out!("{}: patched (cars 33-35 unlocked)", exe.display());
    Ok(ExitCode::SUCCESS)
}
