use crate::codegen::CodegenError;
use crate::syntax::ast::{Expr, Type};
use crate::target::natives::{native_contract_by_name, NativeContract};
use crate::target::nef::MethodToken;

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<(), CodegenError> {
        if let Expr::Member { base, field } = callee {
            if let Expr::Ident(pkg) = base.as_ref() {
                if pkg == "runtime" {
                    return self.compile_runtime_call(field, args);
                }
                if let Some(contract) = native_contract_by_name(pkg) {
                    return self.compile_native_contract_call(contract, field, args);
                }
            }
            if matches!(base.as_ref(), Expr::Self_) && self.contract_name.is_some() {
                return self.compile_contract_call(field, args);
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
                "only `runtime.<method>`, native contracts, struct instance `var.method(...)`, or `self.method(...)` support `x.y(...)` call syntax"
                    .into(),
            ));
        }
        if let Expr::Ident(name) = callee {
            if self.compile_builtin_call(name, args)? {
                return Ok(());
            }
            if let Some(sig) = self.package_fns.get(name) {
                if args.len() != sig.arity {
                    return Err(CodegenError::Unsupported(format!(
                        "call to `{name}` expects {} argument(s), got {}",
                        sig.arity,
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

    fn compile_native_contract_call(
        &mut self,
        contract: &NativeContract,
        method: &str,
        args: &[Expr],
    ) -> Result<(), CodegenError> {
        let native_method = contract.resolve_method(method, args.len()).ok_or_else(|| {
            CodegenError::Unsupported(format!(
                "native call `{}.{method}` with {} argument(s) is not defined",
                contract.name,
                args.len()
            ))
        })?;
        let parameters_count = u16::try_from(args.len()).map_err(|_| {
            CodegenError::Unsupported(format!(
                "native call `{}.{method}` has too many arguments",
                contract.name
            ))
        })?;
        let return_ty = native_method.return_lang_type();
        for arg in args.iter().rev() {
            self.compile_expr(arg)?;
        }
        let token_index = self.method_tokens.intern(MethodToken {
            hash: contract.hash,
            method: method.to_string(),
            parameters_count,
            has_return_value: !matches!(return_ty, Type::Void),
            call_flags: contract.default_call_flags(),
        })?;
        self.builder.emit_callt(token_index);
        Ok(())
    }
}
