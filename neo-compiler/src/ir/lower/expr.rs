use crate::ir::*;
use crate::syntax::ast::*;

use super::builder::Builder;
use super::env::Env;
use super::helpers::{err, field_index_of, LowerError};

impl<'a> Builder<'a> {
    pub fn lower_expr(&mut self, expr: &Expr, env: &mut Env) -> Result<ValueRef, LowerError> {
        match expr {
            Expr::Literal(literal) => {
                let out = self.new_value();
                self.emit(out, Instr::Const(literal.clone()));
                Ok(ValueRef::Value(out))
            }
            Expr::Ident(name) => env
                .get(name)
                .ok_or_else(|| err(format!("undefined variable `{name}`"))),
            Expr::Self_ => env.get("self").ok_or_else(|| err("`self` is not in scope")),
            Expr::Unary { op, expr: inner } => {
                let value = self.lower_expr(inner, env)?;
                let out = self.new_value();
                self.emit(out, Instr::Unary { op: *op, value });
                Ok(ValueRef::Value(out))
            }
            Expr::Paren(inner) => self.lower_expr(inner, env),
            Expr::Cast { expr, ty } => {
                if crate::codegen::expr::get_operand_for_type(ty).is_none() {
                    return Err(err(format!(
                        "IR lowering: `as` to `{ty:?}` is not supported yet",
                    )));
                }
                let value = self.lower_expr(expr, env)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::Cast {
                        value,
                        ty: ty.clone(),
                    },
                );
                Ok(ValueRef::Value(out))
            }
            Expr::Binary { op, left, right } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    return self.lower_short_circuit(*op, left, right, env);
                }
                let left = self.lower_expr(left, env)?;
                let right = self.lower_expr(right, env)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::Binary {
                        op: *op,
                        left,
                        right,
                    },
                );
                Ok(ValueRef::Value(out))
            }
            Expr::Member { base, field } => {
                if matches!(base.as_ref(), Expr::Self_) {
                    if let Some(cf) = self.contract_field_by_name(field) {
                        let ty = cf.ty.clone();
                        if ty.is_map() {
                            return Err(err(format!(
                                "use `self.{field}[key]` to read contract map `{field}` entries (whole-map load or map.size() is not supported)",
                            )));
                        }
                        if ty.is_array() {
                            return Err(err("contract cannot have array fields"));
                        }
                        let out = self.new_value();
                        self.emit(
                            out,
                            Instr::ContractStorageGet {
                                field: field.clone(),
                                value_ty: ty,
                            },
                        );
                        return Ok(ValueRef::Value(out));
                    }
                }

                let (base_ref, base_name) = match base.as_ref() {
                    Expr::Ident(name) => (
                        env.get(name)
                            .ok_or_else(|| err(format!("undefined variable `{name}`")))?,
                        name.as_str(),
                    ),
                    Expr::Self_ => (
                        env.get("self")
                            .ok_or_else(|| err("`self` is not in scope"))?,
                        "self",
                    ),
                    _ => return Err(err("IR lowering: member base must be identifier or self")),
                };

                let struct_name = env.get_struct_var(base_name).ok_or_else(|| {
                    err("IR lowering: member access needs a struct-typed variable")
                })?;
                let field_index = field_index_of(self.structs, struct_name, field)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::StructFieldGet {
                        base: base_ref,
                        index: field_index,
                    },
                );
                Ok(ValueRef::Value(out))
            }
            Expr::Index { base, index } => {
                if let Some((map_name, key_ty, val_ty)) =
                    self.contract_self_map_types(base.as_ref())
                {
                    let key = self.lower_expr(index, env)?;
                    let out = self.new_value();
                    self.emit(
                        out,
                        Instr::ContractMapStorageGet {
                            field: map_name,
                            key_ty,
                            val_ty,
                            key,
                        },
                    );
                    return Ok(ValueRef::Value(out));
                }
                let base = self.lower_expr(base, env)?;
                let index = self.lower_expr(index, env)?;
                let out = self.new_value();
                self.emit(out, Instr::IndexGet { base, index });
                Ok(ValueRef::Value(out))
            }
            Expr::Assign { target, op, value } => self.lower_assign(target, *op, value, env),
            Expr::Call { callee, args } => self.lower_call(callee, args, env),
            Expr::StructLit { name, fields } => {
                let struct_decl = self
                    .structs
                    .iter()
                    .find(|s| s.name == *name)
                    .ok_or_else(|| err(format!("unknown struct `{name}` in literal")))?;
                let mut values = Vec::new();
                for struct_field in &struct_decl.fields {
                    let expr = fields
                        .iter()
                        .find(|(n, _)| n == &struct_field.name)
                        .map(|(_, expr)| expr)
                        .or(struct_field.init.as_ref())
                        .ok_or_else(|| {
                            err(format!(
                                "struct literal `{name}` missing field `{}`",
                                struct_field.name
                            ))
                        })?;
                    values.push(self.lower_expr(expr, env)?);
                }
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::StructPack {
                        struct_name: name.clone(),
                        field_values: values,
                    },
                );
                Ok(ValueRef::Value(out))
            }
            Expr::ArrayLit { elements, .. } => {
                let mut values = Vec::new();
                for expr in elements {
                    values.push(self.lower_expr(expr, env)?);
                }
                let out = self.new_value();
                self.emit(out, Instr::ArrayPack { elements: values });
                Ok(ValueRef::Value(out))
            }
            Expr::MapLit { pairs, .. } => {
                let mut values = Vec::new();
                for (k, v) in pairs {
                    values.push((self.lower_expr(k, env)?, self.lower_expr(v, env)?));
                }
                let out = self.new_value();
                self.emit(out, Instr::MapPack { pairs: values });
                Ok(ValueRef::Value(out))
            }
        }
    }
}
