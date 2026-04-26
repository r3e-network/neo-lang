use std::collections::{HashMap, HashSet};

use crate::codegen::expr::parse_int_literal;
use crate::codegen::function::CompliledFunction;
use crate::codegen::CodegenError;
use crate::ir::*;
use crate::syntax::ast::Literal;
use crate::target::opcode::OpCode;
use crate::target::Builder;

use super::{IrSideEffectMux, IrStackifyContext};

fn is_cheap_const_literal(lit: &Literal) -> bool {
    match lit {
        Literal::Null | Literal::Bool(_) => true,
        Literal::Int(s) => {
            let Some(n) = parse_int_literal(s) else {
                return false;
            };
            // Prefer re-emitting small ints (PUSHM1..PUSH16) instead of spilling.
            (i32::MIN as i128..=i32::MAX as i128).contains(&n)
        }
        // ByteString/Buffer constants can be large; spilling may still be cheaper than duplicating bytes.
        Literal::String(v) | Literal::Buffer(v) => v.len() <= 4,
    }
}

impl FunctionIr {
    pub(crate) fn compile_ir(&self, arg_count: u8) -> Result<CompliledFunction, CodegenError> {
        let mut builder = Builder::new();
        let initslot_idx = builder.instruction_count();
        builder.emit_initslot(0, arg_count);

        // Stackify (first pass): only allocate locals for SSA values we must "spill".
        //
        // - Pure SSA values are computed on-demand and kept on stack for their consuming instruction.
        // - Values used multiple times are spilled to a local slot.
        // - Values defined by side-effecting instructions are never recomputed; if used later, spill.
        //
        // This avoids the previous "every SSA value becomes a local" strategy, which produced
        // excessive `DUP/STLOC/LDLOC` and an ever-growing stack.
        let mut uses: HashMap<ValueId, usize> = HashMap::new();
        let mut def_block: HashMap<ValueId, BlockId> = HashMap::new();
        let mut def_instr: HashMap<ValueId, Instr> = HashMap::new();
        for (block_id, block) in &self.blocks {
            for (value_id, instr) in &block.instrs {
                def_block.insert(*value_id, *block_id);
                def_instr.insert(*value_id, instr.clone());
            }
        }

        // Dense table for defs: avoids any Hash/Eq pitfalls and speeds up lookup.
        let mut def_instr_vec: Vec<Option<Instr>> = vec![None; self.value_count];
        for (value_id, instr) in def_instr.iter() {
            if value_id.0 < def_instr_vec.len() {
                def_instr_vec[value_id.0] = Some(instr.clone());
            }
        }

        // Track whether a ValueId is used outside its defining block. Such values must be spilled,
        // because we cannot inline across control-flow without dominance reasoning.
        let mut cross_block_use: HashSet<ValueId> = HashSet::new();
        for (block_id, block) in &self.blocks {
            for (_, instr) in &block.instrs {
                instr.collect_value_uses(&mut uses);
                instr.collect_cross_block_uses(*block_id, &def_block, &mut cross_block_use);
            }
            block.term.collect_value_uses(&mut uses);
            block
                .term
                .collect_cross_block_uses(*block_id, &def_block, &mut cross_block_use);
        }

        let mut spill: HashSet<ValueId> = HashSet::new();
        for block in self.blocks.values() {
            for (value_id, instr) in &block.instrs {
                let use_count = uses.get(value_id).copied().unwrap_or(0);
                if use_count > 1 {
                    if matches!(instr, Instr::Const(lit) if is_cheap_const_literal(lit)) {
                        continue;
                    }
                    spill.insert(*value_id);
                    continue;
                }
                if instr.has_side_effects() && use_count > 0 {
                    spill.insert(*value_id);
                }
            }
        }
        spill.extend(cross_block_use.iter().copied());

        // Cross-block uses also don't need spilling for cheap constants; they can be re-emitted per use.
        for id in cross_block_use.iter().copied() {
            let Some(def) = def_instr_vec.get(id.0).and_then(|x| x.as_ref()) else {
                continue;
            };
            if matches!(def, Instr::Const(lit) if is_cheap_const_literal(lit)) {
                spill.remove(&id);
            }
        }

        // If `v` is only used as both operands of `v op v` (total use count 2), do not spill:
        // codegen emits the value once and `DUP`s for the second operand.
        for block in self.blocks.values() {
            for (_, instr) in &block.instrs {
                if let Instr::Binary { left, right, .. } = instr {
                    if left == right {
                        if let ValueRef::Value(id) = left {
                            if uses.get(&id).copied() == Some(2) && !cross_block_use.contains(&id) {
                                spill.remove(&id);
                            }
                        }
                    }
                }
            }
        }

        // Allocate local slots for spilled ValueIds and for every block parameter (BlockId, param_idx).
        let mut next_local: u8 = 0;
        let mut value_slot: HashMap<ValueId, u8> = HashMap::new();
        for i in 0..self.value_count {
            let id = ValueId(i);
            if spill.contains(&id) {
                value_slot.insert(id, next_local);
                next_local = next_local
                    .checked_add(1)
                    .ok_or(CodegenError::LocalLimitExceeded)?;
                if next_local == u8::MAX {
                    return Err(CodegenError::LocalLimitExceeded);
                }
            }
        }
        // Block-parameter SSA slots (locals). Entry-block parameters are NeoVM *arguments* (`LDARG*`),
        // not extra locals — do not allocate `param_slot` for `ir.entry` (see block prologue + `emit_value_ref`).
        let mut param_slot: HashMap<(BlockId, usize), u8> = HashMap::new();
        for (block_id, block) in &self.blocks {
            if *block_id == self.entry {
                continue;
            }
            for (index, _p) in block.params.iter().enumerate() {
                param_slot.insert((*block_id, index), next_local);
                next_local = next_local
                    .checked_add(1)
                    .ok_or(CodegenError::LocalLimitExceeded)?;
                if next_local == u8::MAX {
                    return Err(CodegenError::LocalLimitExceeded);
                }
            }
        }

        let mut compound_local_pairs: Vec<(u8, u8)> = Vec::new();
        for block in self.blocks.values() {
            for (_, instr) in &block.instrs {
                if matches!(
                    instr,
                    Instr::ContractMapStorageCompound { .. }
                ) {
                    let key_slot = next_local;
                    next_local = next_local
                        .checked_add(1)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    if next_local == u8::MAX {
                        return Err(CodegenError::LocalLimitExceeded);
                    }
                    let value_slot = next_local;
                    next_local = next_local
                        .checked_add(1)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    if next_local == u8::MAX {
                        return Err(CodegenError::LocalLimitExceeded);
                    }
                    compound_local_pairs.push((key_slot, value_slot));
                }
            }
        }

        let mut call_patches: Vec<(usize, String)> = Vec::new();
        let mut compound_emit_index: usize = 0;

        let mut block_start: HashMap<BlockId, usize> = HashMap::new();
        let mut pending_jumps: Vec<(usize, BlockId)> = Vec::new();

        // Emit blocks in deterministic id order.
        for (block_id, block) in &self.blocks {
            block_start.insert(*block_id, builder.cursor());

            // Block entry: phi parameters from predecessors arrive on the stack (last param on top).
            // The function entry block uses NeoVM argument slots instead (`LDARG*`); do not pop here.
            if *block_id != self.entry {
                for (index, _p) in block.params.iter().enumerate().rev() {
                    let slot = *param_slot.get(&(*block_id, index)).expect("param slot");
                    builder.emit_stloc(slot);
                }
            }

            // Per-block definition table for on-demand emission of pure instructions.
            let mut defs: HashMap<ValueId, Instr> = HashMap::new();
            for (out, instr) in &block.instrs {
                defs.insert(*out, instr.clone());
            }
            let mut emitted_spills: HashSet<ValueId> = HashSet::new();
            let ctx = IrStackifyContext {
                defs: &defs,
                all_defs: &def_instr_vec,
                uses: &uses,
                spill: &spill,
                value_slot: &value_slot,
                param_slot: &param_slot,
                entry_bb: self.entry,
                arg_count,
            };
            let mut mux = IrSideEffectMux {
                compound_pairs: &compound_local_pairs,
                compound_index: &mut compound_emit_index,
                call_patches: &mut call_patches,
            };

            // Emit side-effecting instructions in order; pure ones are emitted on-demand.
            for (out, instr) in &block.instrs {
                if !instr.has_side_effects() {
                    continue;
                }
                builder.emit_instr_stackified(
                    &ctx,
                    &mut emitted_spills,
                    *block_id,
                    &mut mux,
                    *out,
                    instr,
                )?;
            }

            // Ensure any value used across blocks is materialized into its spill slot in its defining block.
            for id in cross_block_use.iter().copied() {
                if def_block.get(&id).copied() != Some(*block_id) {
                    continue;
                }
                if !spill.contains(&id) {
                    continue;
                }
                if emitted_spills.contains(&id) {
                    continue;
                }
                // Only support spilling pure defs cross-block in phase 1.
                let Some(instr) = defs.get(&id) else { continue };
                if instr.has_side_effects() {
                    continue;
                }
                let Some(slot) = value_slot.get(&id).copied() else {
                    continue;
                };
                builder.emit_pure_instr_stackified(&ctx, &mut emitted_spills, *block_id, instr)?;
                builder.emit_stloc(slot);
                emitted_spills.insert(id);
            }

            match &block.term {
                Terminator::Return(value) => {
                    if let Some(value) = value {
                        builder.emit_value_ref_stackified(
                            &ctx,
                            &mut emitted_spills,
                            *block_id,
                            *value,
                        )?;
                    } else {
                        builder.push_null();
                    }
                    builder.emit(OpCode::RET);
                }
                Terminator::Jump { target, args } => {
                    builder.emit_jump_args_stackified(
                        &ctx,
                        &mut emitted_spills,
                        *block_id,
                        args,
                    )?;
                    let j = builder.emit_jmp_l_placeholder();
                    pending_jumps.push((j, *target));
                }
                Terminator::Branch {
                    cond,
                    then_bb,
                    then_args,
                    else_bb,
                    else_args,
                } => {
                    builder.emit_value_ref_stackified(
                        &ctx,
                        &mut emitted_spills,
                        *block_id,
                        *cond,
                    )?;
                    let jump_else = builder.emit_jmpifnot_l_placeholder();

                    // then path
                    builder.emit_jump_args_stackified(
                        &ctx,
                        &mut emitted_spills,
                        *block_id,
                        then_args,
                    )?;
                    let jump_then = builder.emit_jmp_l_placeholder();
                    pending_jumps.push((jump_then, *then_bb));

                    // else stub location
                    let else_stub = builder.cursor();
                    builder.patch_jmp_target_at_instruction(jump_else, else_stub);
                    builder.emit_jump_args_stackified(
                        &ctx,
                        &mut emitted_spills,
                        *block_id,
                        else_args,
                    )?;
                    let jump_to_else = builder.emit_jmp_l_placeholder();
                    pending_jumps.push((jump_to_else, *else_bb));
                }
            }
        }

        // Patch all jumps.
        for (instr_index, target_block) in pending_jumps {
            let target_pc = *block_start.get(&target_block).ok_or_else(|| {
                CodegenError::Unsupported("ir-codegen: missing block start".into())
            })?;
            builder.patch_jmp_target_at_instruction(instr_index, target_pc);
        }

        builder.patch_initslot_local_count(initslot_idx, next_local);
        Ok(CompliledFunction {
            instructions: builder.into_instructions(),
            call_patches,
        })
    }
}
