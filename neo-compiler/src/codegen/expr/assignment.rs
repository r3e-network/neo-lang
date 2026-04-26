use crate::codegen::env::Slot;
use crate::codegen::CodegenError;
use crate::syntax::ast::{AssignOp, Expr};
use crate::target::opcode::{OpCode, ToOpCode};
use crate::target::syscall::Syscall;

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_assign(
        &mut self,
        target: &Expr,
        op: AssignOp,
        value: &Expr,
    ) -> Result<(), CodegenError> {
        match op {
            AssignOp::Assign => match target {
                Expr::Index { base, index } => {
                    if let Some((map_name, key_ty, val_ty)) =
                        self.contract_self_map_field_types(base.as_ref())?
                    {
                        return self
                            .emit_contract_map_assign(&map_name, &key_ty, &val_ty, index, value);
                    }
                    self.compile_expr(base)?;
                    self.compile_expr(index)?;
                    self.compile_expr(value)?;
                    // SETITEM pops value, index, object; assignment expr value is the assigned value.
                    self.builder.emit(OpCode::DUP);
                    self.builder.emit(OpCode::SETITEM);
                    Ok(())
                }
                _ => {
                    self.compile_expr(value)?;
                    self.builder.emit(OpCode::DUP);
                    self.store_assign_target(target)
                }
            },
            _ => self.compile_compound_assign(target, op, value),
        }
    }

    /// Compile compound assignment: `+=`, `-=`, `*=`, `/=`, `%=`, `>>=`, `<<=`, `&=`, `|=`, `^=`
    fn compile_compound_assign(
        &mut self,
        target: &Expr,
        op: AssignOp,
        value: &Expr,
    ) -> Result<(), CodegenError> {
        if let Expr::Index { base, index } = target {
            if let Some((map_name, key_ty, val_ty)) =
                self.contract_self_map_field_types(base.as_ref())?
            {
                return self.compile_contract_map_compound_assign(
                    &map_name, &key_ty, &val_ty, index, op, value,
                );
            }
            let slot_base = self.env.alloc_slot()?;
            let slot_index = self.env.alloc_slot()?;
            let slot_value = self.env.alloc_slot()?;
            self.compile_expr(base)?;
            self.builder.emit_stloc(slot_base);
            self.compile_expr(index)?;
            self.builder.emit_stloc(slot_index);
            self.builder.emit_ldloc(slot_base);
            self.builder.emit_ldloc(slot_index);
            self.builder.emit(OpCode::PICKITEM);
            self.compile_expr(value)?;
            self.builder.emit(op.to_op_code());
            self.builder.emit_stloc(slot_value);
            self.builder.emit_ldloc(slot_base);
            self.builder.emit_ldloc(slot_index);
            self.builder.emit_ldloc(slot_value);
            self.builder.emit(OpCode::SETITEM);
            self.builder.emit_ldloc(slot_value);
            return Ok(());
        }
        self.compile_lvalue_read(target)?;
        self.compile_expr(value)?;
        self.builder.emit(op.to_op_code());
        self.builder.emit(OpCode::DUP);
        self.store_assign_target(target)
    }

    fn compile_lvalue_read(&mut self, target: &Expr) -> Result<(), CodegenError> {
        match target {
            Expr::Ident(name) => {
                let slot = self.env.resolve(name)?;
                self.builder.emit_ldslot(slot);
                Ok(())
            }
            Expr::Member { base, field } if matches!(base.as_ref(), Expr::Self_) => {
                if self
                    .contract_fields
                    .is_some_and(|fs| fs.iter().any(|f| f.name == *field))
                {
                    return self.compile_contract_member_load(field);
                }
                let struct_name = self.value_struct.get("self").cloned().ok_or_else(|| {
                    CodegenError::Unsupported("invalid compound-assignment for `self`".into())
                })?;
                let index = self.field_index_of(&struct_name, field)?;
                let slot = self.env.resolve("self")?;
                self.builder.emit_ldslot(slot);
                self.builder.push_int(index as i64);
                self.builder.emit(OpCode::PICKITEM);
                Ok(())
            }
            _ => Err(CodegenError::Unsupported(
                "invalid assignment target".into(),
            )),
        }
    }

    fn store_assign_target(&mut self, target: &Expr) -> Result<(), CodegenError> {
        match target {
            Expr::Member { base, field } if matches!(base.as_ref(), Expr::Self_) => {
                if self
                    .contract_fields
                    .is_some_and(|fs| fs.iter().any(|f| f.name == *field))
                {
                    let contract_field = self.contract_field_required(field)?;
                    let ty = contract_field.ty.clone();
                    if ty.is_map() {
                        return Err(CodegenError::Unsupported(
                            "cannot assign to a contract map field without `[key]`".into(),
                        ));
                    }
                    self.emit_convert_stack_top_to_storage_buffer(&ty)?;
                    self.builder.push_data(field.as_bytes());
                    self.builder.emit_syscall(Syscall::STORAGE_LOCAL_PUT);
                    return Ok(());
                }
                Err(CodegenError::Unsupported(
                    "assigning to `self.field` is only implemented for contract storage fields"
                        .into(),
                ))
            }
            Expr::Ident(name) => {
                let slot = self.env.resolve(name)?;
                match slot {
                    Slot::Arg(index) => self.builder.emit_starg(index),
                    Slot::Local(index) => self.builder.emit_stloc(index),
                }
                Ok(())
            }
            _ => Err(CodegenError::Unsupported(
                "invalid assignment target".into(),
            )),
        }
    }
}
