use crate::codegen::CodegenError;
use crate::devpack::{syscall_for_module_method, DevPackModule};
use crate::syntax::ast::Expr;

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_devpack_syscall_call(
        &mut self,
        module: DevPackModule,
        method: &str,
        args: &[Expr],
    ) -> Result<(), CodegenError> {
        let Some(syscall) = syscall_for_module_method(module, method) else {
            return Err(CodegenError::Unsupported(format!(
                "neo-devpack module `{}` is recognized, but `{}` calls are not supported by neo-compiler yet",
                module.as_str(),
                method
            )));
        };
        if args.len() != syscall.args.len() {
            return Err(CodegenError::Unsupported(format!(
                "{}.{method} expects {} argument(s), got {}",
                module.as_str(),
                syscall.args.len(),
                args.len()
            )));
        }
        for arg in args.iter().rev() {
            self.compile_expr(arg)?;
        }
        self.builder.emit_syscall(syscall);
        if syscall.return_type.is_none() {
            self.builder.push_null();
        }
        Ok(())
    }
}
