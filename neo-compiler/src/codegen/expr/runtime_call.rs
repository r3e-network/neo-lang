use crate::codegen::CodegenError;
use crate::syntax::ast::Expr;
use crate::target::syscall::{RuntimeEmitStep, RuntimeMethod};

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_runtime_call(
        &mut self,
        method: &str,
        args: &[Expr],
    ) -> Result<(), CodegenError> {
        let Some(binding) = RuntimeMethod::resolve(method) else {
            return Err(CodegenError::Unsupported(format!(
                "runtime.{method} is not a known runtime API or wrong arity"
            )));
        };
        if args.len() != binding.source_arg_count() {
            return Err(CodegenError::Unsupported(format!(
                "runtime.{method} expects {} argument(s), got {}",
                binding.source_arg_count(),
                args.len()
            )));
        }
        for step in binding.emit_steps() {
            match step {
                RuntimeEmitStep::SourceArg(index) => self.compile_expr(&args[index])?,
                RuntimeEmitStep::InjectedInt(value) => self.builder.push_int(value),
                RuntimeEmitStep::Syscall(syscall) => self.builder.emit_syscall(syscall),
            }
        }
        Ok(())
    }
}
