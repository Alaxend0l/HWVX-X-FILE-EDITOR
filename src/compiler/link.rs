//! Linking: final checks over a lowered archive plan.
//!
//! Today this is the duplicate-key check the game cannot tolerate (the TOC
//! is a flat list; the loader takes the first match, so a duplicate would
//! silently shadow a pack). Later milestones add reference validation
//! (car -> geometry, world -> quad chains) here.

use crate::compiler::diag::{codes, Diagnostics};
use crate::compiler::ir::ArchivePlan;

/// Report duplicate `(type, id, sub_id)` keys in one archive.
pub fn check_plan(plan: &ArchivePlan, diags: &mut Diagnostics) {
    let mut seen: Vec<((u32, u32, u32), usize)> = Vec::new();
    for (i, p) in plan.packs.iter().enumerate() {
        let key = p.key();
        if let Some((_, first)) = seen.iter().find(|(k, _)| *k == key) {
            diags.error(
                codes::DUPLICATE_PACK,
                format!(
                    "archive {:?} target {:?}: pack {} appears twice ({} and {})",
                    plan.output,
                    plan.target_name,
                    p.key_string(),
                    plan.packs[*first].source.detail,
                    p.source.detail
                ),
            );
        } else {
            seen.push((key, i));
        }
    }
}
