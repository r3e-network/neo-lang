//! Expression lowering to NeoVM instructions (`ExprGen`).

use std::collections::HashMap;

use crate::codegen::env::*;
use crate::codegen::CodegenError;
use crate::devpack::DevPackImports;
use crate::syntax::ast::*;
use crate::target::opcode::OpCode;
use crate::target::{Builder, StackItemType};
pub(crate) use literal::parse_int_literal;

mod assignment;
mod builtin_call;
mod builtin_function;
mod builtin_method;
mod contract_storage;
mod devpack_call;
mod literal;
mod member;
mod operator;
mod runtime_call;
mod struct_call;

#[cfg(test)]
mod tests;

/// Borrows the pieces of `FunctionCompiler` needed to lower expressions.
pub(crate) struct ExprGen<'a, 'b> {
    pub(crate) builder: &'b mut Builder,
    pub(crate) env: &'b mut VarEnv,
    pub(crate) structs: &'a [StructDecl],

    /// `local_or_param_name` → neo-lang struct type name (for `s.field` → PICKITEM index).
    pub(crate) value_struct: &'b mut HashMap<String, String>,

    /// Mutable contract fields (storage); `None` for package-level functions.
    pub(crate) contract_fields: Option<&'b [ContractField]>,

    /// `(CALL_L instruction_index, callee link symbol)` e.g. `Point::distanceTo`.
    pub(crate) pending_call_l: &'b mut Vec<(usize, String)>,

    /// Top-level `fn name(...)` in the same source file: `name` → parameter count (for `name(...)` → `CALL_L`).
    pub(crate) package_fn_arity: &'b HashMap<String, usize>,

    /// Imported `neo-devpack` module aliases, if any.
    pub(crate) devpack_imports: &'a DevPackImports,
}

impl<'a, 'b> ExprGen<'a, 'b> {
    pub(crate) fn compile_expr(&mut self, expr: &Expr) -> Result<(), CodegenError> {
        match expr {
            Expr::Assign { target, op, value } => self.compile_assign(target, *op, value),
            Expr::Literal(lit) => self.emit_literal(lit),
            Expr::Ident(name) => {
                let slot = self.env.resolve(name)?;
                self.builder.emit_ldslot(slot);
                Ok(())
            }
            Expr::Self_ => match self.env.resolve("self") {
                Ok(slot) => {
                    self.builder.emit_ldslot(slot);
                    Ok(())
                }
                Err(_) => Err(CodegenError::Unsupported(
                    "`self` cannot stand alone; use `self.field` instead`".into(),
                )),
            },
            Expr::Cast { expr: inner, ty } => {
                let op = get_operand_for_type(ty).ok_or_else(|| {
                    CodegenError::Unsupported(format!(
                        "`as` to `{ty:?}` is not supported in codegen yet"
                    ))
                })?;
                self.compile_expr(inner)?;
                self.builder
                    .emit_with_operands(OpCode::CONVERT, std::slice::from_ref(&op));
                Ok(())
            }
            Expr::Binary { op, left, right } => self.compile_binary(*op, left, right),
            Expr::Unary { op, expr: inner } => self.compile_unary(*op, inner),
            Expr::Member { base, field } => self.emit_member_access(base, field),
            Expr::Index { base, index } => self.emit_index_access(base, index),
            Expr::Call { callee, args } => self.compile_call(callee, args),
            Expr::StructLit { name, fields } => self.emit_struct_lit(name, fields),
            Expr::MapLit { pairs, .. } => self.emit_map_lit(pairs),
            Expr::ArrayLit { elements, .. } => self.emit_array_lit(elements),
            Expr::Paren(inner) => self.compile_expr(inner),
        }
    }
}

pub fn get_operand_for_type(ty: &Type) -> Option<u8> {
    Some(match ty {
        Type::Bool => StackItemType::Boolean as u8,
        Type::Int => StackItemType::Integer as u8,
        Type::String | Type::Hash160 | Type::Hash256 => StackItemType::ByteString as u8,
        Type::Buffer => StackItemType::Buffer as u8,
        Type::Array(_) => StackItemType::Array as u8,
        Type::Map { .. } => StackItemType::Map as u8,
        Type::Any | Type::Void | Type::Named(_) => return None,
    })
}
