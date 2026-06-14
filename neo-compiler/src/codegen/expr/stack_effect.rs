use crate::codegen::context::FnSig;
use crate::syntax::ast::{Expr, Type};
use crate::target::builtin::BuiltinMethod;
use crate::target::natives::native_contract_by_name;
use crate::target::syscall::RuntimeMethod;

use std::collections::HashMap;

pub(crate) struct CallStackEffectCtx<'a> {
    pub package_fns: &'a HashMap<String, FnSig>,
    pub contract_fns: Option<&'a HashMap<String, FnSig>>,
}

/// Whether a [`Stmt::Expr`] leaves a value on the NeoVM stack after lowering.
pub(crate) fn expr_stmt_leaves_stack_value(expr: &Expr, ctx: &CallStackEffectCtx<'_>) -> bool {
    match expr {
        Expr::Call { callee, args } => call_leaves_stack_value(callee, args, ctx),
        _ => true,
    }
}

fn call_leaves_stack_value(
    callee: &Expr,
    args: &[Expr],
    ctx: &CallStackEffectCtx<'_>,
) -> bool {
    match callee {
        Expr::Ident(name) => {
            if let Some(builtin) = BuiltinMethod::resolve(name) {
                return builtin.leaves_stack_value();
            }
            package_fn_leaves_stack_value(name, ctx)
        }
        Expr::Member { base, field } => {
            if let Expr::Ident(pkg) = base.as_ref() {
                if pkg == "runtime" {
                    return runtime_call_leaves_stack_value(field);
                }
                if let Some(contract) = native_contract_by_name(pkg) {
                    return native_call_leaves_stack_value(contract, field, args.len());
                }
            }
            if matches!(base.as_ref(), Expr::Self_) {
                return contract_method_leaves_stack_value(field, ctx);
            }
            if let Expr::Member { base: inner, .. } = base.as_ref() {
                if matches!(inner.as_ref(), Expr::Self_) {
                    return self_map_method_leaves_stack_value(field);
                }
            }
            true
        }
        _ => true,
    }
}

fn fn_return_leaves_stack_value(return_ty: Option<&Type>) -> bool {
    return_ty.is_none_or(|ty| !matches!(ty, Type::Void))
}

fn package_fn_leaves_stack_value(name: &str, ctx: &CallStackEffectCtx<'_>) -> bool {
    fn_return_leaves_stack_value(ctx.package_fns.get(name).map(|sig| &sig.return_ty))
}

fn contract_method_leaves_stack_value(method: &str, ctx: &CallStackEffectCtx<'_>) -> bool {
    fn_return_leaves_stack_value(
        ctx.contract_fns
            .and_then(|methods| methods.get(method))
            .map(|sig| &sig.return_ty),
    )
}

fn self_map_method_leaves_stack_value(method: &str) -> bool {
    match method {
        "remove" => false,
        _ => true,
    }
}

fn native_call_leaves_stack_value(
    contract: &crate::target::natives::NativeContract,
    method: &str,
    arg_count: usize,
) -> bool {
    contract
        .resolve_method(method, arg_count)
        .map(|m| m.leaves_stack_value())
        .unwrap_or(true)
}

fn runtime_call_leaves_stack_value(method: &str) -> bool {
    RuntimeMethod::resolve(method)
        .map(|binding| binding.leaves_stack_value())
        .unwrap_or(true)
}
