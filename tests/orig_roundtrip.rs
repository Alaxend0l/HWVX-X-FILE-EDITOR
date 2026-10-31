//! Byte-exact extract + pack of every archive under `orig/` (pc, gc, ps2,
//! ps2-beta). Skipped when the game files are not present.
//! Run with `cargo test --release`.

use std::fs;
use std::path::{Path, PathBuf};

use hwvx_xfile::format::{is_archive_path, Platform};

fn archives(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            archives(&p, out);
        } else if is_archive_path(&p) {
            out.push(p);
        }
    }
}

#[test]
fn every_original_archive_round_trips() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("orig");
    let sets = [
        ("pc", Platform::Pc),
        ("gc", Platform::Gc),
        ("ps2", Platform::Ps2),
        ("ps2-beta", Platform::Ps2Beta),
    ];
    let mut checked = 0;
    for (dir, expected) in sets {
        let mut files = Vec::new();
        archives(&root.join(dir), &mut files);
        for f in files {
            let data = fs::read(&f).unwrap();
            let platform = Platform::detect(&data, Some(&f)).unwrap();
            assert_eq!(platform, expected, "{}", f.display());
            let raw = hwvx_xfile::verify(&data, platform, "x")
                .unwrap_or_else(|e| panic!("{}: {e:#}", f.display()));
            assert_eq!(raw, 0, "{}: packs kept raw", f.display());
            checked += 1;
        }
    }
    if checked == 0 {
        eprintln!("orig/ not found, skipped");
    }
}
