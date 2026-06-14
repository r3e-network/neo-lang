use crate::codegen::function::CompliledFunction;
use crate::codegen::CodegenError;
use crate::ir::FunctionIr;
use crate::syntax::ast::Type;
use crate::target::Builder;

use super::block_emit::IrBlocks;

impl FunctionIr {
    pub(crate) fn compile_ir(
        &self,
        arg_count: u8,
        return_ty: &Type,
        method_tokens: &mut crate::target::method_token::MethodTokenRegistry,
    ) -> Result<CompliledFunction, CodegenError> {
        let mut builder = Builder::new();
        let initslot_idx = builder.instruction_count();
        builder.emit_initslot(0, arg_count);

        let plan = self.build_stackify_plan()?;
        let IrBlocks {
            block_start,
            pending_jumps,
            call_patches,
        } = self.emit_ir_blocks(&plan, arg_count, return_ty, method_tokens, &mut builder)?;

        for (instr_index, target_block) in pending_jumps {
            let target_pc = *block_start.get(&target_block).ok_or_else(|| {
                CodegenError::Unsupported("ir-codegen: missing block start".into())
            })?;
            builder.patch_jmp_target_at_instruction(instr_index, target_pc);
        }

        builder.patch_initslot_local_count(initslot_idx, plan.next_local);
        Ok(CompliledFunction {
            instructions: builder.into_instructions(),
            call_patches,
        })
    }
}
