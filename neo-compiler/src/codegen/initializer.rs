//! Compiler-generated `_initialize` for contract storage fields.
//!
//! Mutable contract fields are initialized during deployment.  This module turns
//! `ContractField` declarations into a synthetic `void _initialize()` routine:
//! - scalar fields get `self.field = <constant>;`
//! - map fields with an explicit map literal get one
//!   `self.map[key] = value;` statement per entry
//! - map fields without an initializer are already empty in contract storage and
//!   therefore do not emit any write

use std::collections::HashMap;

use crate::codegen::CodegenError;
use crate::diagnostic::Span;
use crate::syntax::ast::*;

pub(crate) fn synthesize_initializer(
    contract: &ContractDecl,
) -> Result<Option<FunctionDecl>, CodegenError> {
    if let Some(func) = contract.members.iter().find_map(|member| match member {
        ContractMember::Function(func) if func.name == "_initialize" => Some(func),
        _ => None,
    }) {
        return Err(CodegenError::unsupported(format!(
            "contract method `_initialize` is reserved and is generated automatically (found user-defined `{}` with {} parameter(s))",
            func.name,
            func.params.len()
        )));
    }

    let const_exprs: HashMap<String, Expr> = contract
        .members
        .iter()
        .filter_map(|member| match member {
            ContractMember::ConstProp(prop) => Some((prop.name.clone(), prop.init.clone())),
            _ => None,
        })
        .collect();

    let mut stmts = Vec::new();
    for member in &contract.members {
        let ContractMember::Field(field) = member else {
            continue;
        };

        match &field.ty {
            Type::Map { key, value } => {
                let Some(init) = &field.init else {
                    continue;
                };

                let span = field.init_span.unwrap_or_default();
                let resolved =
                    resolve_const_expr(init, &const_exprs, &mut Vec::new()).map_err(|message| {
                        CodegenError::unsupported_at(
                            span,
                            format!("contract map field `{}` initializer {message}", field.name),
                        )
                    })?;
                let pairs = map_lit_pairs(resolved).map_err(|message| {
                    CodegenError::unsupported_at(
                        span,
                        format!(
                            "contract map field `{}` initializer must be a map literal: {message}",
                            field.name
                        ),
                    )
                })?;

                for (key_expr, value_expr) in pairs {
                    let key = coerce_const_value(key_expr, key, span)?;
                    let value = coerce_const_value(value_expr, value, span)?;
                    stmts.push(map_assign_stmt(&field.name, key, value));
                }
            }
            field_ty => {
                let value_expr = match &field.init {
                    Some(init) => {
                        let span = field.init_span.unwrap_or_default();
                        let resolved = resolve_const_expr(init, &const_exprs, &mut Vec::new())
                            .map_err(|message| {
                                CodegenError::unsupported_at(
                                    span,
                                    format!(
                                        "contract field `{}` initializer {message}",
                                        field.name
                                    ),
                                )
                            })?;
                        coerce_const_value(resolved, field_ty, span).map_err(|message| {
                            CodegenError::unsupported_at(
                                span,
                                format!("contract field `{}`: {message}", field.name),
                            )
                        })?
                    }
                    None => {
                        let span = field.init_span.unwrap_or_default();
                        coerce_const_value(default_value_for_type(field_ty, span)?, field_ty, span)?
                    }
                };
                stmts.push(scalar_assign_stmt(&field.name, value_expr));
            }
        }
    }

    if stmts.is_empty() {
        return Ok(None);
    }

    Ok(Some(FunctionDecl {
        attributes: Vec::new(),
        return_ty: Type::Void,
        name: "_initialize".to_string(),
        params: Vec::new(),
        body: Block::new(stmts),
    }))
}

fn scalar_assign_stmt(field: &str, value: Expr) -> Stmt {
    Stmt::Expr(Expr::Assign {
        target: Box::new(Expr::Member {
            base: Box::new(Expr::Self_),
            field: field.to_string(),
        }),
        op: AssignOp::Assign,
        value: Box::new(value),
    })
}

fn map_assign_stmt(field: &str, key: Expr, value: Expr) -> Stmt {
    Stmt::Expr(Expr::Assign {
        target: Box::new(Expr::Index {
            base: Box::new(Expr::Member {
                base: Box::new(Expr::Self_),
                field: field.to_string(),
            }),
            index: Box::new(key),
        }),
        op: AssignOp::Assign,
        value: Box::new(value),
    })
}

fn default_value_for_type(ty: &Type, span: Span) -> Result<Expr, CodegenError> {
    ensure_initializable_scalar_type(ty, span)?;
    Ok(match ty {
        Type::Bool => Expr::Literal(Literal::Bool(false)),
        Type::Int => Expr::Literal(Literal::Int("0".to_string())),
        Type::String => Expr::Literal(Literal::String(String::new())),
        Type::Hash160 | Type::Hash256 => Expr::Literal(Literal::String(String::new())),
        Type::Buffer => Expr::Literal(Literal::Buffer(String::new())),
        _ => unreachable!("validated scalar type"),
    })
}

fn coerce_const_value(expr: Expr, ty: &Type, span: Span) -> Result<Expr, CodegenError> {
    ensure_initializable_scalar_type(ty, span)?;
    Ok(Expr::Cast {
        expr: Box::new(expr),
        ty: ty.clone(),
    })
}

fn ensure_initializable_scalar_type(ty: &Type, span: Span) -> Result<(), CodegenError> {
    match ty {
        Type::Bool | Type::Int | Type::String | Type::Hash160 | Type::Hash256 | Type::Buffer => {
            Ok(())
        }
        Type::Array(_) => Err(CodegenError::unsupported_at(
            span,
            "contract cannot have array fields".to_string(),
        )),
        Type::Map { .. } => Err(CodegenError::unsupported_at(
            span,
            "contract map field initializer must be a map literal".to_string(),
        )),
        Type::Void | Type::Any | Type::Named(_) => Err(CodegenError::unsupported_at(
            span,
            format!("contract mutable field type `{ty}` cannot be initialized automatically"),
        )),
    }
}

fn map_lit_pairs(expr: Expr) -> Result<Vec<(Expr, Expr)>, String> {
    match expr {
        Expr::MapLit { pairs, .. } => Ok(pairs),
        Expr::Paren(inner) => map_lit_pairs(*inner),
        other => Err(format!("expected map literal, got `{other:?}`")),
    }
}

fn resolve_const_expr(
    expr: &Expr,
    const_exprs: &HashMap<String, Expr>,
    stack: &mut Vec<String>,
) -> Result<Expr, String> {
    match expr {
        Expr::Literal(_) => Ok(expr.clone()),
        Expr::Ident(name) => {
            let Some(init) = const_exprs.get(name) else {
                return Err(format!("references unknown constant `{name}`"));
            };
            if stack.iter().any(|item| item == name) {
                let mut cycle = stack.clone();
                cycle.push(name.clone());
                return Err(format!("constant reference cycle: {}", cycle.join(" -> ")));
            }
            stack.push(name.clone());
            let resolved = resolve_const_expr(init, const_exprs, stack)?;
            stack.pop();
            Ok(resolved)
        }
        Expr::Cast { expr: inner, ty } => Ok(Expr::Cast {
            expr: Box::new(resolve_const_expr(inner, const_exprs, stack)?),
            ty: ty.clone(),
        }),
        Expr::Unary { op, expr: inner } => Ok(Expr::Unary {
            op: *op,
            expr: Box::new(resolve_const_expr(inner, const_exprs, stack)?),
        }),
        Expr::Binary { op, left, right } => Ok(Expr::Binary {
            op: *op,
            left: Box::new(resolve_const_expr(left, const_exprs, stack)?),
            right: Box::new(resolve_const_expr(right, const_exprs, stack)?),
        }),
        Expr::Paren(inner) => Ok(Expr::Paren(Box::new(resolve_const_expr(
            inner,
            const_exprs,
            stack,
        )?))),
        Expr::MapLit { ty, pairs } => Ok(Expr::MapLit {
            ty: ty.clone(),
            pairs: pairs
                .iter()
                .map(|(key, value)| {
                    Ok((
                        resolve_const_expr(key, const_exprs, stack)?,
                        resolve_const_expr(value, const_exprs, stack)?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
        }),
        Expr::ArrayLit { ty, elements } => Ok(Expr::ArrayLit {
            ty: ty.clone(),
            elements: elements
                .iter()
                .map(|element| resolve_const_expr(element, const_exprs, stack))
                .collect::<Result<Vec<_>, String>>()?,
        }),
        Expr::StructLit { name, fields } => Ok(Expr::StructLit {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, value)| {
                    Ok((name.clone(), resolve_const_expr(value, const_exprs, stack)?))
                })
                .collect::<Result<Vec<_>, String>>()?,
        }),
        Expr::Assign { .. }
        | Expr::Self_
        | Expr::Member { .. }
        | Expr::Index { .. }
        | Expr::Call { .. } => Err("is not a constant expression".to_string()),
    }
}
