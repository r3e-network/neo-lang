use std::collections::{HashMap, HashSet};

use crate::codegen::CodegenError;
use crate::ir::*;
use crate::syntax::ast::Type;
use crate::target::opcode::OpCode;
use crate::target::Builder;

use super::stackify_plan::StackifyPlan;
use super::{IrSideEffectMux, IrStackifyContext};

pub(crate) struct IrBlocks {
    pub(crate) block_start: HashMap<BlockId, usize>,
    pub(crate) pending_jumps: Vec<(usize, BlockId)>,
    pub(crate) call_patches: Vec<(usize, String)>,
}

impl FunctionIr {
    pub(super) fn emit_ir_blocks(
        &self,
        plan: &StackifyPlan,
        arg_count: u8,
        _return_ty: &Type,
        builder: &mut Builder,
    ) -> Result<IrBlocks, CodegenError> {
        let mut call_patches: Vec<(usize, String)> = Vec::new();
        let mut compound_emit_index: usize = 0;
        let mut block_start: HashMap<BlockId, usize> = HashMap::new();
        let mut pending_jumps: Vec<(usize, BlockId)> = Vec::new();
        let mut skip_blocks: HashSet<BlockId> = HashSet::new();
        for block in self.blocks.values() {
            if let Terminator::Branch {
                then_bb,
                then_args,
                else_bb,
                ..
            } = &block.term
            {
                if self
                    .branch_try_jmpif_then_return_else_relay(*then_bb, *else_bb, then_args)
                    .is_some()
                {
                    skip_blocks.insert(*else_bb);
                }
            }
        }

        for (block_id, block) in &self.blocks {
            if skip_blocks.contains(block_id) {
                continue;
            }
            block_start.insert(*block_id, builder.cursor());

            if *block_id != self.entry {
                for (index, _p) in block.params.iter().enumerate().rev() {
                    let slot = *plan
                        .param_slot
                        .get(&(*block_id, index))
                        .expect("param slot");
                    builder.emit_stloc(slot);
                }
            }

            let mut defs: HashMap<ValueId, Instr> = HashMap::new();
            for (out, instr) in &block.instrs {
                defs.insert(*out, instr.clone());
            }
            let mut emitted_spills: HashSet<ValueId> = HashSet::new();
            let ctx = IrStackifyContext {
                defs: &defs,
                all_defs: &plan.def_instr_vec,
                uses: &plan.uses,
                spill: &plan.spill,
                value_slot: &plan.value_slot,
                param_slot: &plan.param_slot,
                entry_bb: self.entry,
                arg_count,
            };
            let mut mux = IrSideEffectMux {
                compound_pairs: &plan.compound_local_pairs,
                compound_index: &mut compound_emit_index,
                call_patches: &mut call_patches,
            };

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

            for id in plan.cross_block_use.iter().copied() {
                if plan.def_block.get(&id).copied() != Some(*block_id) {
                    continue;
                }
                if !plan.spill.contains(&id) {
                    continue;
                }
                if emitted_spills.contains(&id) {
                    continue;
                }
                let Some(instr) = defs.get(&id) else { continue };
                if instr.has_side_effects() {
                    continue;
                }
                let Some(slot) = plan.value_slot.get(&id).copied() else {
                    continue;
                };
                builder.emit_pure_instr_stackified(&ctx, &mut emitted_spills, *block_id, instr)?;
                builder.emit_stloc(slot);
                emitted_spills.insert(id);
            }

            match &block.term {
                Terminator::Unset => {
                    return Err(CodegenError::Unsupported(
                        "internal: unset basic block terminator (IR not sealed)".into(),
                    ));
                }
                Terminator::Return(value) => {
                    if let Some(value) = value {
                        builder.emit_value_ref_stackified(
                            &ctx,
                            &mut emitted_spills,
                            *block_id,
                            *value,
                        )?;
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
                    self.emit_branch_terminator(
                        builder,
                        &ctx,
                        &mut emitted_spills,
                        *block_id,
                        &mut pending_jumps,
                        cond,
                        *then_bb,
                        then_args,
                        *else_bb,
                        else_args,
                    )?;
                }
            }
        }

        Ok(IrBlocks {
            block_start,
            pending_jumps,
            call_patches,
        })
    }
}
