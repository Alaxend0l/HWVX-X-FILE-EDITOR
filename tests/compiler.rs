//! End-to-end tests for the vendor-first compiler (roadmap M1).
//! Fixtures are synthetic split packs; outputs go under `target/tmp/`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use hwvx_xfile::archive::Archive;
use hwvx_xfile::compiler::{build_project, BuildOutcome, PROJECT_FILE_NAME};
use hwvx_xfile::format::{PackType, Platform};
use hwvx_xfile::hex::Hex;
use hwvx_xfile::pack::{self, PackHeader, PackJson, PackRef, SplitPack, StdHeader};
use hwvx_xfile::xnc::{is_xnc, XncFile};
use hwvx_xfile::{Layout, Manifest, ManifestEntry, MANIFEST_FILE, PACK_FILE};

fn ws(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/compiler-tests")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn codes(o: &BuildOutcome) -> Vec<String> {
    o.report.diagnostics.iter().map(|d| d.code.clone()).collect()
}

fn ptype(n: &str) -> PackType {
    PackType::from_name(n).unwrap()
}

/// Minimal standard pack; byte values shift with `base` so packs differ.
fn std_pack(base: u8) -> SplitPack {
    let r = |name: &str, id: u32| PackRef {
        name: name.into(),
        id,
        name_raw: Default::default(),
        runtime_slot: 0,
    };
    let mut sections: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    sections.insert("sound_pointers".into(), vec![base; 12]);
    sections.insert("sound_data".into(), vec![base + 1; 77]);
    sections.insert("texture_relocs".into(), vec![base + 2; 16]);
    sections.insert("data".into(), vec![base + 3; 50]);
    sections.insert("ref_relocs".into(), vec![base + 4; 8]);
    sections.insert("pointers".into(), vec![base + 5; 20]);
    SplitPack {
        json: PackJson {
            header: PackHeader::Standard(StdHeader {
                num_textures: 1,
                texture_data_offset: 8,
                num_sounds: 2,
                sfx_offset: 0,
                num_palettes: 0,
                palette_offset: 0,
                num_shaders: 0,
                shader_offset: 0,
                num_triggers: 0,
                trigger_offset: 0,
                song_offset: 0,
                song_bank_offset: 0,
                refs: [r("HW_share", 1), r("", 0)],
            }),
            padding: BTreeMap::new(),
            sections: vec![],
        },
        sections,
    }
}

/// Write `pack.json` + section files; returns the bytes `join(Pc)` produces.
fn write_split_folder(dir: &Path, sp: &SplitPack) -> Vec<u8> {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(PACK_FILE), serde_json::to_vec_pretty(&sp.json).unwrap()).unwrap();
    for (name, bytes) in &sp.sections {
        fs::write(dir.join(format!("{name}.bin")), bytes).unwrap();
    }
    pack::join(Platform::Pc, sp).unwrap()
}

fn write_manifest(dir: &Path, platform: Platform, entries: &[ManifestEntry]) {
    fs::create_dir_all(dir).unwrap();
    let m = Manifest {
        tool: "test".into(),
        manifest_version: 1,
        source_file: "files.xpc".into(),
        platform,
        archive_version: 183,
        entries: entries.to_vec(),
        tail_fill: Hex::default(),
    };
    fs::write(dir.join(MANIFEST_FILE), serde_json::to_vec_pretty(&m).unwrap()).unwrap();
}

fn manifest_entry(path: &str, ty: &str, id: u32, layout: Layout) -> ManifestEntry {
    ManifestEntry {
        kind: ptype(ty),
        id,
        sub_id: 0,
        version: 183,
        path: path.into(),
        layout,
        summary: String::new(),
        gap_fill: Hex::default(),
    }
}

fn write_project(dir: &Path, value: &serde_json::Value) -> PathBuf {
    let p = dir.join(PROJECT_FILE_NAME);
    fs::write(&p, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    p
}

fn build(dir: &Path, selects: &[&str], force: bool) -> BuildOutcome {
    let selects: Vec<String> = selects.iter().map(|s| s.to_string()).collect();
    build_project(&dir.join(PROJECT_FILE_NAME), &selects, &dir.join("build"), force)
}

#[test]
fn vendor_first_build_emits_both_containers() {
    let root = ws("happy");
    // Vendor base (pc): world 0 kept raw, interface 0 split.
    let vendor = root.join("vendor/pc");
    fs::create_dir_all(&vendor).unwrap();
    let world = vec![0xA5u8; 100];
    fs::write(vendor.join("00_world_0.bin"), &world).unwrap();
    let iface0 = write_split_folder(&vendor.join("04_interface_0"), &std_pack(10));
    write_manifest(
        &vendor,
        Platform::Pc,
        &[
            manifest_entry("00_world_0.bin", "world", 0, Layout::Raw),
            manifest_entry("04_interface_0", "interface", 0, Layout::Split),
        ],
    );
    // Project-owned sources.
    let iface1 = write_split_folder(&root.join("mywork/04_interface_1"), &std_pack(20));
    fs::create_dir_all(root.join("blobs")).unwrap();
    let blob = vec![0x5Cu8; 64];
    fs::write(root.join("blobs/patch.bin"), &blob).unwrap();

    write_project(
        &root,
        &json!({
            "schema_version": 1,
            "name": "happy",
            "default_platform": "pc",
            "targets": [ { "name": "pc" }, { "name": "pc-xnc", "container": "xnc" } ],
            "archives": [ {
                "output": "files",
                "base": { "from": "vendor/pc", "exclude": ["world:0"] },
                "packs": [
                    { "kind": "hwvx_pack", "path": "mywork/04_interface_1" },
                    { "kind": "raw", "path": "blobs/patch.bin", "type": "car", "id": 41 }
                ]
            } ]
        }),
    );

    let o = build(&root, &[], false);
    assert_eq!(o.exit, 0, "diags: {:?}", codes(&o));
    assert_eq!(o.report.targets, ["pc", "pc-xnc"]);
    assert_eq!(o.report.archives.len(), 2);

    // Classic container: base pack (joined again for pc) + edited pack + blob.
    let xpc = fs::read(root.join("build/files.xpc")).unwrap();
    let ar = Archive::parse(&xpc, Platform::Pc).unwrap();
    assert_eq!(ar.entries.len(), 3);
    assert_eq!((ar.entries[0].kind, ar.entries[0].id), (ptype("interface"), 0));
    assert_eq!(ar.entries[0].data, iface0);
    assert_eq!((ar.entries[1].id, ar.entries[1].data.as_slice()), (1, iface1.as_slice()));
    assert_eq!((ar.entries[2].kind, ar.entries[2].id), (ptype("car"), 41));
    assert_eq!(ar.entries[2].data, blob);

    // XNC container: same payloads, named TOC entries.
    let xnc = fs::read(root.join("build/files.xnc")).unwrap();
    assert!(is_xnc(&xnc));
    let xf = XncFile::parse(&xnc).unwrap();
    assert_eq!(xf.entries.len(), 3);
    assert_eq!(xf.entries[0].name.as_deref(), Some("HW_INTERFACE"));
    assert_eq!(xf.entries[0].data, iface0);
    assert_eq!(xf.entries[2].name.as_deref(), Some("HW_CAR"));
    assert_eq!(xf.entries[2].data, blob);
    assert_eq!(o.report.archives[0].container, "xarchive");
    assert_eq!(o.report.archives[1].container, "xnc");
}

#[test]
fn base_exclusion_force_and_platform_checks() {
    let root = ws("base");
    let vendor = root.join("vendor/gc");
    fs::create_dir_all(&vendor).unwrap();
    fs::write(vendor.join("00_world_0.bin"), vec![1u8; 40]).unwrap();
    write_split_folder(&vendor.join("04_interface_0"), &std_pack(30));
    write_manifest(
        &vendor,
        Platform::Gc,
        &[
            manifest_entry("00_world_0.bin", "world", 0, Layout::Raw),
            manifest_entry("04_interface_0", "interface", 0, Layout::Split),
        ],
    );
    write_project(
        &root,
        &json!({
            "schema_version": 1,
            "name": "base",
            "targets": [ { "name": "gc", "platform": "gc" } ],
            "archives": [
                { "output": "a", "base": { "from": "vendor/gc",
                    "exclude": ["world:0", "04_interface_0"] } },
                { "output": "b", "base": { "from": "vendor/gc", "exclude": ["world"] } }
            ]
        }),
    );

    // `a` excludes everything -> E0031; `b` keeps the interface pack only.
    let o = build(&root, &[], false);
    assert_eq!(o.exit, 1);
    assert!(codes(&o).contains(&"E0031".to_string()));
    let b = fs::read(root.join("build/b.xgc")).unwrap();
    assert_eq!(Archive::parse(&b, Platform::Gc).unwrap().entries.len(), 1);
    assert!(!root.join("build/a.xgc").exists());

    // A PC target against the GC base is refused until M3.
    write_project(
        &root,
        &json!({
            "schema_version": 1, "name": "base",
            "targets": [ { "name": "pc", "platform": "pc" } ],
            "archives": [ { "output": "b", "base": { "from": "vendor/gc" } } ]
        }),
    );
    let o = build(&root, &[], false);
    assert_eq!(o.exit, 1);
    assert!(codes(&o).contains(&"E0041".to_string()));

    // Output exists -> E0033 without force, gone with force.
    write_project(
        &root,
        &json!({
            "schema_version": 1, "name": "base",
            "targets": [ { "name": "gc", "platform": "gc" } ],
            "archives": [ { "output": "b", "base": { "from": "vendor/gc", "exclude": ["world"] } } ]
        }),
    );
    let o = build(&root, &[], false);
    assert!(codes(&o).contains(&"E0033".to_string()));
    let o = build(&root, &[], true);
    assert_eq!(o.exit, 0, "diags: {:?}", codes(&o));
}

#[test]
fn diagnostics_and_exit_codes() {
    let check = |name: &str, project: serde_json::Value, files: Vec<(&str, Vec<u8>)>,
                 selects: &[&str], exit: u8, want: &str| {
        let root = ws(name);
        for (p, bytes) in files {
            let p = root.join(p);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, bytes).unwrap();
        }
        write_project(&root, &project);
        let o = build(&root, selects, false);
        assert_eq!(o.exit, exit, "{name}: {:?}", codes(&o));
        assert!(codes(&o).contains(&want.to_string()), "{name}: {:?}", codes(&o));
        o
    };
    let blob = vec![0x11u8; 32];
    let one_raw = |packs: serde_json::Value| {
        json!({ "schema_version": 1, "name": "t", "targets": [{ "name": "pc", "platform": "pc" }],
                "archives": [{ "output": "files", "packs": packs }] })
    };
    let sp_json = serde_json::to_vec(&std_pack(40).json).unwrap();

    check("parse", json!({ "not": "a project" }), vec![], &[], 2, "E0001");
    let sp = one_raw(json!([ { "kind": "raw", "path": "b.bin", "type": "car", "id": 1 } ]));
    check("target", sp.clone(), vec![("b.bin", blob.clone())], &["nope"], 2, "E0010");
    check("dup", one_raw(json!([
        { "kind": "raw", "path": "b.bin", "type": "car", "id": 1 },
        { "kind": "raw", "path": "b.bin", "type": "car", "id": 1 }
    ])), vec![("b.bin", blob.clone())], &[], 1, "E0030");
    check("future", one_raw(json!([ { "kind": "car", "file": "c.json" } ])),
          vec![], &[], 1, "E0040");
    check("nostem", one_raw(json!([ { "kind": "hwvx_pack", "path": "misc" } ])),
          vec![("misc/pack.json", sp_json.clone())], &[], 1, "E0032");
    check("missing", one_raw(json!([ { "kind": "raw", "path": "gone.bin", "type": "car", "id": 1 } ])),
          vec![], &[], 2, "E0031");

    // Empty project name is only a warning: the build still succeeds.
    let mut ok = sp;
    ok["name"] = json!("");
    let o = check("warn", ok, vec![("b.bin", blob)], &[], 0, "W0001");
    assert_eq!(o.report.warnings, 1);
    assert_eq!(o.report.archives.len(), 1);
}
