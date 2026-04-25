//! NeoVM instruction peephole / local optimization passes.
//!
//! ## When to run
//! Passes run on each [`super::CompiledFunction`]'s `instructions` **before**
//! [`super::CompiledSourceFile::link_call_l_patches`]: `call_patches` store **instruction indices**
//! into the same `Vec<Instruction>`. Any removal at index `< patch_idx` must decrement that patch
//! index by the number of instructions removed.
//!
//! ## What not to do here
//! - Do not remove or reorder instructions without adjusting **all** branch targets
//!   (`JMP_L`, `JMPIF_L`, …) and [`OpCode::CALL_L`] operands (those are patched in bytes before
//!   link; indices still shift if you edit the vec earlier — this module only runs **pre-link**).
//! - Adjacent `STLOC n` then `LDLOC n` is removed **only** when no later `LDLOC n` appears before the
//!   next `STLOC n` (so the slot write was only consumed by that reload; the value stays on stack).
//!
//! ## Adding a new pass
//! 1. Implement a `fn try_xxx(inst: &mut Vec<Instruction>, patches: &mut Vec<(usize, String)>) -> bool`
//!    that returns `true` if it changed anything.
//! 2. Call it from [`Optimizer::optimize`] for [`CompiledFunction`] in a fixed-point loop or ordered pipeline.
//!
//! ## Current passes
//! - Adjacent duplicate `LDLOC n` / `LDARG n` → second becomes [`OpCode::DUP`] (same stack effect).
//! - [`OpCode::DUP`] then [`OpCode::DROP`] removed when no [`OpCode::CALL_L`] patch targets those indices.
//! - Adjacent `STLOC n` then `LDLOC n` removed when the store is only for that reload (see
//!   [`peel_redundant_stloc_ldloc_pair`]).

use crate::codegen::{CompiledFunction, CompiledSourceFile};
use crate::target::opcode::OpCode;
use crate::target::Instruction;

pub trait Optimizer {
    fn optimize(&mut self);
}

impl Optimizer for CompiledSourceFile {
    fn optimize(&mut self) {
        for f in &mut self.package_functions {
            f.optimize();
        }
        for f in &mut self.struct_methods {
            f.optimize();
        }
        for f in &mut self.contract_methods {
            f.optimize();
        }
    }
}

/// Any [`CompiledFunction`] has a `call_patches` entry pointing **at** that instruction index.
fn call_patch_touches_range(patches: &[(usize, String)], start: usize, len: usize) -> bool {
    let end = start + len;
    patches
        .iter()
        .any(|(index, _)| *index >= start && *index < end)
}

/// After removing `count` instructions starting at `removed_at`, shift patch indices down.
fn shift_call_patches_after_remove(
    patches: &mut Vec<(usize, String)>,
    removed_at: usize,
    count: usize,
) {
    let threshold = removed_at + count;
    for (index, _) in patches.iter_mut() {
        if *index >= threshold {
            *index -= count;
        }
    }
}

/// NeoVM slot index for `STLOC` short forms and `STLOC` with one operand.
fn store_slot(op: OpCode, operands: &[u8]) -> Option<u8> {
    match op {
        OpCode::STLOC0 => Some(0),
        OpCode::STLOC1 => Some(1),
        OpCode::STLOC2 => Some(2),
        OpCode::STLOC3 => Some(3),
        OpCode::STLOC4 => Some(4),
        OpCode::STLOC5 => Some(5),
        OpCode::STLOC6 => Some(6),
        OpCode::STLOC if operands.len() == 1 => Some(operands[0]),
        _ => None,
    }
}

/// NeoVM slot index for `LDLOC` / `LDARG` short forms and `LDLOC` / `LDARG` with one operand.
/// Only local loads (not `LDARG` — parameter slots are a different namespace).
fn load_local_slot(op: OpCode, operands: &[u8]) -> Option<u8> {
    match op {
        OpCode::LDLOC0 => Some(0),
        OpCode::LDLOC1 => Some(1),
        OpCode::LDLOC2 => Some(2),
        OpCode::LDLOC3 => Some(3),
        OpCode::LDLOC4 => Some(4),
        OpCode::LDLOC5 => Some(5),
        OpCode::LDLOC6 => Some(6),
        OpCode::LDLOC if operands.len() == 1 => Some(operands[0]),
        _ => None,
    }
}

fn load_slot(op: OpCode, operands: &[u8]) -> Option<u8> {
    match op {
        OpCode::LDLOC0 | OpCode::LDARG0 => Some(0),
        OpCode::LDLOC1 | OpCode::LDARG1 => Some(1),
        OpCode::LDLOC2 | OpCode::LDARG2 => Some(2),
        OpCode::LDLOC3 | OpCode::LDARG3 => Some(3),
        OpCode::LDLOC4 | OpCode::LDARG4 => Some(4),
        OpCode::LDLOC5 | OpCode::LDARG5 => Some(5),
        OpCode::LDLOC6 | OpCode::LDARG6 => Some(6),
        OpCode::LDLOC | OpCode::LDARG if operands.len() == 1 => Some(operands[0]),
        _ => None,
    }
}

/// `LDLOC n; LDLOC n` (or `LDARG n; LDARG n`) has the same stack effect as `…; DUP`.
/// Replace the second load with `DUP` (same instruction count; cheaper and shorter bytecode for `LDLOC`+operand).
fn merge_adjacent_duplicate_loads(
    inst: &mut Vec<Instruction>,
    patches: &mut Vec<(usize, String)>,
) -> bool {
    let mut changed = false;
    let mut index = 0;
    while index + 1 < inst.len() {
        let first = &inst[index];
        let second = &inst[index + 1];
        let first_slot = load_slot(first.opcode, &first.operands);
        let second_slot = load_slot(second.opcode, &second.operands);
        if first_slot.is_some() && first_slot == second_slot {
            // Do not rewrite if a patch targets either instruction (keep exact opcode for tooling/debug).
            if call_patch_touches_range(patches, index, 2) {
                index += 1;
                continue;
            }
            inst[index + 1] = Instruction {
                opcode: OpCode::DUP,
                operands: vec![],
            };
            changed = true;
            // Allow `LDLOC; LDLOC; LDLOC` → `LDLOC; DUP; DUP` in one forward pass.
            index += 1;
            continue;
        }
        index += 1;
    }
    changed
}

/// `DUP` then `DROP`: stack depth unchanged, no storage side effects — safe to delete both.
fn peel_redundant_dup_drop(
    inst: &mut Vec<Instruction>,
    patches: &mut Vec<(usize, String)>,
) -> bool {
    let mut changed = false;
    let mut index = 0;
    while index + 1 < inst.len() {
        if inst[index].opcode == OpCode::DUP && inst[index + 1].opcode == OpCode::DROP {
            if call_patch_touches_range(patches, index, 2) {
                index += 1;
                continue;
            }
            inst.drain(index..index + 2);
            shift_call_patches_after_remove(patches, index, 2);
            changed = true;
            // Re-scan from same `i` (may chain DUP/DROP/DUP/DROP).
            continue;
        }
        index += 1;
    }
    changed
}

/// `STLOC n` then `LDLOC n` with nothing else needing slot `n` until the next `STLOC n` is a no-op:
/// the value is already on stack before the pair (same net stack as after `LDLOC n`).
fn peel_redundant_stloc_ldloc_pair(
    inst: &mut Vec<Instruction>,
    patches: &mut Vec<(usize, String)>,
) -> bool {
    let mut changed = false;
    let mut index = 0;
    while index + 1 < inst.len() {
        let first = &inst[index];
        let second = &inst[index + 1];
        let first_slot = store_slot(first.opcode, &first.operands);
        let second_slot = load_local_slot(second.opcode, &second.operands);
        if first_slot != second_slot || first_slot.is_none() {
            index += 1;
            continue;
        }
        let n = first_slot.expect("first_slot==second_slot checked");
        if call_patch_touches_range(patches, index, 2) {
            index += 1;
            continue;
        }
        // Any later LDLOC n before the next STLOC n needs this store to remain.
        let mut next = index + 2;
        let mut ok = true;
        while next < inst.len() {
            if load_local_slot(inst[next].opcode, &inst[next].operands) == Some(n) {
                ok = false;
                break;
            }
            if store_slot(inst[next].opcode, &inst[next].operands) == Some(n) {
                break;
            }
            next += 1;
        }
        if !ok {
            index += 1;
            continue;
        }
        inst.drain(index..index + 2);
        shift_call_patches_after_remove(patches, index, 2);
        changed = true;
        continue;
    }
    changed
}

impl Optimizer for CompiledFunction {
    fn optimize(&mut self) {
        let mut round = 0usize;
        const MAX_ROUNDS: usize = 64;
        while round < MAX_ROUNDS {
            round += 1;
            let any_merged =
                merge_adjacent_duplicate_loads(&mut self.instructions, &mut self.call_patches);
            let any_dropped =
                peel_redundant_dup_drop(&mut self.instructions, &mut self.call_patches);
            let any_stloc_ldloc_paired =
                peel_redundant_stloc_ldloc_pair(&mut self.instructions, &mut self.call_patches);
            if !any_merged && !any_dropped && !any_stloc_ldloc_paired {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::Builder;

    #[test]
    fn dup_drop_removed_and_patch_shifted() {
        let mut inst: Vec<Instruction> = vec![
            Instruction {
                opcode: OpCode::PUSH1,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::DUP,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::DROP,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::PUSH2,
                operands: vec![],
            },
        ];
        let mut patches = vec![(3usize, "callee".into())];
        assert!(peel_redundant_dup_drop(&mut inst, &mut patches));
        assert_eq!(inst.len(), 2);
        assert_eq!(inst[0].opcode, OpCode::PUSH1);
        assert_eq!(inst[1].opcode, OpCode::PUSH2);
        assert_eq!(patches, vec![(1, "callee".into())]);
    }

    #[test]
    fn adjacent_duplicate_ldloc_becomes_dup() {
        let mut inst = vec![
            Instruction {
                opcode: OpCode::LDLOC0,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::LDLOC0,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::ADD,
                operands: vec![],
            },
        ];
        let mut patches: Vec<(usize, String)> = vec![];
        assert!(merge_adjacent_duplicate_loads(&mut inst, &mut patches));
        assert_eq!(inst[0].opcode, OpCode::LDLOC0);
        assert_eq!(inst[1].opcode, OpCode::DUP);
        assert_eq!(inst[2].opcode, OpCode::ADD);
    }

    #[test]
    fn dup_drop_skipped_when_patch_points_at_dup() {
        let mut inst = vec![
            Instruction {
                opcode: OpCode::DUP,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::DROP,
                operands: vec![],
            },
        ];
        let mut patches = vec![(0usize, "x".into())];
        assert!(!peel_redundant_dup_drop(&mut inst, &mut patches));
        assert_eq!(inst.len(), 2);
    }

    #[test]
    fn builder_dup_drop_roundtrip() {
        let mut b = Builder::new();
        b.push_int(1);
        b.emit(OpCode::DUP);
        b.emit(OpCode::DROP);
        b.push_int(2);
        let mut inst = b.into_instructions();
        let mut patches: Vec<(usize, String)> = vec![];
        assert!(peel_redundant_dup_drop(&mut inst, &mut patches));
        assert_eq!(inst.len(), 2);
    }

    #[test]
    fn peel_stloc_ldloc_when_no_later_reload_of_slot() {
        let mut inst = vec![
            Instruction {
                opcode: OpCode::CALL_L,
                operands: vec![0, 0, 0, 0],
            },
            Instruction {
                opcode: OpCode::STLOC4,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::LDLOC4,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::PUSH5,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::EQUAL,
                operands: vec![],
            },
        ];
        let mut patches = vec![(0usize, "Point::distanceTo".into())];
        assert!(peel_redundant_stloc_ldloc_pair(&mut inst, &mut patches));
        assert_eq!(inst.len(), 3);
        assert_eq!(inst[0].opcode, OpCode::CALL_L);
        assert_eq!(inst[1].opcode, OpCode::PUSH5);
        assert_eq!(inst[2].opcode, OpCode::EQUAL);
        assert_eq!(patches[0].0, 0);
    }

    #[test]
    fn peel_stloc_ldloc_skipped_when_slot_reloaded_later() {
        let mut inst = vec![
            Instruction {
                opcode: OpCode::STLOC4,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::LDLOC4,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::DROP,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::LDLOC4,
                operands: vec![],
            },
        ];
        let mut patches: Vec<(usize, String)> = vec![];
        assert!(!peel_redundant_stloc_ldloc_pair(&mut inst, &mut patches));
        assert_eq!(inst.len(), 4);
    }
}
