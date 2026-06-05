use crate::codegen::CodegenError;
use crate::syntax::ast::Expr;

use super::ExprGen;

impl ExprGen<'_, '_> {
    /// `self.method(args...)` within the same contract → `CALL_L` to `Contract::method`.
    /// Arguments only (no implicit `self`); stack order matches package calls.
    pub(super) fn compile_contract_call(
        &mut self,
        method: &str,
        args: &[Expr],
    ) -> Result<(), CodegenError> {
        let contract_name = self.contract_name.ok_or_else(|| {
            CodegenError::Unsupported(
                "`self.method(...)` is only valid inside contract methods".into(),
            )
        })?;
        let fn_table = self.contract_fns.ok_or_else(|| {
            CodegenError::Unsupported(
                "internal: contract method table missing for `self.method(...)`".into(),
            )
        })?;
        let sig = fn_table.get(method).ok_or_else(|| {
            CodegenError::Unsupported(format!("contract has no method `{method}`"))
        })?;
        if args.len() != sig.arity {
            return Err(CodegenError::Unsupported(format!(
                "`self.{method}` expects {} argument(s), got {}",
                sig.arity,
                args.len()
            )));
        }
        for arg in args.iter().rev() {
            self.compile_expr(arg)?;
        }
        let index = self.builder.emit_call_l_placeholder();
        let target = format!("{contract_name}::{method}");
        self.pending_call_l.push((index, target));
        Ok(())
    }
}
