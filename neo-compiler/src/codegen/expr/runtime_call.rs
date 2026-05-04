use crate::codegen::CodegenError;
use crate::syntax::ast::Expr;
use crate::target::syscall::{runtime_syscall_for_method, CallFlags, Syscall};

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_runtime_call(
        &mut self,
        method: &str,
        args: &[Expr],
    ) -> Result<(), CodegenError> {
        // `System.Contract.Call` exposed as `runtime.contractCall` with injected read-only flags.
        // Syscall stack order matches `CALL`: bottom → top is last arg … first arg (see `codegen` module docs).
        if method == "contractCall" && args.len() == 3 {
            self.compile_expr(&args[2])?;
            self.builder.push_int(i64::from(CallFlags::ReadOnly as u8));
            self.compile_expr(&args[1])?;
            self.compile_expr(&args[0])?;
            self.builder.emit_syscall(Syscall::CONTRACT_CALL);
            return Ok(());
        }
        if let Some(syscall) = runtime_syscall_for_method(method) {
            if args.len() != syscall.args.len() {
                return Err(CodegenError::Unsupported(format!(
                    "runtime.{method} expects {} argument(s), got {}",
                    syscall.args.len(),
                    args.len()
                )));
            }
            for arg in args.iter().rev() {
                self.compile_expr(arg)?;
            }
            self.builder.emit_syscall(*syscall);
            return Ok(());
        }
        Err(CodegenError::Unsupported(format!(
            "runtime.{method} is not a known System.Runtime API or wrong arity"
        )))
    }
}
