use crate::codegen::CodegenError;
use crate::syntax::ast::Expr;
use crate::target::builtin::{BuiltinEmitStep, BuiltinMethod};

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_builtin_call(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> Result<bool, CodegenError> {
        let Some(builtin) = BuiltinMethod::resolve(name) else {
            return Ok(false);
        };
        if args.len() != builtin.source_arg_count() {
            return Err(CodegenError::Unsupported(format!(
                "`{name}` expects {} argument(s), got {}",
                builtin.source_arg_count(),
                args.len()
            )));
        }
        for step in builtin.binding().emit_plan {
            match step {
                BuiltinEmitStep::SourceArg(index) => self.compile_expr(&args[*index])?,
                BuiltinEmitStep::Op(opcode) => self.builder.emit(*opcode),
            }
        }
        Ok(true)
    }
}
