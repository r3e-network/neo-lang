use crate::codegen::CodegenError;
use crate::syntax::ast::Expr;

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_call(&mut self, callee: &Expr, args: &[Expr]) -> Result<(), CodegenError> {
        if let Expr::Member { base, field } = callee {
            if let Expr::Ident(pkg) = base.as_ref() {
                if pkg == "runtime" {
                    return self.compile_runtime_call(field, args);
                }
            }
            if self.compile_builtin_method_call(base.as_ref(), field, args)? {
                return Ok(());
            }
            if let Expr::Ident(receiver_variable) = base.as_ref() {
                if self.value_struct.contains_key(receiver_variable) {
                    return self.compile_struct_call(receiver_variable, field, args);
                }
            }
            return Err(CodegenError::Unsupported(
                "only `runtime.<method>` or struct instance `var.method(...)` support `x.y(...)` call syntax"
                    .into(),
            ));
        }
        if let Expr::Ident(name) = callee {
            if self.compile_builtin_call(name, args)? {
                return Ok(());
            }
            if let Some(&expected_arity) = self.package_fn_arity.get(name) {
                if args.len() != expected_arity {
                    return Err(CodegenError::Unsupported(format!(
                        "call to `{name}` expects {expected_arity} argument(s), got {}",
                        args.len()
                    )));
                }
                for arg in args.iter().rev() {
                    self.compile_expr(arg)?;
                }
                let index = self.builder.emit_call_l_placeholder();
                self.pending_call_l.push((index, name.clone()));
                return Ok(());
            }
        }
        Err(CodegenError::Unsupported(
            "only package-level functions, built-in functions, struct methods, and runtime.* calls are supported"
                .into(),
        ))
    }
}

