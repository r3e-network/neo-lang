use crate::natives::native_contract_by_name;
use crate::syntax::ast::Expr;

use super::context::{FnEnv, TypeCheckContext};
use super::error::{err_at, TypeError};
use crate::syntax::ast::Type;

impl<'a> TypeCheckContext<'a> {
    pub(super) fn check_contract_method_call(
        &self,
        env: &mut FnEnv<'a>,
        method: &str,
        args: &[Expr],
    ) -> Result<Type, TypeError> {
        let method_decl = self.contract_fns.get(method).ok_or_else(|| {
            err_at(
                env.current_span,
                format!("contract has no method `{method}`"),
            )
        })?;
        if args.len() != method_decl.params.len() {
            return Err(err_at(
                env.current_span,
                format!(
                    "`self.{method}` expects {} argument(s), got {}",
                    method_decl.params.len(),
                    args.len()
                ),
            ));
        }
        for (expr, param) in args.iter().zip(&method_decl.params) {
            let ty = self.infer_expr(env, expr)?;
            if !ty.can_assign_to(&param.ty) {
                return Err(err_at(
                    env.current_span,
                    format!(
                        "argument `{}` to `self.{method}` type mismatch: expected `{}`, got `{ty}`",
                        param.name, param.ty
                    ),
                ));
            }
        }
        Ok(method_decl.return_ty.clone())
    }

    pub(super) fn check_call(
        &self,
        env: &mut FnEnv<'a>,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<Type, TypeError> {
        if let Expr::Member { base, field } = callee {
            if let Expr::Ident(pkg) = base.as_ref() {
                if pkg == "runtime" {
                    return self.check_runtime_call(field, args, env);
                }
                if let Some(contract) = native_contract_by_name(pkg) {
                    return self.check_extern_contract_call(contract, field, args, env);
                }
            }
            if matches!(base.as_ref(), Expr::Self_) && env.is_contract_fn {
                return self.check_contract_method_call(env, field, args);
            }
            if let Some(ty) = self.check_builtin_method_call(env, base.as_ref(), field, args)? {
                return Ok(ty);
            }
            if let Expr::Ident(recv) = base.as_ref() {
                if let Some(struct_name) = env.value_struct.get(recv).cloned() {
                    let struct_decl = self.structs.get(&struct_name).ok_or_else(|| {
                        err_at(env.current_span, format!("unknown struct `{struct_name}`"))
                    })?;
                    let method = struct_decl
                        .methods
                        .iter()
                        .find(|m| m.name == *field)
                        .ok_or_else(|| {
                            err_at(
                                env.current_span,
                                format!("struct `{struct_name}` has no method `{field}`"),
                            )
                        })?;
                    if args.len() != method.params.len() {
                        return Err(err_at(
                            env.current_span,
                            format!(
                                "`{struct_name}::{field}` expects {} argument(s), got {}",
                                method.params.len(),
                                args.len()
                            ),
                        ));
                    }
                    for (expr, param) in args.iter().zip(&method.params) {
                        let ty = self.infer_expr(env, expr)?;
                        if !ty.can_assign_to(&param.ty) {
                            return Err(err_at(env.current_span, format!(
                            "argument `{}` to `{struct_name}.{field}` type mismatch: expected `{}`, got `{ty}`",
                            param.name, param.ty
                        )));
                        }
                    }
                    return Ok(method.return_ty.clone());
                }
            }
        }

        if let Expr::Ident(name) = callee {
            if let Some(ty) = self.check_builtin_call(env, name, args)? {
                return Ok(ty);
            }
            if let Some(fn_decl) = self.package_fns.get(name) {
                if args.len() != fn_decl.params.len() {
                    return Err(err_at(
                        env.current_span,
                        format!(
                            "call to `{name}` expects {} argument(s), got {}",
                            fn_decl.params.len(),
                            args.len()
                        ),
                    ));
                }
                for (expr, param) in args.iter().zip(&fn_decl.params) {
                    let ty = self.infer_expr(env, expr)?;
                    if !ty.can_assign_to(&param.ty) {
                        return Err(err_at(
                            env.current_span,
                            format!(
                        "argument `{}` to `{name}` type mismatch: expected `{}`, got `{ty}`",
                        param.name, param.ty
                    ),
                        ));
                    }
                }
                return Ok(fn_decl.return_ty.clone());
            }
        }

        Err(err_at(env.current_span,
            "only package-level functions, built-in functions, external contracts, struct methods, and runtime.* calls are supported",
        ))
    }

    pub(super) fn check_contract_storage_map_method(
        &self,
        key: &Type,
        method: &str,
        args: &[Expr],
        env: &mut FnEnv<'a>,
    ) -> Result<Option<Type>, TypeError> {
        let span = env.current_span;
        let err_method =
            |msg: &str| -> TypeError { err_at(span, format!("built-in method `{method}`: {msg}")) };
        match method {
            "has" => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                if !t0.can_assign_to(key) {
                    return Err(err_method("type mismatch"));
                }
                Ok(Some(Type::Bool))
            }
            "remove" => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                if !t0.can_assign_to(key) {
                    return Err(err_method("type mismatch"));
                }
                Ok(Some(Type::Void))
            }
            _ => Err(err_at(
                env.current_span,
                format!("contract storage map does not support `{method}`(only `has`, `remove`, and index access)"),
            )),
        }
    }
}
