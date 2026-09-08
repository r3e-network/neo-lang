use crate::syntax::ast::{Block, Expr, FunctionDecl, Stmt, Type};

use super::context::{FnEnv, FnType, TypeCheckContext};
use super::error::{err_at, TypeError};

impl<'a> TypeCheckContext<'a> {
    pub(super) fn check_function(
        &self,
        func: &FunctionDecl,
        fn_type: FnType,
        span: crate::diagnostic::Span,
    ) -> Result<(), TypeError> {
        let is_contract_fn = matches!(fn_type, FnType::ContractMethod { .. });
        let mut env = FnEnv::new(is_contract_fn);
        env.current_span = span;
        match &fn_type {
            FnType::Package => {}
            FnType::StructMethod { struct_name } => {
                env.declare("self", Type::Named(struct_name.clone()))?;
                env.value_struct.insert("self".into(), struct_name.clone());
            }
            FnType::ContractMethod { contract_name } => {
                env.declare("self", Type::Named(contract_name.clone()))?;
            }
        }

        for p in &func.params {
            env.declare(&p.name, p.ty.clone())?;
            if let Type::Named(sn) = &p.ty {
                env.value_struct.insert(p.name.clone(), sn.clone());
            }
        }

        self.check_block(&mut env, &func.body, &func.return_ty)?;

        Ok(())
    }

    pub(super) fn check_block(
        &self,
        env: &mut FnEnv<'a>,
        block: &'a Block,
        return_ty: &Type,
    ) -> Result<(), TypeError> {
        let previous_expr_spans = env.expr_spans;
        env.expr_spans = Some(&block.expr_spans);
        env.push_scope();
        for (index, stmt) in block.stmts.iter().enumerate() {
            env.current_span = block.stmt_spans.get(index).copied().unwrap_or_default();
            self.check_stmt(env, stmt, return_ty)?;
        }
        env.pop_scope();
        env.expr_spans = previous_expr_spans;
        Ok(())
    }

    pub(super) fn check_stmt(
        &self,
        env: &mut FnEnv<'a>,
        stmt: &'a Stmt,
        return_ty: &Type,
    ) -> Result<(), TypeError> {
        match stmt {
            Stmt::Var { name, init } => {
                let ty = if let Some(expr) = init {
                    let ty = self.infer_expr(env, expr)?;
                    if let Expr::StructLit {
                        name: struct_name, ..
                    } = expr
                    {
                        env.value_struct.insert(name.clone(), struct_name.clone());
                    } else if let Type::Named(struct_name) = &ty {
                        if self.structs.contains_key(struct_name) {
                            env.value_struct.insert(name.clone(), struct_name.clone());
                        }
                    }
                    ty
                } else {
                    Type::Any
                };
                env.declare(name, ty)?;
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.infer_expr(env, expr)?;
                Ok(())
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                let ty = self.infer_expr(env, cond)?;
                if ty != Type::Bool {
                    return Err(err_at(
                        env.current_span,
                        format!("`if` condition must be bool, got `{ty}`"),
                    ));
                }
                self.check_block(env, then_block, return_ty)?;
                if let Some(else_block) = else_block {
                    self.check_block(env, else_block, return_ty)?;
                }
                Ok(())
            }
            Stmt::While { cond, body } => {
                let ty = self.infer_expr(env, cond)?;
                if ty != Type::Bool {
                    return Err(err_at(
                        env.current_span,
                        format!("`while` condition must be bool, got `{ty}`"),
                    ));
                }
                self.check_block(env, body, return_ty)?;
                Ok(())
            }
            Stmt::ForArray { item, iter, body } => {
                let iter_ty = self.infer_expr(env, iter)?;
                let elem_ty = match iter_ty {
                    Type::Array(ty) => *ty,
                    _ => {
                        return Err(err_at(
                            env.current_span,
                            format!("for-in-array expects an array, got `{iter_ty}`"),
                        ));
                    }
                };
                env.push_scope();
                env.declare(item, elem_ty)?;
                self.check_block(env, body, return_ty)?;
                env.pop_scope();
                Ok(())
            }
            Stmt::ForMap {
                key,
                value,
                map,
                body,
            } => {
                let map_ty = self.infer_expr(env, map)?;
                let (key_ty, value_ty) = match map_ty {
                    Type::Map { key, value } => (*key, *value),
                    _ => {
                        return Err(err_at(
                            env.current_span,
                            format!("for-in-map expects a map, got `{map_ty}`"),
                        ));
                    }
                };
                env.push_scope();
                env.declare(key, key_ty)?;
                env.declare(value, value_ty)?;
                self.check_block(env, body, return_ty)?;
                env.pop_scope();
                Ok(())
            }
            Stmt::Return(opt) => match opt {
                None => {
                    if !matches!(return_ty, Type::Void) {
                        Err(err_at(
                            env.current_span,
                            format!("missing return value (expected `{return_ty}`)"),
                        ))
                    } else {
                        Ok(())
                    }
                }
                Some(expr) => {
                    if matches!(return_ty, Type::Void) {
                        return Err(err_at(
                            env.current_span,
                            "void function must not return a value",
                        ));
                    }
                    let ty = self.infer_expr(env, expr)?;
                    if !ty.can_assign_to(return_ty) {
                        Err(err_at(
                            env.current_span,
                            format!("return type mismatch: expected `{return_ty}`, got `{ty}`"),
                        ))
                    } else {
                        Ok(())
                    }
                }
            },
            Stmt::Emit { name, args } => {
                let event_decl = self.events.get(name).ok_or_else(|| {
                    err_at(
                        env.current_span,
                        format!("unknown event `{name}` for `emit`"),
                    )
                })?;
                if args.len() != event_decl.params.len() {
                    return Err(err_at(
                        env.current_span,
                        format!(
                            "event `{name}` expects {} argument(s), got {}",
                            event_decl.params.len(),
                            args.len()
                        ),
                    ));
                }
                for (expr, param) in args.iter().zip(event_decl.params.iter()) {
                    let ty = self.infer_expr(env, expr)?;
                    if !ty.can_assign_to(&param.ty) {
                        return Err(err_at(
                            env.current_span,
                            format!(
                        "`emit {name}` argument `{}` type mismatch: expected `{:?}`, got `{ty:?}`",
                        param.name, param.ty
                    ),
                        ));
                    }
                }
                Ok(())
            }
            Stmt::Block(block) => self.check_block(env, block, return_ty),
        }
    }
}
