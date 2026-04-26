use crate::ir::{Terminator, ValueRef};
use crate::syntax::ast::{Block, Expr, Stmt, StructDecl};

use std::collections::HashSet;

#[derive(Debug, thiserror::Error)]
pub enum LowerError {
    #[error("lower-error: {0}")]
    Message(String),
}

pub(crate) fn err(s: impl std::fmt::Display) -> LowerError {
    LowerError::Message(s.to_string())
}

pub(crate) fn collect_assigned_vars_in_block(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Var { name, .. } => {
                out.insert(name.clone());
            }
            Stmt::Expr(expr) => collect_assigned_vars_in_expr(expr, out),
            Stmt::If {
                then_block,
                else_block,
                ..
            } => {
                collect_assigned_vars_in_block(then_block, out);
                if let Some(else_block) = else_block {
                    collect_assigned_vars_in_block(else_block, out);
                }
            }
            Stmt::While { body, .. } => collect_assigned_vars_in_block(body, out),
            Stmt::Block(block) => collect_assigned_vars_in_block(block, out),
            _ => {}
        }
    }
}

pub(crate) fn collect_assigned_vars_in_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Assign { target, value, .. } => {
            if let Expr::Ident(name) = target.as_ref() {
                out.insert(name.clone());
            }
            collect_assigned_vars_in_expr(value, out);
        }
        Expr::Binary { left, right, .. } => {
            collect_assigned_vars_in_expr(left, out);
            collect_assigned_vars_in_expr(right, out);
        }
        Expr::Unary { expr, .. } => collect_assigned_vars_in_expr(expr, out),
        Expr::Call { callee, args } => {
            collect_assigned_vars_in_expr(callee, out);
            for arg in args {
                collect_assigned_vars_in_expr(arg, out);
            }
        }
        Expr::Member { base, .. } => collect_assigned_vars_in_expr(base, out),
        Expr::Index { base, index } => {
            collect_assigned_vars_in_expr(base, out);
            collect_assigned_vars_in_expr(index, out);
        }
        Expr::Cast { expr, .. } => collect_assigned_vars_in_expr(expr, out),
        Expr::Paren(expr) => collect_assigned_vars_in_expr(expr, out),
        Expr::StructLit { fields, .. } => {
            for (_, expr) in fields {
                collect_assigned_vars_in_expr(expr, out);
            }
        }
        Expr::MapLit { pairs, .. } => {
            for (k, v) in pairs {
                collect_assigned_vars_in_expr(k, out);
                collect_assigned_vars_in_expr(v, out);
            }
        }
        Expr::ArrayLit { elements, .. } => {
            for expr in elements {
                collect_assigned_vars_in_expr(expr, out);
            }
        }
        Expr::Literal(_) | Expr::Ident(_) | Expr::Self_ => {}
    }
}

pub(crate) trait JumpArgsExt {
    fn into_jump_args(self) -> Option<Vec<ValueRef>>;
}

impl JumpArgsExt for Terminator {
    fn into_jump_args(self) -> Option<Vec<ValueRef>> {
        match self {
            Terminator::Jump { args, .. } => Some(args),
            _ => None,
        }
    }
}

pub(crate) fn field_index_of(
    structs: &[StructDecl],
    struct_name: &str,
    field: &str,
) -> Result<usize, LowerError> {
    let struct_decl = structs
        .iter()
        .find(|struct_decl| struct_decl.name == struct_name)
        .ok_or_else(|| err(format!("unknown struct `{struct_name}` for member access")))?;
    struct_decl
        .fields
        .iter()
        .position(|field_decl| field_decl.name == field)
        .ok_or_else(|| err(format!("struct `{struct_name}` has no field `{field}`")))
}
