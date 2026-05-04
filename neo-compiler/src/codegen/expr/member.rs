use crate::codegen::CodegenError;
use crate::syntax::ast::{Expr, StructDecl, Type};
use crate::target::opcode::OpCode;

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn emit_member_access(
        &mut self,
        base: &Expr,
        field: &str,
    ) -> Result<(), CodegenError> {
        match base {
            Expr::Ident(var) => {
                let struct_name = self
                    .value_struct
                    .get(var)
                    .cloned()
                    .ok_or_else(|| {
                        CodegenError::Unsupported(
                            "member access needs a variable with struct type".into(),
                        )
                    })?;
                let index = self.field_index_of(&struct_name, field)?;
                self.compile_expr(base)?;
                self.builder.push_int(index as i64);
                self.builder.emit(OpCode::PICKITEM);
                Ok(())
            }
            Expr::Self_ => {
                if self
                    .contract_fields
                    .is_some_and(|fs| fs.iter().any(|f| f.name == *field))
                {
                    return self.compile_contract_member_load(field);
                }
                let struct_name = self.value_struct.get("self").cloned().ok_or_else(|| {
                    CodegenError::Unsupported(
                        "`self.member` needs a contract field or a struct method `self` parameter"
                            .into(),
                    )
                })?;
                let index = self.field_index_of(&struct_name, field)?;
                let slot = self.env.resolve("self")?;
                self.builder.emit_ldslot(slot);
                self.builder.push_int(index as i64);
                self.builder.emit(OpCode::PICKITEM);
                Ok(())
            }
            _ => Err(CodegenError::Unsupported(
                "only `variable.field` or `self.field` member access is allowed (no chained `a.b.c` yet)"
                    .into(),
            )),
        }
    }

    pub(super) fn emit_struct_lit(
        &mut self,
        name: &str,
        fields: &[(String, Expr)],
    ) -> Result<(), CodegenError> {
        let struct_decl = self
            .structs
            .iter()
            .find(|s| s.name == *name)
            .ok_or_else(|| {
                CodegenError::Unsupported(format!("unknown struct `{name}` in literal"))
            })?;
        for field in &struct_decl.fields {
            let init = fields
                .iter()
                .find(|(n, _)| n == &field.name)
                .map(|(_, expr)| expr)
                .or(field.init.as_ref());
            match init {
                Some(expr) => self.compile_expr(expr)?,
                None => self.emit_default_for_type(&field.ty)?,
            }
        }
        self.builder.push_int(
            struct_decl
                .fields
                .len()
                .try_into()
                .map_err(|_| CodegenError::Unsupported("struct too many fields".into()))?,
        );
        self.builder.emit(OpCode::PACK);
        Ok(())
    }

    pub(super) fn emit_index_access(
        &mut self,
        base: &Expr,
        index: &Expr,
    ) -> Result<(), CodegenError> {
        if let Some((map_name, key_ty, val_ty)) = self.contract_self_map_field_types(base)? {
            self.emit_contract_map_get(&map_name, &key_ty, &val_ty, index)?;
            return Ok(());
        }
        self.compile_expr(base)?;
        self.compile_expr(index)?;
        self.builder.emit(OpCode::PICKITEM);
        Ok(())
    }

    pub(super) fn emit_map_lit(&mut self, pairs: &[(Expr, Expr)]) -> Result<(), CodegenError> {
        for (k, v) in pairs.iter().rev() {
            self.compile_expr(k)?;
            self.compile_expr(v)?;
        }
        self.builder.push_int(
            pairs
                .len()
                .try_into()
                .map_err(|_| CodegenError::Unsupported("map literal too large".into()))?,
        );
        self.builder.emit(OpCode::PACKMAP);
        Ok(())
    }

    pub(super) fn emit_array_lit(&mut self, items: &[Expr]) -> Result<(), CodegenError> {
        for item in items.iter().rev() {
            self.compile_expr(item)?;
        }
        self.builder.push_int(
            items
                .len()
                .try_into()
                .map_err(|_| CodegenError::Unsupported("array literal too large".into()))?,
        );
        self.builder.emit(OpCode::PACK);
        Ok(())
    }

    pub(super) fn field_index_of(
        &self,
        struct_name: &str,
        field: &str,
    ) -> Result<usize, CodegenError> {
        let StructDecl { fields, .. } = self
            .structs
            .iter()
            .find(|s| s.name == struct_name)
            .ok_or_else(|| {
                CodegenError::Unsupported(format!("unknown struct type `{struct_name}`"))
            })?;
        fields.iter().position(|f| f.name == field).ok_or_else(|| {
            CodegenError::Unsupported(format!("struct `{struct_name}` has no field `{field}`"))
        })
    }

    pub(super) fn contract_field_required(
        &self,
        field: &str,
    ) -> Result<&crate::syntax::ast::ContractField, CodegenError> {
        let fields = self.contract_fields.ok_or_else(|| {
            CodegenError::Unsupported("`self` is only valid on contract storage fields".into())
        })?;
        fields
            .iter()
            .find(|f| f.name == field)
            .ok_or_else(|| CodegenError::Unsupported(format!("unknown contract field `{field}`")))
    }

    pub(super) fn contract_self_map_field_types(
        &self,
        base: &Expr,
    ) -> Result<Option<(String, Type, Type)>, CodegenError> {
        let Expr::Member {
            base: inner,
            field: field_name,
        } = base
        else {
            return Ok(None);
        };
        if !matches!(inner.as_ref(), Expr::Self_) {
            return Ok(None);
        }
        let Some(fields) = self.contract_fields else {
            return Ok(None);
        };
        let Some(contract_field) = fields.iter().find(|f| f.name == *field_name) else {
            return Ok(None);
        };
        match &contract_field.ty {
            Type::Map { key, value } => Ok(Some((
                contract_field.name.clone(),
                (*key.as_ref()).clone(),
                (*value.as_ref()).clone(),
            ))),
            _ => Err(CodegenError::Unsupported(format!(
                "only `map` contract fields support `[`index`]`; field `{field_name}` is not a map"
            ))),
        }
    }
}
