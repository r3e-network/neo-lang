use crate::codegen::CodegenError;
use crate::syntax::ast::Expr;

use super::ExprGen;

impl ExprGen<'_, '_> {
    /// `recv.method(args...)` → `CALL_L` to lowered `Struct::method`.
    /// Stack before `CALL_L` (bottom → top) matches `codegen` module docs: for `s.add(a, b)` it is `| b | a | s |`
    /// (explicit args pushed **reverse** source order, then receiver on top).
    pub(super) fn compile_struct_call(
        &mut self,
        receiver_variable: &str,
        method: &str,
        args: &[Expr],
    ) -> Result<(), CodegenError> {
        let struct_name = self
            .value_struct
            .get(receiver_variable)
            .cloned()
            .ok_or_else(|| {
                CodegenError::Unsupported(format!(
                    "`{receiver_variable}.method(...)` needs `{receiver_variable}` to be a struct-typed variable"
                ))
            })?;
        let struct_decl = self
            .structs
            .iter()
            .find(|s| s.name == struct_name)
            .ok_or_else(|| CodegenError::Unsupported(format!("unknown struct `{struct_name}`")))?;
        let method_decl = struct_decl.methods.iter().find(|m| m.name == method).ok_or_else(|| {
            CodegenError::Unsupported(format!(
                "struct `{struct_name}` has no method `{method}` (for `{receiver_variable}.{method}(...)`)"
            ))
        })?;
        if args.len() != method_decl.params.len() {
            return Err(CodegenError::Unsupported(format!(
                "`{struct_name}::{method}` expects {} argument(s), got {}",
                method_decl.params.len(),
                args.len()
            )));
        }
        for arg in args.iter().rev() {
            self.compile_expr(arg)?;
        }
        self.compile_expr(&Expr::Ident(receiver_variable.into()))?;
        let index = self.builder.emit_call_l_placeholder();
        let target = format!("{struct_name}.{method}");
        self.pending_call_l.push((index, target));
        Ok(())
    }
}
