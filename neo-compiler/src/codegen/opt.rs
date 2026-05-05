//! NeoVM instruction peephole / local optimization passes.
//!
//! ## When to run
//! Passes run on each [`super::CompiledFunction`]'s `instructions` **before**
//! [`super::CompiledSourceFile::link_call_l_patches`]: `call_patches` store **instruction indices**
//! into the same `Vec<Instruction>`. Any removal at index `< patch_idx` must decrement that patch
//! index by the number of instructions removed.
//!
//! ## What not to do here
//! - Do not remove or reorder instructions without adjusting **all** branch targets (`JMP` /
//!   `JMP_L`, `JMPIF` / `JMPIF_L`, …, `CALL` / `CALL_L`, `TRY` / `TRY_L`, …) — operands are
//!   PC-relative byte offsets before link; indices still shift if you edit the vec earlier (this
//!   module only runs **pre-link**).
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
//! - Unconditional [`OpCode::JMP`] / [`OpCode::JMP_L`] whose relative offset equals “fall through” to
//!   the next instruction (same net control flow) is removed; every branch / call / try with a
//!   **1-byte** or **4-byte** PC-relative operand is re-encoded for the shorter script (see
//!   [`relayout_pc_relatives`]). If a short-form offset would no longer fit in `i8`, the redundant
//!   jump is not removed.
//! - Single-operand `*_L` / [`OpCode::ENDTRY_L`] are narrowed to their 1-byte opcode when the offset
//!   fits in `i8` (see [`shorten_long_pc_relative_branches`]). [`OpCode::CALL_L`] is left as the long
//!   form until [`super::CompiledSourceFile::link_call_l_patches`] (which patches 4-byte operands).
//!   [`OpCode::TRY_L`] is unchanged (two `i32` operands are not one contiguous shrink in the script layout).

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
#[allow(clippy::ptr_arg)]
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
#[allow(clippy::ptr_arg)]
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

/// Byte offset of each instruction's opcode in the serialized script (sum of prior `encoded_len()`).
fn prefix_byte_offsets(inst: &[Instruction]) -> Vec<usize> {
    let mut out = Vec::with_capacity(inst.len());
    let mut pc = 0usize;
    for ins in inst {
        out.push(pc);
        pc += ins.encoded_len();
    }
    out
}

/// Map a byte offset in the script **before** deleting `[del_start, del_start + del_len)` into the
/// new script.
///
/// The deleted span is always a redundant **unconditional** jump (fall-through to its successor).
/// Any branch that targeted the first byte of that jump (common for `JMPIFNOT_L` → else stub) or
/// any interior byte must land at the same place as executing that jump would: the successor’s
/// first byte, which after deletion sits at offset `del_start` in the new script.
fn map_byte_offset_after_delete(p: usize, del_start: usize, del_len: usize) -> usize {
    if p < del_start {
        p
    } else if p < del_start + del_len {
        del_start
    } else {
        p - del_len
    }
}

fn read_i32_le(b: &[u8]) -> Option<i32> {
    if b.len() < 4 {
        return None;
    }
    Some(i32::from_le_bytes(b[0..4].try_into().ok()?))
}

fn write_i32_le(out: &mut [u8], rel: i32) {
    out[0..4].copy_from_slice(&rel.to_le_bytes());
}

#[inline]
fn operand_as_i8(operands: &[u8]) -> Option<i8> {
    operands.first().copied().map(|b| b as i8)
}

/// Map an absolute byte offset after removing a **strict** contiguous half-open range
/// `[del_start, del_start + del_len)` with no interior landing semantics.
fn map_byte_offset_after_strict_remove(
    absolute_offset: usize,
    del_start: usize,
    del_len: usize,
) -> Option<usize> {
    if absolute_offset < del_start {
        Some(absolute_offset)
    } else if absolute_offset < del_start + del_len {
        None
    } else {
        Some(absolute_offset - del_len)
    }
}

/// Re-encode every PC-relative branch / call / try operand after a script edit. `old_index(j)` maps
/// each surviving instruction index `j` in `inst` to its index in the **pre-edit** `prefix_before`
/// table. `map_abs` maps an absolute byte offset in the pre-edit script to the post-edit script
/// (`None` ⇒ invalid / abort).
fn relayout_pc_relatives(
    inst: &mut [Instruction],
    prefix_before: &[usize],
    prefix_after: &[usize],
    mut old_index: impl FnMut(usize) -> usize,
    mut map_abs: impl FnMut(usize) -> Option<usize>,
) -> bool {
    debug_assert_eq!(prefix_after.len(), inst.len());

    for (j, instr) in inst.iter_mut().enumerate() {
        let old_o = old_index(j);
        let old_j_pc = prefix_before[old_o];
        let new_j_pc = prefix_after[j];
        match instr.opcode {
            OpCode::TRY => {
                if instr.operands.len() != 2 {
                    continue;
                }
                for offset in [0usize, 1usize] {
                    let relative = instr.operands[offset] as i8 as i32;
                    let abs = old_j_pc as i64 + relative as i64;
                    let Ok(abs) = usize::try_from(abs) else {
                        return false;
                    };
                    let Some(new_abs) = map_abs(abs) else {
                        return false;
                    };
                    let new_relative = new_abs as i64 - new_j_pc as i64;
                    let Ok(n) = i8::try_from(new_relative) else {
                        return false;
                    };
                    instr.operands[offset] = n as u8;
                }
            }
            _ if instr.opcode.is_change_pc_short() => {
                if instr.operands.len() != 1 {
                    continue;
                }
                let relative = match operand_as_i8(&instr.operands) {
                    Some(r) => r as i32,
                    None => continue,
                };
                let abs = old_j_pc as i64 + relative as i64;
                let Ok(abs) = usize::try_from(abs) else {
                    return false;
                };
                let Some(new_abs) = map_abs(abs) else {
                    return false;
                };
                let new_relative = new_abs as i64 - new_j_pc as i64;
                let Ok(n) = i8::try_from(new_relative) else {
                    return false;
                };
                instr.operands[0] = n as u8;
            }
            OpCode::TRY_L => {
                if instr.operands.len() != 8 {
                    continue;
                }
                let relative_catch = match read_i32_le(&instr.operands[0..4]) {
                    Some(r) => r,
                    None => continue,
                };
                let relative_finally = match read_i32_le(&instr.operands[4..8]) {
                    Some(r) => r,
                    None => continue,
                };
                for (offset, relative) in [(0usize, relative_catch), (4usize, relative_finally)] {
                    let abs = old_j_pc as i64 + relative as i64;
                    let Ok(abs) = usize::try_from(abs) else {
                        return false;
                    };
                    let Some(new_abs) = map_abs(abs) else {
                        return false;
                    };
                    let Ok(new_relative) = i32::try_from(new_abs as i64 - new_j_pc as i64) else {
                        return false;
                    };
                    write_i32_le(&mut instr.operands[offset..offset + 4], new_relative);
                }
            }
            _ if instr.opcode.is_change_pc_long() => {
                if instr.operands.len() != 4 {
                    continue;
                }
                let relative = match read_i32_le(&instr.operands) {
                    Some(r) => r,
                    None => continue,
                };
                let abs = old_j_pc as i64 + relative as i64;
                let Ok(abs) = usize::try_from(abs) else {
                    return false;
                };
                let Some(new_abs) = map_abs(abs) else {
                    return false;
                };
                let Ok(new_relative) = i32::try_from(new_abs as i64 - new_j_pc as i64) else {
                    return false;
                };
                write_i32_le(&mut instr.operands, new_relative);
            }
            _ => {}
        }
    }
    true
}

/// After deleting `del_len` bytes starting at `del_start` in the pre-delete script (whole
/// instruction removal), re-encode PC-relative operands.
fn relayout_branch_operands_after_byte_delete(
    inst: &mut [Instruction],
    prefix_before: &[usize],
    prefix_after: &[usize],
    del_start: usize,
    del_len: usize,
    removed_instr_index: usize,
) -> bool {
    debug_assert_eq!(prefix_before.len(), inst.len() + 1);
    relayout_pc_relatives(
        inst,
        prefix_before,
        prefix_after,
        |j| if j < removed_instr_index { j } else { j + 1 },
        |abs| Some(map_byte_offset_after_delete(abs, del_start, del_len)),
    )
}

fn long_pc_branch_to_short_opcode(op: OpCode) -> Option<OpCode> {
    match op {
        OpCode::JMP_L => Some(OpCode::JMP),
        OpCode::JMPIF_L => Some(OpCode::JMPIF),
        OpCode::JMPIFNOT_L => Some(OpCode::JMPIFNOT),
        OpCode::JMPEQ_L => Some(OpCode::JMPEQ),
        OpCode::JMPNE_L => Some(OpCode::JMPNE),
        OpCode::JMPGT_L => Some(OpCode::JMPGT),
        OpCode::JMPGE_L => Some(OpCode::JMPGE),
        OpCode::JMPLT_L => Some(OpCode::JMPLT),
        OpCode::JMPLE_L => Some(OpCode::JMPLE),
        OpCode::ENDTRY_L => Some(OpCode::ENDTRY),
        _ => None,
    }
}

/// Replace eligible `*_L` / [`OpCode::ENDTRY_L`] with 1-byte-operand forms when the offset fits in
/// `i8`. Operand layout shrinks by removing the high 3 bytes of the LE `i32` (script bytes
/// `[instr_start + 2, instr_start + 5)`). [`OpCode::CALL_L`] is excluded: link still patches
/// 4-byte [`OpCode::CALL_L`] operands.
fn try_shorten_long_pc_branch_at(inst: &mut Vec<Instruction>, index: usize) -> bool {
    let ins = &inst[index];
    let Some(short_op) = long_pc_branch_to_short_opcode(ins.opcode) else {
        return false;
    };
    if ins.operands.len() != 4 {
        return false;
    }
    let relative = match read_i32_le(&ins.operands) {
        Some(r) => r,
        None => return false,
    };
    if !(i8::MIN as i32..=i8::MAX as i32).contains(&relative) {
        return false;
    }

    let prefix_before = prefix_byte_offsets(inst);
    let instr_start = prefix_before[index];
    let del_start = instr_start + 2;
    let del_len = 3;

    let mut trial = inst.clone();
    trial[index] = Instruction {
        opcode: short_op,
        operands: vec![relative as i8 as u8],
    };
    let prefix_after = prefix_byte_offsets(&trial);
    if !relayout_pc_relatives(
        &mut trial,
        &prefix_before,
        &prefix_after,
        |j| j,
        |abs| map_byte_offset_after_strict_remove(abs, del_start, del_len),
    ) {
        return false;
    }
    *inst = trial;
    true
}

/// One forward pass: shorten every eligible long PC-relative branch.
fn shorten_long_pc_relative_branches(inst: &mut Vec<Instruction>) -> bool {
    let mut changed = false;
    for index in 0..inst.len() {
        if try_shorten_long_pc_branch_at(inst, index) {
            changed = true;
        }
    }
    changed
}

/// Unconditional [`OpCode::JMP`] / [`OpCode::JMP_L`] whose offset equals its own encoded length only
/// transfers to the next instruction (fall-through). Never applied to [`OpCode::CALL`] / [`OpCode::CALL_L`].
fn peel_redundant_uncond_jmp_to_next(
    inst: &mut Vec<Instruction>,
    patches: &mut Vec<(usize, String)>,
) -> bool {
    let mut changed = false;
    let mut index = 0;
    while index < inst.len() {
        let ins = &inst[index];
        let relative = match (&ins.opcode, ins.operands.len()) {
            (OpCode::JMP_L, 4) => match read_i32_le(&ins.operands) {
                Some(r) => r,
                None => {
                    index += 1;
                    continue;
                }
            },
            (OpCode::JMP, 1) => match operand_as_i8(&ins.operands) {
                Some(b) => b as i32,
                None => {
                    index += 1;
                    continue;
                }
            },
            _ => {
                index += 1;
                continue;
            }
        };

        let jmp_len = ins.encoded_len();
        let Ok(jmp_len) = i32::try_from(jmp_len) else {
            index += 1;
            continue;
        };
        if relative != jmp_len {
            index += 1;
            continue;
        }
        if index + 1 >= inst.len() {
            index += 1;
            continue;
        }
        if call_patch_touches_range(patches, index, 1) {
            index += 1;
            continue;
        }

        let prefix_before = prefix_byte_offsets(inst);
        let del_start = prefix_before[index];
        let del_len = inst[index].encoded_len();
        let mut trial = inst.clone();
        trial.drain(index..index + 1);
        let prefix_after = prefix_byte_offsets(&trial);
        if !relayout_branch_operands_after_byte_delete(
            &mut trial,
            &prefix_before,
            &prefix_after,
            del_start,
            del_len,
            index,
        ) {
            index += 1;
            continue;
        }

        *inst = trial;
        shift_call_patches_after_remove(patches, index, 1);
        changed = true;
        // Re-scan from this index (another redundant jmp may start here).
        continue;
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
            let any_jmp_to_next =
                peel_redundant_uncond_jmp_to_next(&mut self.instructions, &mut self.call_patches);
            let any_shortened = shorten_long_pc_relative_branches(&mut self.instructions);
            if !any_merged
                && !any_dropped
                && !any_stloc_ldloc_paired
                && !any_jmp_to_next
                && !any_shortened
            {
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

    #[test]
    fn peel_jmp_l_when_target_is_next_instruction() {
        let mut inst = vec![
            Instruction {
                opcode: OpCode::PUSH0,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::JMP_L,
                operands: 5i32.to_le_bytes().to_vec(),
            },
            Instruction {
                opcode: OpCode::PUSH2,
                operands: vec![],
            },
        ];
        let mut patches: Vec<(usize, String)> = vec![];
        assert!(super::peel_redundant_uncond_jmp_to_next(
            &mut inst,
            &mut patches
        ));
        assert_eq!(inst.len(), 2);
        assert_eq!(inst[0].opcode, OpCode::PUSH0);
        assert_eq!(inst[1].opcode, OpCode::PUSH2);
    }

    #[test]
    fn peel_redundant_jmp_when_conditional_targeted_its_first_byte() {
        // Else stub points at the first byte of the redundant `JMP_L` (same layout as IR codegen).
        let mut inst = vec![
            Instruction {
                opcode: OpCode::JMPIFNOT_L,
                operands: 5i32.to_le_bytes().to_vec(),
            },
            Instruction {
                opcode: OpCode::JMP_L,
                operands: 5i32.to_le_bytes().to_vec(),
            },
            Instruction {
                opcode: OpCode::PUSHT,
                operands: vec![],
            },
        ];
        let mut patches: Vec<(usize, String)> = vec![];
        assert!(super::peel_redundant_uncond_jmp_to_next(
            &mut inst,
            &mut patches
        ));
        assert_eq!(inst.len(), 2);
        assert_eq!(inst[0].opcode, OpCode::JMPIFNOT_L);
        assert_eq!(inst[1].opcode, OpCode::PUSHT);
        let rel = super::read_i32_le(&inst[0].operands).expect("JMPIFNOT_L");
        assert_eq!(rel, 5);
    }

    #[test]
    fn shorten_jmp_l_to_jmp_when_rel_fits_i8() {
        let mut inst = vec![
            Instruction {
                opcode: OpCode::PUSH0,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::JMP_L,
                operands: 5i32.to_le_bytes().to_vec(),
            },
            Instruction {
                opcode: OpCode::PUSH1,
                operands: vec![],
            },
        ];
        assert!(super::shorten_long_pc_relative_branches(&mut inst));
        assert_eq!(inst[1].opcode, OpCode::JMP);
        assert_eq!(inst[1].encoded_len(), 2);
    }

    #[test]
    fn shorten_jmp_l_skipped_when_rel_out_of_i8() {
        let mut inst = vec![Instruction {
            opcode: OpCode::JMP_L,
            operands: 200i32.to_le_bytes().to_vec(),
        }];
        assert!(!super::shorten_long_pc_relative_branches(&mut inst));
        assert_eq!(inst[0].opcode, OpCode::JMP_L);
    }

    #[test]
    fn peel_short_jmp_when_target_is_next_instruction() {
        let mut inst = vec![
            Instruction {
                opcode: OpCode::PUSH0,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::JMP,
                operands: vec![2u8],
            },
            Instruction {
                opcode: OpCode::PUSH2,
                operands: vec![],
            },
        ];
        let mut patches: Vec<(usize, String)> = vec![];
        assert!(super::peel_redundant_uncond_jmp_to_next(
            &mut inst,
            &mut patches
        ));
        assert_eq!(inst.len(), 2);
        assert_eq!(inst[0].opcode, OpCode::PUSH0);
        assert_eq!(inst[1].opcode, OpCode::PUSH2);
    }

    #[test]
    fn peel_jmp_l_to_next_adjusts_earlier_jump_operands() {
        // Old bytes: PUSH0 @0, JMP_L @1 rel=10 → @11, redundant JMP_L @6 rel=5 → @11, PUSH0 @11.
        let mut inst = vec![
            Instruction {
                opcode: OpCode::PUSH0,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::JMP_L,
                operands: 10i32.to_le_bytes().to_vec(),
            },
            Instruction {
                opcode: OpCode::JMP_L,
                operands: 5i32.to_le_bytes().to_vec(),
            },
            Instruction {
                opcode: OpCode::PUSH0,
                operands: vec![],
            },
        ];
        let mut patches: Vec<(usize, String)> = vec![];
        assert!(super::peel_redundant_uncond_jmp_to_next(
            &mut inst,
            &mut patches
        ));
        assert_eq!(inst.len(), 3);
        let rel = super::read_i32_le(&inst[1].operands).expect("JMP_L operands");
        assert_eq!(rel, 5);
    }

    #[test]
    fn peel_jmp_l_to_next_skipped_when_call_patch_points_at_jump() {
        let mut inst = vec![
            Instruction {
                opcode: OpCode::PUSH0,
                operands: vec![],
            },
            Instruction {
                opcode: OpCode::JMP_L,
                operands: 5i32.to_le_bytes().to_vec(),
            },
            Instruction {
                opcode: OpCode::PUSH1,
                operands: vec![],
            },
        ];
        let mut patches = vec![(1usize, "callee".into())];
        assert!(!super::peel_redundant_uncond_jmp_to_next(
            &mut inst,
            &mut patches
        ));
        assert_eq!(inst.len(), 3);
    }
}
