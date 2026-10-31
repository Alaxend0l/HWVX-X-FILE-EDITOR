//! Dev check: write the walker's swap plan for an extracted pack folder
//! (1 = word, 2 = half, 3 = byte, per blob byte).
use hwvx_xfile::blob::{Blob, Kind};
use hwvx_xfile::format::Endian;
use hwvx_xfile::pack::{PackJson, SplitPack};
use std::collections::BTreeMap;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&a[1]);
    let e = if a[2] == "le" { Endian::Little } else { Endian::Big };
    let json: PackJson = serde_json::from_slice(&std::fs::read(dir.join("pack.json")).unwrap()).unwrap();
    let mut sections = BTreeMap::new();
    for n in ["data", "pointers", "texture_relocs", "sound_relocs", "ref_relocs", "shader_relocs"] {
        if let Ok(b) = std::fs::read(dir.join(format!("{n}.bin"))) { sections.insert(n.to_string(), b); }
    }
    let blob = Blob::from_split(e, &SplitPack { json, sections });
    let w = hwvx_xfile::convert::carwalk::walk(&blob, e);
    let out: Vec<u8> = w.swap.kinds.iter().map(|k| match k { Kind::Word | Kind::Unknown => 1, Kind::Half => 2, Kind::Byte => 3 }).collect();
    std::fs::write(&a[3], out).unwrap();
    eprintln!("geometry roots {} car {:?}", w.geometry_roots.len(), w.car_info);
}
