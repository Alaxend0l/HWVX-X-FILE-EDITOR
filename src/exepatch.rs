//! Patch the PC `Hot Wheels.exe` so cars added after the 33 retail ones
//! (HW_Cars ids 33-35) count as unlocked.
//!
//! The game keeps one unlock flag per car (code variable 0x64) in an array of
//! 33 (`HotWheelsGameData.cars[0x21]`, VA 0x64D300); both accessors refuse
//! ids >= 0x21. The patch redirects those two bounds checks into unused
//! alignment padding (15 NOP bytes after a `ret`, nothing jumps there):
//!
//! * get (VA 0x42A680, check at 0x42A704): ids 33-35 return 1 (unlocked),
//!   ids >= 36 still return 0.
//! * set (VA 0x429FA0, check at 0x42A00C): ids 33-35 are stored in the
//!   unused padding after the track flags (`pad5A[0..2]`, the same place as
//!   the HWVX-SDL change), ids >= 36 are still ignored.
//!
//! Only relative jumps are used, so no relocation entries are needed. Every
//! patched byte is checked against the retail exe first.

use anyhow::{bail, Result};

/// (file offset, retail bytes, patched bytes, what it does)
const EDITS: &[(usize, &[u8], &[u8], &str)] = &[
    (
        0x02A707,
        &[0x0f, 0x83, 0x1c, 0x01, 0x00, 0x00],
        &[0x0f, 0x83, 0x44, 0xf7, 0xff, 0xff],
        "get: ids >= 33 -> cave A",
    ),
    (
        0x029E51,
        &[0x90; 14],
        // cmp eax,0x24 ; jb 0x42a6cf (return 1) ; jmp 0x42a829 (return 0)
        &[0x83, 0xf8, 0x24, 0x0f, 0x82, 0x75, 0x08, 0x00, 0x00, 0xe9, 0xca, 0x09, 0x00, 0x00],
        "cave A: 33-35 unlocked",
    ),
    (
        0x02A00F,
        &[0x0f, 0x83, 0xdf, 0x04, 0x00, 0x00],
        &[0x0f, 0x83, 0xdc, 0x18, 0x00, 0x00],
        "set: ids >= 33 -> cave B",
    ),
    (
        0x02B8F1,
        &[0x90; 15],
        // cmp eax,0x24 ; jae ->(pop esi; ret) ; add eax,0x39 ; jmp 0x42a015 (store)
        &[0x83, 0xf8, 0x24, 0x73, 0x08, 0x83, 0xc0, 0x39, 0xe9, 0x17, 0xe7, 0xff, 0xff, 0x5e, 0xc3],
        "cave B: 33-35 stored in spare save bytes",
    ),
];

#[derive(Debug, PartialEq, Eq)]
pub enum State {
    Retail,
    Patched,
}

/// Is `exe` the retail exe, already patched, or something else?
pub fn state(exe: &[u8]) -> Result<State> {
    let at = |off: usize, b: &[u8]| exe.get(off..off + b.len()) == Some(b);
    if EDITS.iter().all(|(o, old, _, _)| at(*o, old)) {
        Ok(State::Retail)
    } else if EDITS.iter().all(|(o, _, new, _)| at(*o, new)) {
        Ok(State::Patched)
    } else {
        bail!("this is not the retail PC Hot Wheels.exe this patch was made for (bytes differ at the patch sites)")
    }
}

/// Apply the patch; returns the descriptions of the edits.
pub fn apply(exe: &mut [u8]) -> Result<Vec<&'static str>> {
    match state(exe)? {
        State::Patched => bail!("the exe is already patched"),
        State::Retail => {}
    }
    for (off, _, new, _) in EDITS {
        exe[*off..*off + new.len()].copy_from_slice(new);
    }
    Ok(EDITS.iter().map(|e| e.3).collect())
}

/// Undo the patch.
pub fn revert(exe: &mut [u8]) -> Result<()> {
    match state(exe)? {
        State::Retail => bail!("the exe is not patched"),
        State::Patched => {}
    }
    for (off, old, _, _) in EDITS {
        exe[*off..*off + old.len()].copy_from_slice(old);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_have_matching_lengths() {
        for (_, old, new, what) in EDITS {
            assert_eq!(old.len(), new.len(), "{what}");
        }
    }

    #[test]
    fn apply_and_revert_roundtrip() {
        let mut exe = vec![0u8; 0x30000];
        for (off, old, _, _) in EDITS {
            exe[*off..*off + old.len()].copy_from_slice(old);
        }
        let orig = exe.clone();
        apply(&mut exe).unwrap();
        assert_eq!(state(&exe).unwrap(), State::Patched);
        assert!(apply(&mut exe).is_err());
        revert(&mut exe).unwrap();
        assert_eq!(exe, orig);
    }
}
