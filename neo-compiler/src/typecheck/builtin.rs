use crate::natives::ExternContract;
use crate::syntax::ast::{Expr, Type};
use crate::target::builtin::BuiltinMethod;
use crate::target::syscall::RuntimeMethod;

use super::context::{FnEnv, TypeCheckContext};
use super::error::{err_at, TypeError};

impl<'a> TypeCheckContext<'a> {
    pub(super) fn check_builtin_method_call(
        &self,
        env: &mut FnEnv<'a>,
        receiver: &Expr,
        method: &str,
        args: &[Expr],
    ) -> Result<Option<Type>, TypeError> {
        if let Expr::Member { base, field } = receiver {
            if matches!(base.as_ref(), Expr::Self_) && env.is_contract_fn {
                let Some(cf) = self.contract_fields.iter().find(|f| f.name == *field) else {
                    return Err(err_at(
                        env.current_span,
                        format!("contract doesn't have field `{field}`"),
                    ));
                };
                if let Type::Map { key, .. } = &cf.ty {
                    return self.check_contract_storage_map_method(&key, method, args, env);
                }
            }
        };

        let recv_ty = self.infer_expr(env, receiver)?;
        let span = env.current_span;
        let err_method =
            |msg: &str| -> TypeError { err_at(span, format!("built-in method `{method}`: {msg}")) };
        match (&recv_ty, method) {
            (Type::String | Type::Hash160 | Type::Hash256, "size") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Int))
            }
            (Type::String | Type::Hash160 | Type::Hash256, "sub") => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                let t1 = self.infer_expr(env, &args[1])?;
                if t0 != Type::Int || t1 != Type::Int {
                    return Err(err_method("expects (int, int)"));
                }
                Ok(Some(recv_ty))
            }
            (Type::Buffer, "size") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Int))
            }
            (Type::Buffer, "sub") => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                let t1 = self.infer_expr(env, &args[1])?;
                if t0 != Type::Int || t1 != Type::Int {
                    return Err(err_method("expects (int, int)"));
                }
                Ok(Some(Type::Buffer))
            }
            (Type::Int, "sqrt") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Int))
            }
            (Type::Int, "modmul") | (Type::Int, "modpow") => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                let t1 = self.infer_expr(env, &args[1])?;
                if t0 != Type::Int || t1 != Type::Int {
                    return Err(err_method("expects (int, int)"));
                }
                Ok(Some(Type::Int))
            }
            (Type::Int, "within") => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                let t1 = self.infer_expr(env, &args[1])?;
                if t0 != Type::Int || t1 != Type::Int {
                    return Err(err_method("expects (int, int)"));
                }
                Ok(Some(Type::Bool))
            }
            (Type::Array(_), "size") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Int))
            }
            (Type::Array(elem), "push") => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                if !t0.can_assign_to(elem.as_ref()) {
                    return Err(err_method("type mismatch"));
                }
                Ok(Some(Type::Void))
            }
            (Type::Array(elem), "pop") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some((**elem).clone()))
            }
            (Type::Array(_), "clear") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Void))
            }
            (Type::Map { .. }, "size") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Int))
            }
            (Type::Map { key, .. }, "keys") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Array(key.clone())))
            }
            (Type::Map { value, .. }, "values") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Array(value.clone())))
            }
            (Type::Map { key, .. }, "has") => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                if !t0.can_assign_to(key.as_ref()) {
                    return Err(err_method("type mismatch"));
                }
                Ok(Some(Type::Bool))
            }
            (Type::Map { .. }, "clear") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Void))
            }
            (Type::Map { key, .. }, "remove") => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                if !t0.can_assign_to(key.as_ref()) {
                    return Err(err_method("type mismatch"));
                }
                Ok(Some(Type::Void))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn check_builtin_call(
        &self,
        env: &mut FnEnv<'a>,
        name: &str,
        args: &[Expr],
    ) -> Result<Option<Type>, TypeError> {
        let Some(builtin) = BuiltinMethod::resolve(name) else {
            return Ok(None);
        };
        if args.len() != builtin.source_arg_count() {
            return Err(err_at(
                env.current_span,
                format!(
                    "`{name}` expects {} argument(s), got {}",
                    builtin.source_arg_count(),
                    args.len()
                ),
            ));
        }
        for (index, expr) in args.iter().enumerate() {
            let ty = self.infer_expr(env, expr)?;
            if !builtin.binding().arg_type_matches(index, &ty) {
                return Err(err_at(
                    env.current_span,
                    format!(
                        "`{name}` argument type mismatch: expected `{:?}`, got `{ty}`",
                        builtin.binding().source_arg_type(index)
                    ),
                ));
            }
        }
        Ok(Some(builtin.return_lang_type()))
    }

    pub(super) fn check_extern_contract_call(
        &self,
        contract: &ExternContract,
        method: &str,
        args: &[Expr],
        env: &mut FnEnv<'a>,
    ) -> Result<Type, TypeError> {
        let Some(extern_method) = contract.resolve_method(method, args.len()) else {
            return Err(err_at(
                env.current_span,
                format!(
                    "{}.{method} is not a known external contract method with {} argument(s)",
                    contract.name,
                    args.len()
                ),
            ));
        };
        for (index, arg) in args.iter().enumerate() {
            let ty = self.infer_expr(env, arg)?;
            if !extern_method.arg_type_matches(index, &ty) {
                return Err(err_at(
                    env.current_span,
                    format!(
                        "{}.{method} argument {} type mismatch: expected `{}`, got `{ty}`",
                        contract.name,
                        index + 1,
                        extern_method.params[index].0,
                    ),
                ));
            }
        }
        Ok(extern_method.return_ty.clone())
    }

    pub(super) fn check_runtime_call(
        &self,
        method: &str,
        args: &[Expr],
        env: &mut FnEnv<'a>,
    ) -> Result<Type, TypeError> {
        let Some(binding) = RuntimeMethod::resolve(method) else {
            return Err(err_at(
                env.current_span,
                format!("runtime.{method} is not a known runtime API"),
            ));
        };
        if args.len() != binding.source_arg_count() {
            return Err(err_at(
                env.current_span,
                format!(
                    "runtime.{method} expects {} argument(s), got {}",
                    binding.source_arg_count(),
                    args.len()
                ),
            ));
        }
        for (index, expr) in args.iter().enumerate() {
            let ty = self.infer_expr(env, expr)?;
            let sit = binding.binding().source_arg_type(index);
            if !sit.satisfies_lang_type(&ty) {
                return Err(err_at(
                    env.current_span,
                    format!(
                        "runtime.{method} argument type mismatch: expected `{sit:?}`, got `{ty}`"
                    ),
                ));
            }
        }
        Ok(binding.return_lang_type())
    }
}
