//! Codegen from IR (CFG + block-parameter SSA) to NeoVM instructions.

use std::collections::{HashMap, HashSet};

use crate::codegen::expr::{convert_operand_for_type, parse_int_literal};
use crate::codegen::function::CompliledFunction;
use crate::codegen::CodegenError;
use crate::ir::*;
use crate::syntax::ast::{AssignOp, BinaryOp, Literal, Type, UnaryOp};
use crate::target::opcode::{OpCode, ToOpCode};
use crate::target::syscall::Syscall;
use crate::target::{Builder, StackItemType};

// Keep errors in `CodegenError` for now; a dedicated error type can be added later.

/// Immutable IR + slot maps shared while emitting stackified code for one function.
struct IrStackifyContext<'a> {
    defs: &'a HashMap<ValueId, Instr>,
    all_defs: &'a [Option<Instr>],
    uses: &'a HashMap<ValueId, usize>,
    spill: &'a HashSet<ValueId>,
    value_slot: &'a HashMap<ValueId, u8>,
    param_slot: &'a HashMap<(BlockId, usize), u8>,
    entry_bb: BlockId,
    arg_count: u8,
}

/// Mutable scratch for side-effecting IR emission (compound map ops + `CALL_L` patches).
struct IrSideEffectMux<'a> {
    compound_pairs: &'a [(u8, u8)],
    compound_idx: &'a mut usize,
    call_patches: &'a mut Vec<(usize, String)>,
}

impl FunctionIr {
    pub(crate) fn compile_ir(&self, arg_count: u8) -> Result<CompliledFunction, CodegenError> {
        let mut builder = Builder::new();
        let initslot_idx = builder.instruction_count();
        builder.emit_initslot(0, arg_count);

        // Stackify (first pass): only allocate locals for SSA values we must "spill".
        //
        // - Pure SSA values are computed on-demand and kept on stack for their consuming instruction.
        // - Values used multiple times are spilled to a local slot.
        // - Values defined by side-effecting instructions are never recomputed; if used later, spill.
        //
        // This avoids the previous "every SSA value becomes a local" strategy, which produced
        // excessive `DUP/STLOC/LDLOC` and an ever-growing stack.
        let mut uses: HashMap<ValueId, usize> = HashMap::new();
        let mut def_block: HashMap<ValueId, BlockId> = HashMap::new();
        let mut def_instr: HashMap<ValueId, Instr> = HashMap::new();
        for (bbid, bb) in &self.blocks {
            for (value_id, instr) in &bb.instrs {
                def_block.insert(*value_id, *bbid);
                def_instr.insert(*value_id, instr.clone());
            }
        }
        // Dense table for defs: avoids any Hash/Eq pitfalls and speeds up lookup.
        let mut def_instr_vec: Vec<Option<Instr>> = vec![None; self.value_count];
        for (value_id, instr) in def_instr.iter() {
            if value_id.0 < def_instr_vec.len() {
                def_instr_vec[value_id.0] = Some(instr.clone());
            }
        }

        // Track whether a ValueId is used outside its defining block. Such values must be spilled,
        // because we cannot inline across control-flow without dominance reasoning.
        let mut cross_block_use: HashSet<ValueId> = HashSet::new();

        for (bbid, bb) in &self.blocks {
            for (_, instr) in &bb.instrs {
                collect_value_uses_in_instr(instr, &mut uses);
                collect_cross_block_uses_in_instr(*bbid, instr, &def_block, &mut cross_block_use);
            }
            collect_value_uses_in_term(&bb.term, &mut uses);
            collect_cross_block_uses_in_term(*bbid, &bb.term, &def_block, &mut cross_block_use);
        }

        let mut spill: HashSet<ValueId> = HashSet::new();
        for bb in self.blocks.values() {
            for (value_id, instr) in &bb.instrs {
                let id = uses.get(value_id).copied().unwrap_or(0);
                if id > 1 {
                    spill.insert(*value_id);
                    continue;
                }
                if instr.has_side_effects() && id > 0 {
                    spill.insert(*value_id);
                }
            }
        }
        spill.extend(cross_block_use.iter().copied());

        // If `v` is only used as both operands of `v op v` (total use count 2), do not spill:
        // codegen emits the value once and `DUP`s for the second operand.
        for bb in self.blocks.values() {
            for (_, instr) in &bb.instrs {
                if let Instr::Binary { left, right, .. } = instr {
                    if left == right {
                        if let ValueRef::Value(id) = left {
                            if uses.get(&id).copied() == Some(2) && !cross_block_use.contains(&id) {
                                spill.remove(&id);
                            }
                        }
                    }
                }
            }
        }

        // Allocate local slots for spilled ValueIds and for every block parameter (BlockId, param_idx).
        let mut next_local: u8 = 0;
        let mut value_slot: HashMap<ValueId, u8> = HashMap::new();
        for i in 0..self.value_count {
            let id = ValueId(i);
            if spill.contains(&id) {
                value_slot.insert(id, next_local);
                next_local = next_local
                    .checked_add(1)
                    .ok_or(CodegenError::LocalLimitExceeded)?;
                if next_local == u8::MAX {
                    return Err(CodegenError::LocalLimitExceeded);
                }
            }
        }
        // Block-parameter SSA slots (locals). Entry-block parameters are NeoVM *arguments* (`LDARG*`),
        // not extra locals — do not allocate `param_slot` for `ir.entry` (see block prologue + `emit_value_ref`).
        let mut param_slot: HashMap<(BlockId, usize), u8> = HashMap::new();
        for (bbid, bb) in &self.blocks {
            if *bbid == self.entry {
                continue;
            }
            for (index, _p) in bb.params.iter().enumerate() {
                param_slot.insert((*bbid, index), next_local);
                next_local = next_local
                    .checked_add(1)
                    .ok_or(CodegenError::LocalLimitExceeded)?;
                if next_local == u8::MAX {
                    return Err(CodegenError::LocalLimitExceeded);
                }
            }
        }

        let mut compound_local_pairs: Vec<(u8, u8)> = Vec::new();
        for bb in self.blocks.values() {
            for (_, instr) in &bb.instrs {
                if matches!(instr, Instr::ContractMapStorageCompound { .. }) {
                    let k = next_local;
                    next_local = next_local
                        .checked_add(1)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    if next_local == u8::MAX {
                        return Err(CodegenError::LocalLimitExceeded);
                    }
                    let vl = next_local;
                    next_local = next_local
                        .checked_add(1)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    if next_local == u8::MAX {
                        return Err(CodegenError::LocalLimitExceeded);
                    }
                    compound_local_pairs.push((k, vl));
                }
            }
        }

        let mut call_patches: Vec<(usize, String)> = Vec::new();
        let mut compound_emit_idx: usize = 0;

        let mut block_start: HashMap<BlockId, usize> = HashMap::new();
        let mut pending_jmps: Vec<(usize, BlockId)> = Vec::new();

        // Emit blocks in deterministic id order.
        for (bbid, bb) in &self.blocks {
            block_start.insert(*bbid, builder.cursor());

            // Block entry: phi parameters from predecessors arrive on the stack (last param on top).
            // The function entry block uses NeoVM argument slots instead (`LDARG*`); do not pop here.
            if *bbid != self.entry {
                for (index, _p) in bb.params.iter().enumerate().rev() {
                    let slot = *param_slot.get(&(*bbid, index)).expect("param slot");
                    builder.emit_stloc(slot);
                }
            }

            // Per-block definition table for on-demand emission of pure instructions.
            let mut defs: HashMap<ValueId, Instr> = HashMap::new();
            for (out, instr) in &bb.instrs {
                defs.insert(*out, instr.clone());
            }
            let mut emitted_spills: HashSet<ValueId> = HashSet::new();
            let ctx = IrStackifyContext {
                defs: &defs,
                all_defs: &def_instr_vec,
                uses: &uses,
                spill: &spill,
                value_slot: &value_slot,
                param_slot: &param_slot,
                entry_bb: self.entry,
                arg_count,
            };
            let mut mux = IrSideEffectMux {
                compound_pairs: &compound_local_pairs,
                compound_idx: &mut compound_emit_idx,
                call_patches: &mut call_patches,
            };

            // Emit side-effecting instructions in order; pure ones are emitted on-demand.
            for (out, instr) in &bb.instrs {
                if !instr.has_side_effects() {
                    continue;
                }
                builder.emit_instr_stackified(
                    &ctx,
                    &mut emitted_spills,
                    *bbid,
                    &mut mux,
                    *out,
                    instr,
                )?;
            }

            // Ensure any value used across blocks is materialized into its spill slot in its defining block.
            for id in cross_block_use.iter().copied() {
                if def_block.get(&id).copied() != Some(*bbid) {
                    continue;
                }
                if !spill.contains(&id) {
                    continue;
                }
                if emitted_spills.contains(&id) {
                    continue;
                }
                // Only support spilling pure defs cross-block in phase 1.
                let Some(instr) = defs.get(&id) else { continue };
                if instr.has_side_effects() {
                    continue;
                }
                let Some(slot) = value_slot.get(&id).copied() else {
                    continue;
                };
                builder.emit_pure_instr_stackified(&ctx, &mut emitted_spills, *bbid, instr)?;
                builder.emit_stloc(slot);
                emitted_spills.insert(id);
            }

            match &bb.term {
                Terminator::Return(value) => {
                    if let Some(value) = value {
                        builder.emit_value_ref_stackified(
                            &ctx,
                            &mut emitted_spills,
                            *bbid,
                            *value,
                        )?;
                    } else {
                        builder.push_null();
                    }
                    builder.emit(OpCode::RET);
                }
                Terminator::Jump { target, args } => {
                    builder.emit_jump_args_stackified(&ctx, &mut emitted_spills, *bbid, args)?;
                    let j = builder.emit_jmp_l_placeholder();
                    pending_jmps.push((j, *target));
                }
                Terminator::Branch {
                    cond,
                    then_bb,
                    then_args,
                    else_bb,
                    else_args,
                } => {
                    builder.emit_value_ref_stackified(&ctx, &mut emitted_spills, *bbid, *cond)?;
                    let jmp_else = builder.emit_jmpifnot_l_placeholder();

                    // then path
                    builder.emit_jump_args_stackified(
                        &ctx,
                        &mut emitted_spills,
                        *bbid,
                        then_args,
                    )?;
                    let jmp_then = builder.emit_jmp_l_placeholder();
                    pending_jmps.push((jmp_then, *then_bb));

                    // else stub location
                    let else_stub = builder.cursor();
                    builder.patch_jmp_target_at_instruction(jmp_else, else_stub);
                    builder.emit_jump_args_stackified(
                        &ctx,
                        &mut emitted_spills,
                        *bbid,
                        else_args,
                    )?;
                    let jmp_to_else = builder.emit_jmp_l_placeholder();
                    pending_jmps.push((jmp_to_else, *else_bb));
                }
            }
        }

        // Patch all jumps.
        for (inst_idx, target_bb) in pending_jmps {
            let target_pc = *block_start.get(&target_bb).ok_or_else(|| {
                CodegenError::Unsupported("ir-codegen: missing block start".into())
            })?;
            builder.patch_jmp_target_at_instruction(inst_idx, target_pc);
        }

        builder.patch_initslot_local_count(initslot_idx, next_local);
        Ok(CompliledFunction {
            instructions: builder.into_instructions(),
            call_patches,
        })
    }
}

impl Builder {
    fn emit_jump_args_stackified(
        &mut self,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        cur_bb: BlockId,
        args: &[ValueRef],
    ) -> Result<(), CodegenError> {
        // Push in reverse, so that param0 ends up deepest (caller pushes args in order).
        for arg in args.iter().rev() {
            self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *arg)?;
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
        cur_bb: BlockId,
        field_name: &str,
        key_ty: &Type,
        key: ValueRef,
    ) -> Result<(), CodegenError> {
        let mut prefix = field_name.as_bytes().to_vec();
        prefix.push(0);
        self.push_data(&prefix);
        self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, key)?;
        self.emit_map_key_as_bytestring(key_ty)?;
        self.emit(OpCode::CAT);
        Ok(())
    }

    fn emit_value_ref_stackified(
        &mut self,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        cur_bb: BlockId,
        value_ref: ValueRef,
    ) -> Result<(), CodegenError> {
        match value_ref {
            ValueRef::Value(id) => self.emit_value_id_stackified(ctx, emitted_spills, cur_bb, id),
            ValueRef::Param(ParamId(id)) => {
                if cur_bb == ctx.entry_bb {
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
                    let slot = ctx.param_slot.get(&(cur_bb, id)).copied().ok_or_else(|| {
                        CodegenError::Unsupported(format!(
                            "ir-codegen: unknown param {id} in block {:?}",
                            cur_bb
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
        cur_bb: BlockId,
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
                self.emit_pure_instr_stackified(ctx, emitted_spills, cur_bb, instr)?;
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
                    id, cur_bb
                ))
            })?;
        if instr.has_side_effects() {
            return Err(CodegenError::Unsupported(
                "ir-codegen: side-effect value must be spilled or emitted in order".into(),
            ));
        }
        self.emit_pure_instr_stackified(ctx, emitted_spills, cur_bb, instr)
    }

    fn emit_pure_instr_stackified(
        &mut self,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        cur_bb: BlockId,
        instr: &Instr,
    ) -> Result<(), CodegenError> {
        match instr {
            Instr::Const(lit) => {
                self.emit_literal(lit)?;
                Ok(())
            }
            Instr::StructFieldGet { base, index } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *base)?;
                self.push_int(*index as i64);
                self.emit(OpCode::PICKITEM);
                Ok(())
            }
            Instr::IndexGet { base, index } => {
                if base == index {
                    // e.g. `a[a]` — one evaluation, duplicate for PICKITEM operands.
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *base)?;
                    self.emit(OpCode::DUP);
                } else {
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *base)?;
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *index)?;
                }
                self.emit(OpCode::PICKITEM);
                Ok(())
            }
            Instr::IndexSet { .. } | Instr::StructFieldSet { .. } => {
                Err(CodegenError::Unsupported(
                    "ir-codegen: IndexSet/StructFieldSet must be emitted in-order".into(),
                ))
            }
            Instr::Copy(value) => {
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *value)
            }
            Instr::Unary { op, value } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *value)?;
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
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *left)?;
                    self.emit(OpCode::DUP);
                } else {
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *left)?;
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *right)?;
                }
                match op {
                    BinaryOp::Mul => self.emit(OpCode::MUL),
                    BinaryOp::Div => self.emit(OpCode::DIV),
                    BinaryOp::Mod => self.emit(OpCode::MOD),
                    BinaryOp::Add => self.emit(OpCode::ADD),
                    BinaryOp::Sub => self.emit(OpCode::SUB),
                    BinaryOp::Shl => self.emit(OpCode::SHL),
                    BinaryOp::Shr => self.emit(OpCode::SHR),
                    BinaryOp::BitAnd => self.emit(OpCode::AND),
                    BinaryOp::BitOr => self.emit(OpCode::OR),
                    BinaryOp::BitXor => self.emit(OpCode::XOR),
                    BinaryOp::Eq => self.emit(OpCode::EQUAL),
                    BinaryOp::Ne => self.emit(OpCode::NOTEQUAL),
                    BinaryOp::Lt => self.emit(OpCode::LT),
                    BinaryOp::Le => self.emit(OpCode::LE),
                    BinaryOp::Gt => self.emit(OpCode::GT),
                    BinaryOp::Ge => self.emit(OpCode::GE),
                    BinaryOp::And | BinaryOp::Or => {
                        return Err(CodegenError::Unsupported(
                            "ir-codegen: logical and/or not supported".into(),
                        ));
                    }
                }
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
                    cur_bb,
                    field,
                    key_ty,
                    *key,
                )?;
                self.emit_syscall(Syscall::STORAGE_LOCAL_GET);
                self.emit_convert_buffer_on_stack_to_type(val_ty)?;
                Ok(())
            }
            Instr::StructPack { field_values, .. } => {
                for vr in field_values {
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *vr)?;
                }
                self.push_int(field_values.len() as i64);
                self.emit(OpCode::PACK);
                Ok(())
            }
            Instr::ArrayPack { elements } => {
                for vr in elements.iter().rev() {
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *vr)?;
                }
                self.push_int(elements.len() as i64);
                self.emit(OpCode::PACK);
                Ok(())
            }
            Instr::MapPack { pairs } => {
                for (k, v) in pairs.iter().rev() {
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *v)?;
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *k)?;
                }
                self.push_int(pairs.len() as i64);
                self.emit(OpCode::PACKMAP);
                Ok(())
            }
            Instr::Cast { value, ty } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *value)?;
                let op = convert_operand_for_type(ty).ok_or_else(|| {
                    CodegenError::Unsupported(format!(
                        "ir-codegen: `as` to `{ty:?}` is not supported yet",
                    ))
                })?;
                self.emit_with_operands(OpCode::CONVERT, std::slice::from_ref(&op));
                Ok(())
            }
            Instr::Min { left, right } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *left)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *right)?;
                self.emit(OpCode::MIN);
                Ok(())
            }
            Instr::Max { left, right } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *left)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *right)?;
                self.emit(OpCode::MAX);
                Ok(())
            }
            Instr::ContractStoragePut { .. }
            | Instr::ContractMapStoragePut { .. }
            | Instr::ContractMapStorageCompound { .. }
            | Instr::Assert { .. }
            | Instr::Abort { .. }
            | Instr::Emit { .. }
            | Instr::PackageCall { .. }
            | Instr::StructInstanceCall { .. }
            | Instr::RuntimeLog { .. } => Err(CodegenError::Unsupported(
                "ir-codegen: side-effecting instruction reached pure emitter".into(),
            )),
            Instr::EvalAst(_) => Err(CodegenError::Unsupported(
                "ir-codegen: EvalAst should not appear in phase 1".into(),
            )),
        }
    }

    fn emit_instr_stackified(
        &mut self,
        ctx: &IrStackifyContext<'_>,
        emitted_spills: &mut HashSet<ValueId>,
        cur_bb: BlockId,
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
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *base)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *index)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *value)?;

                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.emit(OpCode::DUP);
                }
                self.emit(OpCode::SETITEM);
                if out_uses > 0 {
                    // The assignment expression evaluates to the assigned value.
                    if spill.contains(&out) {
                        let slot = *value_slot
                            .get(&out)
                            .ok_or(CodegenError::LocalLimitExceeded)?;
                        self.emit(OpCode::DUP);
                        self.emit_stloc(slot);
                        emitted_spills.insert(out);
                    }
                }
                Ok(())
            }
            Instr::StructFieldSet { base, index, value } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *base)?;
                self.push_int(*index as i64);
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *value)?;

                let out_uses = uses.get(&out).copied().unwrap_or(0);
                if out_uses > 0 {
                    self.emit(OpCode::DUP);
                }
                self.emit(OpCode::SETITEM);
                if out_uses > 0 {
                    if spill.contains(&out) {
                        let slot = *value_slot
                            .get(&out)
                            .ok_or(CodegenError::LocalLimitExceeded)?;
                        self.emit(OpCode::DUP);
                        self.emit_stloc(slot);
                        emitted_spills.insert(out);
                    }
                }
                Ok(())
            }
            Instr::ContractStoragePut {
                field,
                value_ty,
                value,
            } => {
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *value)?;
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
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *value)?;
                self.emit_contract_map_composite_key(
                    ctx,
                    emitted_spills,
                    cur_bb,
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
                let pair = mux.compound_pairs.get(*mux.compound_idx).ok_or_else(|| {
                    CodegenError::Unsupported("ir-codegen: compound scratch slots".into())
                })?;
                *mux.compound_idx += 1;
                let key_slot = pair.0;
                let val_slot = pair.1;

                self.emit_contract_map_composite_key(
                    ctx,
                    emitted_spills,
                    cur_bb,
                    field,
                    key_ty,
                    *key,
                )?;
                self.emit(OpCode::DUP);
                self.emit_stloc(key_slot);
                self.emit_syscall(Syscall::STORAGE_LOCAL_GET);
                self.emit_convert_buffer_on_stack_to_type(val_ty)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *value)?;
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
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *message)?;
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
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *cond)?;
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *message)?;
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
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *arg)?;
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
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *arg)?;
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
            Instr::StructInstanceCall {
                struct_name,
                method,
                recv,
                args,
            } => {
                for arg in args.iter().rev() {
                    self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *arg)?;
                }
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *recv)?;
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
                self.emit_value_ref_stackified(ctx, emitted_spills, cur_bb, *message)?;
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
                if n < i64::MIN as i128 || n > i64::MAX as i128 {
                    self.push_int128(n);
                } else {
                    self.push_int(n as i64);
                }
            }
            Literal::String(s) => self.push_data(s.as_bytes()),
            Literal::Buffer(s) => self.push_data(s.as_bytes()),
        }
        Ok(())
    }
}

fn collect_value_uses_in_instr(instr: &Instr, out: &mut HashMap<ValueId, usize>) {
    fn bump(value: ValueRef, out: &mut HashMap<ValueId, usize>) {
        if let ValueRef::Value(id) = value {
            *out.entry(id).or_insert(0) += 1;
        }
    }
    match instr {
        Instr::Const(_) => {}
        Instr::StructFieldGet { base, .. } => bump(*base, out),
        Instr::IndexGet { base, index } => {
            bump(*base, out);
            bump(*index, out);
        }
        Instr::IndexSet { base, index, value } => {
            bump(*base, out);
            bump(*index, out);
            bump(*value, out);
        }
        Instr::StructFieldSet { base, value, .. } => {
            bump(*base, out);
            bump(*value, out);
        }
        Instr::Unary { value, .. } | Instr::Copy(value) => bump(*value, out),
        Instr::Binary { left, right, .. } => {
            bump(*left, out);
            bump(*right, out);
        }
        Instr::Cast { value, .. } => bump(*value, out),
        Instr::Min { left, right } | Instr::Max { left, right } => {
            bump(*left, out);
            bump(*right, out);
        }
        Instr::Abort { message } => bump(*message, out),
        Instr::ContractStorageGet { .. } => {}
        Instr::ContractStoragePut { value, .. } => bump(*value, out),
        Instr::ContractMapStorageGet { key, .. } => bump(*key, out),
        Instr::ContractMapStoragePut { key, value, .. } => {
            bump(*key, out);
            bump(*value, out);
        }
        Instr::ContractMapStorageCompound { key, value, .. } => {
            bump(*key, out);
            bump(*value, out);
        }
        Instr::Assert { cond, message } => {
            bump(*cond, out);
            bump(*message, out);
        }
        Instr::Emit { args, .. } => {
            for arg in args {
                bump(*arg, out);
            }
        }
        Instr::PackageCall { args, .. } => {
            for arg in args {
                bump(*arg, out);
            }
        }
        Instr::StructPack { field_values, .. } => {
            for value in field_values {
                bump(*value, out);
            }
        }
        Instr::StructInstanceCall { recv, args, .. } => {
            bump(*recv, out);
            for arg in args {
                bump(*arg, out);
            }
        }
        Instr::RuntimeLog { message } => bump(*message, out),
        Instr::ArrayPack { elements } => {
            for value in elements {
                bump(*value, out);
            }
        }
        Instr::MapPack { pairs } => {
            for (key, value) in pairs {
                bump(*key, out);
                bump(*value, out);
            }
        }
        Instr::EvalAst(_) => {}
    }
}

fn collect_cross_block_uses_in_instr(
    use_bb: BlockId,
    instr: &Instr,
    def_block: &HashMap<ValueId, BlockId>,
    out: &mut HashSet<ValueId>,
) {
    fn bump(
        use_bb: BlockId,
        value: ValueRef,
        def_block: &HashMap<ValueId, BlockId>,
        out: &mut HashSet<ValueId>,
    ) {
        let ValueRef::Value(id) = value else { return };
        if def_block.get(&id).copied().is_some_and(|db| db != use_bb) {
            out.insert(id);
        }
    }
    match instr {
        Instr::Const(_) => {}
        Instr::StructFieldGet { base, .. } => bump(use_bb, *base, def_block, out),
        Instr::IndexGet { base, index } => {
            bump(use_bb, *base, def_block, out);
            bump(use_bb, *index, def_block, out);
        }
        Instr::IndexSet { base, index, value } => {
            bump(use_bb, *base, def_block, out);
            bump(use_bb, *index, def_block, out);
            bump(use_bb, *value, def_block, out);
        }
        Instr::StructFieldSet { base, value, .. } => {
            bump(use_bb, *base, def_block, out);
            bump(use_bb, *value, def_block, out);
        }
        Instr::Unary { value, .. } | Instr::Copy(value) => bump(use_bb, *value, def_block, out),
        Instr::Binary { left, right, .. } => {
            bump(use_bb, *left, def_block, out);
            bump(use_bb, *right, def_block, out);
        }
        Instr::Cast { value, .. } => bump(use_bb, *value, def_block, out),
        Instr::Min { left, right } | Instr::Max { left, right } => {
            bump(use_bb, *left, def_block, out);
            bump(use_bb, *right, def_block, out);
        }
        Instr::Abort { message } => bump(use_bb, *message, def_block, out),
        Instr::ContractStorageGet { .. } => {}
        Instr::ContractStoragePut { value, .. } => bump(use_bb, *value, def_block, out),
        Instr::ContractMapStorageGet { key, .. } => bump(use_bb, *key, def_block, out),
        Instr::ContractMapStoragePut { key, value, .. } => {
            bump(use_bb, *key, def_block, out);
            bump(use_bb, *value, def_block, out);
        }
        Instr::ContractMapStorageCompound { key, value, .. } => {
            bump(use_bb, *key, def_block, out);
            bump(use_bb, *value, def_block, out);
        }
        Instr::Assert { cond, message } => {
            bump(use_bb, *cond, def_block, out);
            bump(use_bb, *message, def_block, out);
        }
        Instr::Emit { args, .. } => {
            for arg in args {
                bump(use_bb, *arg, def_block, out);
            }
        }
        Instr::PackageCall { args, .. } => {
            for arg in args {
                bump(use_bb, *arg, def_block, out);
            }
        }
        Instr::StructPack { field_values, .. } => {
            for value in field_values {
                bump(use_bb, *value, def_block, out);
            }
        }
        Instr::StructInstanceCall { recv, args, .. } => {
            bump(use_bb, *recv, def_block, out);
            for arg in args {
                bump(use_bb, *arg, def_block, out);
            }
        }
        Instr::RuntimeLog { message } => bump(use_bb, *message, def_block, out),
        Instr::ArrayPack { elements } => {
            for value in elements {
                bump(use_bb, *value, def_block, out);
            }
        }
        Instr::MapPack { pairs } => {
            for (key, value) in pairs {
                bump(use_bb, *key, def_block, out);
                bump(use_bb, *value, def_block, out);
            }
        }
        Instr::EvalAst(_) => {}
    }
}

fn collect_value_uses_in_term(terminator: &Terminator, out: &mut HashMap<ValueId, usize>) {
    fn bump(value: ValueRef, out: &mut HashMap<ValueId, usize>) {
        if let ValueRef::Value(id) = value {
            *out.entry(id).or_insert(0) += 1;
        }
    }
    match terminator {
        Terminator::Return(value) => {
            if let Some(value) = value {
                bump(*value, out);
            }
        }
        Terminator::Jump { args, .. } => {
            for arg in args {
                bump(*arg, out);
            }
        }
        Terminator::Branch {
            cond,
            then_args,
            else_args,
            ..
        } => {
            bump(*cond, out);
            for arg in then_args.iter().chain(else_args.iter()) {
                bump(*arg, out);
            }
        }
    }
}

fn collect_cross_block_uses_in_term(
    use_bb: BlockId,
    terminator: &Terminator,
    def_block: &HashMap<ValueId, BlockId>,
    out: &mut HashSet<ValueId>,
) {
    fn bump(
        use_bb: BlockId,
        value: ValueRef,
        def_block: &HashMap<ValueId, BlockId>,
        out: &mut HashSet<ValueId>,
    ) {
        let ValueRef::Value(id) = value else { return };
        if def_block.get(&id).copied().is_some_and(|db| db != use_bb) {
            out.insert(id);
        }
    }
    match terminator {
        Terminator::Return(value) => {
            if let Some(value) = value {
                bump(use_bb, *value, def_block, out);
            }
        }
        Terminator::Jump { args, .. } => {
            for arg in args {
                bump(use_bb, *arg, def_block, out);
            }
        }
        Terminator::Branch {
            cond,
            then_args,
            else_args,
            ..
        } => {
            bump(use_bb, *cond, def_block, out);
            for arg in then_args.iter().chain(else_args.iter()) {
                bump(use_bb, *arg, def_block, out);
            }
        }
    }
}
