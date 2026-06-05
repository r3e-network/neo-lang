use crate::codegen::CodegenError;
use crate::syntax::ast::{AssignOp, Expr, Type};
use crate::target::opcode::{OpCode, ToOpCode};
use crate::target::syscall::Syscall;
use crate::target::StackItemType;

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_contract_member_load(&mut self, field: &str) -> Result<(), CodegenError> {
        let contract_field = self.contract_field_required(field)?;
        let ty = contract_field.ty.clone();
        if ty.is_map() {
            return Err(CodegenError::Unsupported(
                "only has, remove, and index access are supported for contract map fields".into(),
            ));
        }
        if ty.is_array() {
            return Err(CodegenError::Unsupported(
                "contract cannot have array fields".into(),
            ));
        }
        self.builder.push_data(field.as_bytes());
        self.builder.emit_syscall(Syscall::STORAGE_LOCAL_GET);
        self.emit_convert_buffer_on_stack_to_type(&ty)
    }

    pub(super) fn emit_convert_buffer_on_stack_to_type(
        &mut self,
        ty: &Type,
    ) -> Result<(), CodegenError> {
        let op = match ty {
            Type::Bool => StackItemType::Boolean as u8,
            Type::Int => StackItemType::Integer as u8,
            Type::String | Type::Hash160 | Type::Hash256 => StackItemType::ByteString as u8,
            Type::Buffer => StackItemType::Buffer as u8,
            Type::Array(_) | Type::Map { .. } => {
                return Err(CodegenError::Unsupported(format!(
                    "storage load as `{ty:?}` is not implemented yet"
                )));
            }
            Type::Void | Type::Any | Type::Named(_) => {
                return Err(CodegenError::Unsupported(format!(
                    "storage load as `{ty:?}` is not supported"
                )));
            }
        };
        self.builder
            .emit_with_operands(OpCode::CONVERT, std::slice::from_ref(&op));
        Ok(())
    }

    pub(super) fn emit_convert_stack_top_to_storage_buffer(
        &mut self,
        ty: &Type,
    ) -> Result<(), CodegenError> {
        if matches!(ty, Type::Buffer) {
            return Ok(());
        }

        if ty.is_primitive() {
            self.builder.emit_with_operands(
                OpCode::CONVERT,
                std::slice::from_ref(&(StackItemType::Buffer as u8)),
            );
            return Ok(());
        }

        // TODO: use StdLib.Serialize to serialize the compound type to buffer.
        return Err(CodegenError::Unsupported(format!(
            "storage put for type `{ty:?}` is not implemented yet"
        )));
    }

    pub(super) fn emit_map_key_as_bytestring(&mut self, key_ty: &Type) -> Result<(), CodegenError> {
        match key_ty {
            Type::Bool | Type::Int | Type::String | Type::Hash160 | Type::Hash256 => {
                self.builder
                    .emit_with_operands(OpCode::CONVERT, &[StackItemType::ByteString as u8]);
                Ok(())
            }
            Type::Buffer => Ok(()),
            _ => Err(CodegenError::Unsupported(format!(
                "map storage key type `{key_ty:?}` is not supported yet"
            ))),
        }
    }

    /// `Some((field_name, key_ty, val_ty))` when `receiver` is `self.<field>` for a contract `map` field.
    pub(super) fn contract_storage_map_receiver(
        &self,
        receiver: &Expr,
    ) -> Option<(String, Type, Type)> {
        let Expr::Member {
            base: inner,
            field: fname,
        } = receiver
        else {
            return None;
        };
        if !matches!(inner.as_ref(), Expr::Self_) {
            return None;
        }
        let cf = self.contract_fields.iter().find(|cf| cf.name == *fname)?;
        let Type::Map { key, value } = &cf.ty else {
            return None;
        };
        Some((
            cf.name.clone(),
            (*key.as_ref()).clone(),
            (*value.as_ref()).clone(),
        ))
    }

    /// `self.map.has(key)` using composite storage key: `Get` then non-null check.
    pub(super) fn emit_contract_map_has(
        &mut self,
        field_name: &str,
        key_ty: &Type,
        key_expr: &Expr,
    ) -> Result<(), CodegenError> {
        self.emit_contract_map_key_on_stack(field_name, key_ty, key_expr)?;
        self.builder.emit_syscall(Syscall::STORAGE_LOCAL_GET);
        self.builder.emit(OpCode::ISNULL);
        self.builder.emit(OpCode::NOT);
        Ok(())
    }

    /// `self.map.remove(key)` using `System.Storage.Local.Delete` on the composite key.
    pub(super) fn emit_contract_map_delete(
        &mut self,
        field_name: &str,
        key_ty: &Type,
        key_expr: &Expr,
    ) -> Result<(), CodegenError> {
        self.emit_contract_map_key_on_stack(field_name, key_ty, key_expr)?;
        self.builder.emit_syscall(Syscall::STORAGE_LOCAL_DELETE);
        Ok(())
    }

    /// Stack: … → …, composite_key (ByteString), where key = `{field_name}\0` ‖ key_bytes.
    pub(super) fn emit_contract_map_key_on_stack(
        &mut self,
        field_name: &str,
        key_ty: &Type,
        index: &Expr,
    ) -> Result<(), CodegenError> {
        let mut prefix = field_name.as_bytes().to_vec();
        prefix.push(0);
        self.builder.push_data(&prefix);
        self.compile_expr(index)?;
        self.emit_map_key_as_bytestring(key_ty)?;
        self.builder.emit(OpCode::CAT);
        Ok(())
    }

    pub(super) fn emit_contract_map_get(
        &mut self,
        field_name: &str,
        key_ty: &Type,
        value_ty: &Type,
        index: &Expr,
    ) -> Result<(), CodegenError> {
        self.emit_contract_map_key_on_stack(field_name, key_ty, index)?;
        self.builder.emit_syscall(Syscall::STORAGE_LOCAL_GET);
        self.emit_convert_buffer_on_stack_to_type(value_ty)
    }

    pub(super) fn emit_contract_map_assign(
        &mut self,
        field_name: &str,
        key_ty: &Type,
        value_ty: &Type,
        index: &Expr,
        value: &Expr,
    ) -> Result<(), CodegenError> {
        // `Put(key, value)`: stack bottom → top matches plain calls — `| value | key |` (key = first param on top).
        self.compile_expr(value)?;
        self.emit_contract_map_key_on_stack(field_name, key_ty, index)?;
        self.builder.emit(OpCode::SWAP);
        self.emit_convert_stack_top_to_storage_buffer(value_ty)?;
        self.builder.emit(OpCode::SWAP);
        self.builder.emit_syscall(Syscall::STORAGE_LOCAL_PUT);
        Ok(())
    }

    pub(super) fn compile_contract_map_compound_assign(
        &mut self,
        field_name: &str,
        key_ty: &Type,
        value_ty: &Type,
        index: &Expr,
        op: AssignOp,
        value: &Expr,
    ) -> Result<(), CodegenError> {
        let key_slot = self.env.alloc_slot()?;
        self.emit_contract_map_key_on_stack(field_name, key_ty, index)?;
        self.builder.emit(OpCode::DUP);
        self.builder.emit_stloc(key_slot);
        self.builder.emit_syscall(Syscall::STORAGE_LOCAL_GET);
        self.emit_convert_buffer_on_stack_to_type(value_ty)?;
        self.compile_expr(value)?;
        self.builder.emit(op.to_op_code());
        let value_slot = self.env.alloc_slot()?;
        self.builder.emit_stloc(value_slot);
        self.builder.emit_ldloc(key_slot);
        self.builder.emit_ldloc(value_slot);
        self.emit_convert_stack_top_to_storage_buffer(value_ty)?;
        self.builder.emit(OpCode::SWAP);
        self.builder.emit_syscall(Syscall::STORAGE_LOCAL_PUT);
        self.builder.emit_ldloc(value_slot);
        Ok(())
    }
}
