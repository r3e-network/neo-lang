use std::collections::HashSet;

use crate::codegen::CodegenError;
use crate::ir::*;
use crate::target::Builder;

use super::IrStackifyContext;

impl FunctionIr {
    /// `lower_if` often ends with `then_bb` = early `Return` and a trivial `else_bb` (no instructions)
    /// that only `Jump`s to `join` with `else_args`. The default branch lowering (`JMPIFNOT` + relay
    /// `JMP` into `else_bb`) creates a bytecode triangle; when `else_bb` is only the false target of
    /// this branch, we emit `JMPIF` straight to `then_bb` and fold the else stub (see `block_emit`).
    pub(super) fn branch_try_jmpif_then_return_else_relay(
        &self,
        then_bb: BlockId,
        else_bb: BlockId,
        then_args: &[ValueRef],
    ) -> Option<BlockId> {
        if !then_args.is_empty() {
            return None;
        }
        if then_bb == else_bb {
            return None;
        }
        let then_b = self.blocks.get(&then_bb)?;
        if !matches!(then_b.term, Terminator::Return(_)) {
            return None;
        }
        let else_b = self.blocks.get(&else_bb)?;
        if !else_b.instrs.is_empty() {
            return None;
        }
        let join = match &else_b.term {
            Terminator::Jump { target, .. } => *target,
            _ => return None,
        };
        if join == else_bb || join == then_bb {
            return None;
        }
        let branch_else_count = self
            .blocks
            .values()
            .filter(|b| {
                matches!(
                    &b.term,
                    Terminator::Branch { else_bb: e, .. } if *e == else_bb
                )
            })
            .count();
        if branch_else_count != 1 {
            return None;
        }
        if self
            .blocks
            .values()
            .any(|b| matches!(&b.term, Terminator::Jump { target, .. } if *target == else_bb))
        {
            return None;
        }
        if self.blocks.values().any(|b| {
            matches!(
                &b.term,
                Terminator::Branch { then_bb: t, .. } if *t == else_bb
            )
        }) {
            return None;
        }
        Some(join)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_branch_terminator(
        &self,
        builder: &mut Builder,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        block_id: BlockId,
        pending_jumps: &mut Vec<(usize, BlockId)>,
        cond: &ValueRef,
        then_bb: BlockId,
        then_args: &[ValueRef],
        else_bb: BlockId,
        else_args: &[ValueRef],
    ) -> Result<(), CodegenError> {
        if let Some(join) =
            self.branch_try_jmpif_then_return_else_relay(then_bb, else_bb, then_args)
        {
            builder.emit_value_ref_stackified(ctx, emitted_spills, block_id, *cond)?;
            let jump_then = builder.emit_jmpif_l_placeholder();
            pending_jumps.push((jump_then, then_bb));
            builder.emit_jump_args_stackified(ctx, emitted_spills, block_id, else_args)?;
            let jump_join = builder.emit_jmp_l_placeholder();
            pending_jumps.push((jump_join, join));
            return Ok(());
        }

        builder.emit_value_ref_stackified(ctx, emitted_spills, block_id, *cond)?;

        let then_empty = then_args.is_empty();
        let else_empty = else_args.is_empty();

        if !then_empty && !else_empty {
            let jump_else = builder.emit_jmpifnot_l_placeholder();
            builder.emit_jump_args_stackified(ctx, emitted_spills, block_id, then_args)?;
            let jump_then = builder.emit_jmp_l_placeholder();
            pending_jumps.push((jump_then, then_bb));
            let else_stub = builder.cursor();
            builder.patch_jmp_target_at_instruction(jump_else, else_stub);
            builder.emit_jump_args_stackified(ctx, emitted_spills, block_id, else_args)?;
            let jump_to_else = builder.emit_jmp_l_placeholder();
            pending_jumps.push((jump_to_else, else_bb));
        } else if !then_empty && else_empty {
            let jump_else = builder.emit_jmpifnot_l_placeholder();
            pending_jumps.push((jump_else, else_bb));
            builder.emit_jump_args_stackified(ctx, emitted_spills, block_id, then_args)?;
            let jump_then = builder.emit_jmp_l_placeholder();
            pending_jumps.push((jump_then, then_bb));
        } else if then_empty && !else_empty {
            let jump_then = builder.emit_jmpif_l_placeholder();
            pending_jumps.push((jump_then, then_bb));
            builder.emit_jump_args_stackified(ctx, emitted_spills, block_id, else_args)?;
            let jump_else = builder.emit_jmp_l_placeholder();
            pending_jumps.push((jump_else, else_bb));
        } else {
            let jump_then = builder.emit_jmpif_l_placeholder();
            pending_jumps.push((jump_then, then_bb));
            let jump_else = builder.emit_jmp_l_placeholder();
            pending_jumps.push((jump_else, else_bb));
        }

        Ok(())
    }
}
