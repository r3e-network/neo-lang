use std::collections::HashSet;

use crate::codegen::expr::{get_operand_for_type, parse_int_literal, ParsedIntLiteral};
use crate::codegen::CodegenError;
use crate::ir::*;
use crate::syntax::ast::{AssignOp, BinaryOp, Literal, Type, UnaryOp};
use crate::target::opcode::{OpCode, ToOpCode};
use crate::target::syscall::{CallFlags, Syscall};
use crate::target::{Builder, StackItemType};

use super::{IrSideEffectMux, IrStackifyContext};

impl Builder {
    pub(super) fn emit_jump_args_stackified(
        &mut self,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        current_block: BlockId,
        args: &[ValueRef],
    ) -> Result<(), CodegenError> {
        // Push in reverse, so that param0 ends up deepest (caller pushes args in order).
        for arg in args.iter().rev() {
            self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *arg)?;
        }
        Ok(())
    }

    fn emit_convert_buffer_on_stack_to_type(&mut self, ty: &Type) -> Result<(), CodegenError> {
        let op: u8 = match ty {
            Type::Bool => StackItemType::Boolean as u8,
            Type::Int => StackItemType::Integer as u8,
            Type::String | Type::Hash160 | Type::Hash256 => StackItemType::ByteString as u8,
            Type::Buffer => StackItemType::Buffer as u8,
            Type::Array(_) | Type::Map { .. } => {
                return Err(CodegenError::Unsupported(format!(
                    "ir-codegen: storage load as `{ty:?}` is not implemented",
                )));
            }
            Type::Void | Type::Any | Type::Named(_) => {
                return Err(CodegenError::Unsupported(format!(
                    "ir-codegen: storage load as `{ty:?}` is not supported",
                )));
            }
        };
        self.emit_with_operands(OpCode::CONVERT, std::slice::from_ref(&op));
        Ok(())
    }

    fn emit_convert_stack_top_to_storage_buffer(&mut self, ty: &Type) -> Result<(), CodegenError> {
        let op: u8 = match ty {
            Type::Bool | Type::Int => StackItemType::Buffer as u8,
            Type::String | Type::Hash160 | Type::Hash256 | Type::Buffer => {
                StackItemType::Buffer as u8
            }
            _ => {
                return Err(CodegenError::Unsupported(format!(
                    "ir-codegen: storage put for `{ty:?}` is not implemented",
                )));
            }
        };
        self.emit_with_operands(OpCode::CONVERT, std::slice::from_ref(&op));
        Ok(())
    }

    fn emit_map_key_as_bytestring(&mut self, key_ty: &Type) -> Result<(), CodegenError> {
        match key_ty {
            Type::Bool | Type::Int | Type::String | Type::Hash160 | Type::Hash256 => {
                self.emit_with_operands(OpCode::CONVERT, &[StackItemType::ByteString as u8]);
                Ok(())
            }
            Type::Buffer => Ok(()),
            _ => Err(CodegenError::Unsupported(format!(
                "ir-codegen: map storage key type `{key_ty:?}` is not supported",
            ))),
        }
    }

    fn compound_assign_opcode(op: AssignOp) -> Result<OpCode, CodegenError> {
        if matches!(op, AssignOp::Assign) {
            return Err(CodegenError::Unsupported(
                "ir-codegen: compound assign opcode for `=`".into(),
            ));
        }
        Ok(op.to_op_code())
    }

    fn emit_contract_map_composite_key(
        &mut self,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        current_block: BlockId,
        field_name: &str,
        key_ty: &Type,
        key: ValueRef,
    ) -> Result<(), CodegenError> {
        let mut prefix = field_name.as_bytes().to_vec();
        prefix.push(0);
        self.push_data(&prefix);
        self.emit_value_ref_stackified(ctx, emitted_spills, current_block, key)?;
        self.emit_map_key_as_bytestring(key_ty)?;
        self.emit(OpCode::CAT);
        Ok(())
    }

    pub(super) fn emit_value_ref_stackified(
        &mut self,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        current_block: BlockId,
        value_ref: ValueRef,
    ) -> Result<(), CodegenError> {
        match value_ref {
            ValueRef::Value(id) => {
                self.emit_value_id_stackified(ctx, emitted_spills, current_block, id)
            }
            ValueRef::Param(ParamId(id)) => {
                if current_block == ctx.entry_bb {
                    let index = id as u8;
                    if index >= ctx.arg_count {
                        return Err(CodegenError::Unsupported(format!(
                            "ir-codegen: param index {index} out of range for arg_count {}",
                            ctx.arg_count,
                        )));
                    }
                    self.emit_ldarg(index);
                    Ok(())
                } else {
                    let slot = ctx
                        .param_slot
                        .get(&(current_block, id))
                        .copied()
                        .ok_or_else(|| {
                            CodegenError::Unsupported(format!(
                                "ir-codegen: unknown param {id} in block {:?}",
                                current_block
                            ))
                        })?;
                    self.emit_ldloc(slot);
                    Ok(())
                }
            }
        }
    }

    fn emit_value_id_stackified(
        &mut self,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        current_block: BlockId,
        id: ValueId,
    ) -> Result<(), CodegenError> {
        let IrStackifyContext {
            defs,
            all_defs,
            uses: _,
            spill,
            value_slot,
            param_slot: _,
            entry_bb: _,
            arg_count: _,
        } = ctx;
        if spill.contains(&id) {
            // Cross-block uses: require the value to have been stored at its definition site.
            if let Some(slot) = value_slot.get(&id).copied() {
                if emitted_spills.contains(&id) {
                    self.emit_ldloc(slot);
                    return Ok(());
                }
                // If we don't have a local definition, we can only load (it must have been materialized).
                if !defs.contains_key(&id) {
                    self.emit_ldloc(slot);
                    emitted_spills.insert(id);
                    return Ok(());
                }
                // Emit once (in this block), then spill to local for future uses.
                let instr = defs
                    .get(&id)
                    .or_else(|| all_defs.get(id.0).and_then(|x| x.as_ref()))
                    .ok_or_else(|| {
                        CodegenError::Unsupported("ir-codegen: missing spilled value def".into())
                    })?;
                self.emit_pure_instr_stackified(ctx, emitted_spills, current_block, instr)?;
                // Keep a copy for both stack (this use) and local slot (future uses).
                self.emit(OpCode::DUP);
                self.emit_stloc(slot);
                emitted_spills.insert(id);
                return Ok(());
            }
            return Err(CodegenError::Unsupported(
                "ir-codegen: spilled value missing slot".into(),
            ));
        }

        let instr = defs
            .get(&id)
            .or_else(|| all_defs.get(id.0).and_then(|x| x.as_ref()))
            .ok_or_else(|| {
                CodegenError::Unsupported(format!(
                    "ir-codegen: unknown value {:?} in block {:?}",
                    id, current_block
                ))
            })?;
        if instr.has_side_effects() {
            return Err(CodegenError::Unsupported(
                "ir-codegen: side-effect value must be spilled or emitted in order".into(),
            ));
        }
        self.emit_pure_instr_stackified(ctx, emitted_spills, current_block, instr)
    }

    pub(super) fn emit_pure_instr_stackified(
        &mut self,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        current_block: BlockId,
        instr: &Instr,
    ) -> Result<(), CodegenError> {
        match instr {
            Instr::Const(lit) => {
                self.emit_literal(lit)?;
                Ok(())
            }
            Instr::StructFieldGet { base, index } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *base)?;
                self.push_int(*index as i64);
                self.emit(OpCode::PICKITEM);
                Ok(())
            }
            Instr::IndexGet { base, index } => {
                if base == index {
                    // e.g. `a[a]` — one evaluation, duplicate for PICKITEM operands.
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *base)?;
                    self.emit(OpCode::DUP);
                } else {
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *base)?;
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *index)?;
                }
                self.emit(OpCode::PICKITEM);
                Ok(())
            }
            Instr::IndexSet { .. } | Instr::StructFieldSet { .. } => {
                Err(CodegenError::Unsupported(
                    "ir-codegen: IndexSet/StructFieldSet must be emitted in-order".into(),
                ))
            }
            Instr::Copy(value) => match *value {
                // Only `lower_function_to_ir` emits `Copy(Param(i))` for callee formals. That is never a
                // join-slot phi; `LDARG i` is valid on every basic block in the routine.
                ValueRef::Param(ParamId(idx)) => {
                    let index = u8::try_from(idx).map_err(|_| {
                        CodegenError::Unsupported(
                            "internal: formal index overflow for LDARG".into(),
                        )
                    })?;
                    if index >= ctx.arg_count {
                        return Err(CodegenError::Unsupported(
                            "internal: Copy(Param(i)) with i >= arg_count".into(),
                        ));
                    }
                    self.emit_ldarg(index);
                    Ok(())
                }
                other => self.emit_value_ref_stackified(ctx, emitted_spills, current_block, other),
            },
            Instr::Unary { op, value } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                match op {
                    UnaryOp::Positive => {}
                    UnaryOp::Negative => self.emit(OpCode::NEGATE),
                    UnaryOp::Not => self.emit(OpCode::NOT),
                    UnaryOp::BitNot => self.emit(OpCode::INVERT),
                }
                Ok(())
            }
            Instr::Binary { op, left, right } => {
                if left == right {
                    // Common after CSE: `dx * dx`, `x + x`, etc. — avoid recomputing or reloading locals.
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *left)?;
                    self.emit(OpCode::DUP);
                } else {
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *left)?;
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *right)?;
                }
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    return Err(CodegenError::Unsupported(
                        "ir-codegen: logical and/or not supported".into(),
                    ));
                }
                self.emit(op.to_op_code());
                Ok(())
            }
            Instr::ContractStorageGet { field, value_ty } => {
                self.push_data(field.as_bytes());
                self.emit_syscall(Syscall::STORAGE_LOCAL_GET);
                self.emit_convert_buffer_on_stack_to_type(value_ty)?;
                Ok(())
            }
            Instr::ContractMapStorageGet {
                field,
                key_ty,
                val_ty,
                key,
            } => {
                self.emit_contract_map_composite_key(
                    ctx,
                    emitted_spills,
                    current_block,
                    field,
                    key_ty,
                    *key,
                )?;
                self.emit_syscall(Syscall::STORAGE_LOCAL_GET);
                self.emit_convert_buffer_on_stack_to_type(val_ty)?;
                Ok(())
            }
            Instr::ContractMapStorageHas { field, key_ty, key } => {
                self.emit_contract_map_composite_key(
                    ctx,
                    emitted_spills,
                    current_block,
                    field,
                    key_ty,
                    *key,
                )?;
                self.emit_syscall(Syscall::STORAGE_LOCAL_GET);
                self.emit(OpCode::ISNULL);
                self.emit(OpCode::NOT);
                Ok(())
            }
            // Contract storage arrays are not supported.
            Instr::StructPack { field_values, .. } => {
                for vr in field_values {
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *vr)?;
                }
                self.push_int(field_values.len() as i64);
                self.emit(OpCode::PACK);
                Ok(())
            }
            Instr::ArrayPack { elements } => {
                for vr in elements.iter().rev() {
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *vr)?;
                }
                self.push_int(elements.len() as i64);
                self.emit(OpCode::PACK);
                Ok(())
            }
            Instr::MapPack { pairs } => {
                for (k, v) in pairs.iter().rev() {
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *v)?;
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *k)?;
                }
                self.push_int(pairs.len() as i64);
                self.emit(OpCode::PACKMAP);
                Ok(())
            }
            Instr::Size { value } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                self.emit(OpCode::SIZE);
                Ok(())
            }
            Instr::Keys { map } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *map)?;
                self.emit(OpCode::KEYS);
                Ok(())
            }
            Instr::Values { map } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *map)?;
                self.emit(OpCode::VALUES);
                Ok(())
            }
            Instr::HasKey { map, key } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *map)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *key)?;
                self.emit(OpCode::HASKEY);
                Ok(())
            }
            Instr::SubStr {
                value,
                start,
                length,
            } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *start)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *length)?;
                self.emit(OpCode::SUBSTR);
                Ok(())
            }
            Instr::Sqrt { value } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                self.emit(OpCode::SQRT);
                Ok(())
            }
            Instr::ModMul {
                value,
                other,
                modulus,
            } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *other)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *modulus)?;
                self.emit(OpCode::MODMUL);
                Ok(())
            }
            Instr::ModPow {
                value,
                exponent,
                modulus,
            } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *exponent)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *modulus)?;
                self.emit(OpCode::MODPOW);
                Ok(())
            }
            Instr::Within {
                value,
                min_inclusive,
                max_exclusive,
            } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *min_inclusive)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *max_exclusive)?;
                self.emit(OpCode::WITHIN);
                Ok(())
            }
            Instr::Cast { value, ty } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                let op = get_operand_for_type(ty).ok_or_else(|| {
                    CodegenError::Unsupported(format!(
                        "ir-codegen: `as` to `{ty:?}` is not supported yet",
                    ))
                })?;
                self.emit_with_operands(OpCode::CONVERT, std::slice::from_ref(&op));
                Ok(())
            }
            Instr::Min { left, right } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *left)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *right)?;
                self.emit(OpCode::MIN);
                Ok(())
            }
            Instr::Max { left, right } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *left)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *right)?;
                self.emit(OpCode::MAX);
                Ok(())
            }
            _ => Err(CodegenError::Unsupported(
                "ir-codegen: unknown instruction".into(),
            )),
        }
    }

    pub(super) fn emit_instr_stackified(
        &mut self,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        current_block: BlockId,
        mux: &mut IrSideEffectMux<'_>,
        out: ValueId,
        instr: &Instr,
    ) -> Result<(), CodegenError> {
        let IrStackifyContext {
            uses,
            spill,
            value_slot,
            ..
        } = ctx;
        match instr {
            Instr::IndexSet { base, index, value } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *base)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *index)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;

                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.emit(OpCode::DUP);
                }
                self.emit(OpCode::SETITEM);
                if out_uses > 0 && spill.contains(&out) {
                    // The assignment expression evaluates to the assigned value.
                    let slot = *value_slot
                        .get(&out)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    self.emit(OpCode::DUP);
                    self.emit_stloc(slot);
                    emitted_spills.insert(out);
                }
                Ok(())
            }
            Instr::StructFieldSet { base, index, value } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *base)?;
                self.push_int(*index as i64);
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;

                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.emit(OpCode::DUP);
                }
                self.emit(OpCode::SETITEM);
                if out_uses > 0 && spill.contains(&out) {
                    let slot = *value_slot
                        .get(&out)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    self.emit(OpCode::DUP);
                    self.emit_stloc(slot);
                    emitted_spills.insert(out);
                }
                Ok(())
            }
            Instr::ContractStoragePut {
                field,
                value_ty,
                value,
            } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.emit(OpCode::DUP);
                }
                self.emit_convert_stack_top_to_storage_buffer(value_ty)?;
                self.push_data(field.as_bytes());
                self.emit_syscall(Syscall::STORAGE_LOCAL_PUT);
                if out_uses > 0 && spill.contains(&out) {
                    let slot = *value_slot
                        .get(&out)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    self.emit_stloc(slot);
                    emitted_spills.insert(out);
                }
                Ok(())
            }
            Instr::ContractMapStoragePut {
                field,
                key_ty,
                val_ty,
                key,
                value,
            } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                self.emit_contract_map_composite_key(
                    ctx,
                    emitted_spills,
                    current_block,
                    field,
                    key_ty,
                    *key,
                )?;
                self.emit(OpCode::SWAP);
                self.emit_convert_stack_top_to_storage_buffer(val_ty)?;
                self.emit(OpCode::SWAP);
                self.emit_syscall(Syscall::STORAGE_LOCAL_PUT);
                Ok(())
            }
            Instr::ContractMapStorageCompound {
                field,
                key_ty,
                val_ty,
                key,
                value,
                op,
            } => {
                let pair = mux.compound_pairs.get(*mux.compound_index).ok_or_else(|| {
                    CodegenError::Unsupported("ir-codegen: compound scratch slots".into())
                })?;
                *mux.compound_index += 1;
                let key_slot = pair.0;
                let val_slot = pair.1;

                self.emit_contract_map_composite_key(
                    ctx,
                    emitted_spills,
                    current_block,
                    field,
                    key_ty,
                    *key,
                )?;
                self.emit(OpCode::DUP);
                self.emit_stloc(key_slot);
                self.emit_syscall(Syscall::STORAGE_LOCAL_GET);
                self.emit_convert_buffer_on_stack_to_type(val_ty)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                self.emit(Self::compound_assign_opcode(*op)?);
                self.emit_stloc(val_slot);
                self.emit_ldloc(key_slot);
                self.emit_ldloc(val_slot);
                self.emit_convert_stack_top_to_storage_buffer(val_ty)?;
                self.emit(OpCode::SWAP);
                self.emit_syscall(Syscall::STORAGE_LOCAL_PUT);

                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.emit_ldloc(val_slot);
                    if spill.contains(&out) {
                        let slot = *value_slot
                            .get(&out)
                            .ok_or(CodegenError::LocalLimitExceeded)?;
                        self.emit_stloc(slot);
                        emitted_spills.insert(out);
                    }
                }
                Ok(())
            }
            Instr::Abort { message } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *message)?;
                self.emit(OpCode::ABORTMSG);
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.push_null();
                    if spill.contains(&out) {
                        let slot = *value_slot
                            .get(&out)
                            .ok_or(CodegenError::LocalLimitExceeded)?;
                        self.emit_stloc(slot);
                        emitted_spills.insert(out);
                    }
                }
                Ok(())
            }
            Instr::Assert { cond, message } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *cond)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *message)?;
                self.emit(OpCode::ASSERTMSG);
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.push_null();
                    if spill.contains(&out) {
                        let slot = *value_slot
                            .get(&out)
                            .ok_or(CodegenError::LocalLimitExceeded)?;
                        self.emit_stloc(slot);
                        emitted_spills.insert(out);
                    }
                }
                Ok(())
            }
            Instr::Emit { name, args } => {
                for arg in args {
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *arg)?;
                }
                self.push_int(
                    args.len().try_into().map_err(|_| {
                        CodegenError::Unsupported("ir-codegen: emit arg count".into())
                    })?,
                );
                self.emit(OpCode::PACK);
                self.push_data(name.as_bytes());
                self.emit_syscall(Syscall::RUNTIME_NOTIFY);
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.push_null();
                    if spill.contains(&out) {
                        let slot = *value_slot
                            .get(&out)
                            .ok_or(CodegenError::LocalLimitExceeded)?;
                        self.emit_stloc(slot);
                        emitted_spills.insert(out);
                    }
                }
                Ok(())
            }
            Instr::PackageCall { name, args } => {
                for arg in args.iter().rev() {
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *arg)?;
                }
                let index = self.emit_call_l_placeholder();
                mux.call_patches.push((index, name.clone()));
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses == 0 {
                    self.emit(OpCode::DROP);
                } else if spill.contains(&out) {
                    let slot = *value_slot
                        .get(&out)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    self.emit_stloc(slot);
                    emitted_spills.insert(out);
                }
                Ok(())
            }
            Instr::StructCall {
                struct_name,
                method,
                recv,
                args,
            } => {
                for arg in args.iter().rev() {
                    self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *arg)?;
                }
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *recv)?;
                let index = self.emit_call_l_placeholder();
                mux.call_patches
                    .push((index, format!("{struct_name}::{method}")));
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses == 0 {
                    self.emit(OpCode::DROP);
                } else if spill.contains(&out) {
                    let slot = *value_slot
                        .get(&out)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    self.emit_stloc(slot);
                    emitted_spills.insert(out);
                }
                Ok(())
            }
            Instr::RuntimeLog { message } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *message)?;
                self.emit_syscall(Syscall::RUNTIME_LOG);
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.push_null();
                    if spill.contains(&out) {
                        let slot = *value_slot
                            .get(&out)
                            .ok_or(CodegenError::LocalLimitExceeded)?;
                        self.emit_stloc(slot);
                        emitted_spills.insert(out);
                    }
                }
                Ok(())
            }
            Instr::RuntimeNotify { event_name, state } => {
                // Syscall expects: eventName, state (Array)
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *state)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *event_name)?;
                self.emit_syscall(Syscall::RUNTIME_NOTIFY);
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.push_null();
                    if spill.contains(&out) {
                        let slot = *value_slot
                            .get(&out)
                            .ok_or(CodegenError::LocalLimitExceeded)?;
                        self.emit_stloc(slot);
                        emitted_spills.insert(out);
                    }
                }
                Ok(())
            }
            Instr::ContractCallReadOnly {
                contract,
                method,
                params,
            } => {
                // Stack order matches `codegen/expr.rs`:
                // params, flags, method, contract, then syscall.
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *params)?;
                self.push_int(i64::from(CallFlags::ReadOnly as u8));
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *method)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *contract)?;
                self.emit_syscall(Syscall::CONTRACT_CALL);
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses == 0 {
                    self.emit(OpCode::DROP);
                } else if spill.contains(&out) {
                    let slot = *value_slot
                        .get(&out)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    self.emit_stloc(slot);
                    emitted_spills.insert(out);
                }
                Ok(())
            }
            Instr::ArrayAppend { array, value } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *array)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *value)?;
                self.emit(OpCode::APPEND);
                // APPEND returns the array; keep/drop based on uses.
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses == 0 {
                    self.emit(OpCode::DROP);
                } else if spill.contains(&out) {
                    let slot = *value_slot
                        .get(&out)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    self.emit_stloc(slot);
                    emitted_spills.insert(out);
                }
                Ok(())
            }
            Instr::ArrayPop { array } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *array)?;
                self.emit(OpCode::POPITEM);
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses == 0 {
                    self.emit(OpCode::DROP);
                } else if spill.contains(&out) {
                    let slot = *value_slot
                        .get(&out)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    self.emit_stloc(slot);
                    emitted_spills.insert(out);
                }
                Ok(())
            }
            Instr::ClearItems { collection } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *collection)?;
                self.emit(OpCode::CLEARITEMS);
                // CLEARITEMS leaves collection on stack.
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses == 0 {
                    self.emit(OpCode::DROP);
                } else if spill.contains(&out) {
                    let slot = *value_slot
                        .get(&out)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    self.emit_stloc(slot);
                    emitted_spills.insert(out);
                }
                Ok(())
            }
            Instr::ContractMapStorageRemove { field, key_ty, key } => {
                self.emit_contract_map_composite_key(
                    ctx,
                    emitted_spills,
                    current_block,
                    field,
                    key_ty,
                    *key,
                )?;
                self.emit_syscall(Syscall::STORAGE_LOCAL_DELETE);
                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.push_null();
                    if spill.contains(&out) {
                        let slot = *value_slot
                            .get(&out)
                            .ok_or(CodegenError::LocalLimitExceeded)?;
                        self.emit_stloc(slot);
                        emitted_spills.insert(out);
                    }
                }
                Ok(())
            }
            Instr::Remove { map, key } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *map)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, current_block, *key)?;
                self.emit(OpCode::REMOVE);
                Ok(())
            }
            Instr::EvalAst(_) => Err(CodegenError::Unsupported(
                "ir-codegen: EvalAst should not appear in phase 1".into(),
            )),
            _ => Err(CodegenError::Unsupported(
                "ir-codegen: unexpected non-side-effect instruction in ordered emission".into(),
            )),
        }
    }

    fn emit_literal(&mut self, lit: &Literal) -> Result<(), CodegenError> {
        match lit {
            Literal::Null => self.push_null(),
            Literal::Bool(b) => self.push_bool(*b),
            Literal::Int(s) => {
                let n = parse_int_literal(s)
                    .ok_or_else(|| CodegenError::BadIntegerLiteral(s.clone()))?;
                match n {
                    ParsedIntLiteral::I128(n) => {
                        if n < i64::MIN as i128 || n > i64::MAX as i128 {
                            self.push_int128(n);
                        } else {
                            self.push_int(n as i64);
                        }
                    }
                    ParsedIntLiteral::I256(bytes) => self.push_int256(&bytes),
                }
            }
            Literal::String(s) => self.push_data(s.as_bytes()),
            Literal::Buffer(s) => self.push_data(s.as_bytes()),
        }
        Ok(())
    }
}
