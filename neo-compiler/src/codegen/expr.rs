//! Expression lowering to NeoVM instructions (`ExprGen`).

use std::collections::HashMap;

use crate::codegen::env::*;
use crate::codegen::CodegenError;
use crate::syntax::ast::*;
use crate::target::opcode::{OpCode, ToOpCode};
use crate::target::syscall::*;
use crate::target::{Builder, StackItemType};

pub fn convert_operand_for_type(ty: &Type) -> Option<u8> {
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

pub(crate) fn parse_int_literal(raw: &str) -> Option<i128> {
    let raw: String = raw.chars().filter(|&c| c != '_').collect();
    let value = if raw.len() > 2 && (raw.starts_with("0x") || raw.starts_with("0X")) {
        i128::from_str_radix(&raw[2..], 16).ok()?
    } else if raw.len() > 2 && (raw.starts_with("0b") || raw.starts_with("0B")) {
        i128::from_str_radix(&raw[2..], 2).ok()?
    } else {
        raw.parse::<i128>().ok()?
    };
    Some(value)
}

// Borrows the pieces of `FunctionCompiler` needed to lower expressions.
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
                let op = convert_operand_for_type(ty).ok_or_else(|| {
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

    fn emit_member_access(&mut self, base: &Expr, field: &str) -> Result<(), CodegenError> {
        match base {
            Expr::Ident(var) => {
                let struct_name = self
                    .value_struct
                    .get(var)
                    .cloned()
                    .ok_or_else(|| {
                        CodegenError::Unsupported("member access needs a variable with struct type".into())
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
                "only `variable.field` or `self.field` member access is allowed (no chained `a.b.c` yet)".into(),
            )),
        }
    }

    fn emit_struct_lit(
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

    fn emit_index_access(&mut self, base: &Expr, index: &Expr) -> Result<(), CodegenError> {
        if let Some((map_name, key_ty, val_ty)) = self.contract_self_map_field_types(base)? {
            self.emit_contract_map_get(&map_name, &key_ty, &val_ty, index)?;
            return Ok(());
        }
        self.compile_expr(base)?;
        self.compile_expr(index)?;
        self.builder.emit(OpCode::PICKITEM);
        Ok(())
    }

    fn emit_map_lit(&mut self, pairs: &[(Expr, Expr)]) -> Result<(), CodegenError> {
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

    fn emit_array_lit(&mut self, items: &[Expr]) -> Result<(), CodegenError> {
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

    fn field_index_of(&self, struct_name: &str, field: &str) -> Result<usize, CodegenError> {
        let s = self
            .structs
            .iter()
            .find(|s| s.name == struct_name)
            .ok_or_else(|| {
                CodegenError::Unsupported(format!("unknown struct type `{struct_name}`"))
            })?;
        s.fields
            .iter()
            .position(|f| f.name == field)
            .ok_or_else(|| {
                CodegenError::Unsupported(format!("struct `{struct_name}` has no field `{field}`"))
            })
    }

    fn contract_field_required(&self, field: &str) -> Result<&ContractField, CodegenError> {
        let fields = self.contract_fields.ok_or_else(|| {
            CodegenError::Unsupported("`self` is only valid on contract storage fields".into())
        })?;
        fields
            .iter()
            .find(|f| f.name == field)
            .ok_or_else(|| CodegenError::Unsupported(format!("unknown contract field `{field}`")))
    }

    /// `self.map[key]` when `base` is `self.map` and the contract field is a `map`.
    /// Returns `(field_name, key_type, value_type)` by clone to avoid borrow conflicts with `&mut self`.
    fn contract_self_map_field_types(
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

    fn compile_contract_member_load(&mut self, field: &str) -> Result<(), CodegenError> {
        let contract_field = self.contract_field_required(field)?;
        let ty = contract_field.ty.clone();
        if ty.is_map() {
            return Err(CodegenError::Unsupported(format!(
                "use `{field}[key]` to read contract map `{field}` entries (whole-map load is not implemented)"
            )));
        }
        if ty.is_array() {
            return Err(CodegenError::Unsupported(
                "contract array storage field read is not implemented yet".into(),
            ));
        }
        self.builder.push_data(field.as_bytes());
        self.builder.emit_syscall(Syscall::STORAGE_LOCAL_GET);
        self.emit_convert_buffer_on_stack_to_type(&ty)
    }

    fn emit_convert_buffer_on_stack_to_type(&mut self, ty: &Type) -> Result<(), CodegenError> {
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

    fn emit_convert_stack_top_to_storage_buffer(&mut self, ty: &Type) -> Result<(), CodegenError> {
        let op = match ty {
            Type::Bool | Type::Int => StackItemType::Buffer as u8,
            Type::String | Type::Hash160 | Type::Hash256 | Type::Buffer => {
                StackItemType::Buffer as u8
            }
            _ => {
                return Err(CodegenError::Unsupported(format!(
                    "storage put for type `{ty:?}` is not implemented yet"
                )));
            }
        };
        self.builder
            .emit_with_operands(OpCode::CONVERT, std::slice::from_ref(&op));
        Ok(())
    }

    fn emit_map_key_as_bytestring(&mut self, key_ty: &Type) -> Result<(), CodegenError> {
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

    /// Stack: … → …, composite_key (ByteString), where key = `{field_name}\0` ‖ key_bytes.
    fn emit_contract_map_key_on_stack(
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

    fn emit_contract_map_get(
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

    fn emit_contract_map_assign(
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

    fn compile_contract_map_compound_assign(
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
        let val_slot = self.env.alloc_slot()?;
        self.builder.emit_stloc(val_slot);
        self.builder.emit_ldloc(key_slot);
        self.builder.emit_ldloc(val_slot);
        self.emit_convert_stack_top_to_storage_buffer(value_ty)?;
        self.builder.emit(OpCode::SWAP);
        self.builder.emit_syscall(Syscall::STORAGE_LOCAL_PUT);
        self.builder.emit_ldloc(val_slot);
        Ok(())
    }

    fn compile_assign(
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
                    CodegenError::Unsupported(
                        "invalid compound-assignment receiver for `self`".into(),
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
                    let cf = self.contract_field_required(field)?;
                    let ty = cf.ty.clone();
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

    fn compile_binary(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<(), CodegenError> {
        match op {
            BinaryOp::And => {
                self.compile_expr(left)?;
                let jmp_short = self.builder.emit_jmpifnot_l_placeholder();
                self.compile_expr(right)?;
                let jmp_end = self.builder.emit_jmp_l_placeholder();
                let false_label = self.builder.cursor();
                self.builder
                    .patch_jmp_target_at_instruction(jmp_short, false_label);
                self.builder.push_bool(false);
                let end = self.builder.cursor();
                self.builder.patch_jmp_target_at_instruction(jmp_end, end);
                Ok(())
            }
            BinaryOp::Or => {
                self.compile_expr(left)?;
                self.builder.emit(OpCode::DUP);
                let jmp_done = self.builder.emit_jmpif_l_placeholder();
                self.builder.emit(OpCode::DROP);
                self.compile_expr(right)?;
                let end = self.builder.cursor();
                self.builder.patch_jmp_target_at_instruction(jmp_done, end);
                Ok(())
            }
            _ => {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.builder.emit(op.to_op_code());
                Ok(())
            }
        }
    }

    fn compile_unary(&mut self, op: UnaryOp, expr: &Expr) -> Result<(), CodegenError> {
        match op {
            UnaryOp::Positive => self.compile_expr(expr),
            UnaryOp::Negative => {
                self.compile_expr(expr)?;
                self.builder.emit(OpCode::NEGATE);
                Ok(())
            }
            UnaryOp::Not => {
                self.compile_expr(expr)?;
                self.builder.emit(OpCode::NOT);
                Ok(())
            }
            UnaryOp::BitNot => {
                self.compile_expr(expr)?;
                self.builder.emit(OpCode::INVERT);
                Ok(())
            }
        }
    }

    fn compile_call(&mut self, callee: &Expr, args: &[Expr]) -> Result<(), CodegenError> {
        if let Expr::Member { base, field } = callee {
            if let Expr::Ident(pkg) = base.as_ref() {
                if pkg == "runtime" {
                    return self.compile_runtime_call(field, args);
                }
            }
            if let Expr::Ident(recv) = base.as_ref() {
                if self.value_struct.contains_key(recv) {
                    return self.compile_struct_instance_call(recv, field, args);
                }
            }
            return Err(CodegenError::Unsupported(
                "only `runtime.<method>` or struct instance `var.method(...)` support `x.y(...)` call syntax"
                    .into(),
            ));
        }
        if let Expr::Ident(name) = callee {
            match name.as_str() {
                "assert" if args.len() == 2 => {
                    self.compile_expr(&args[0])?;
                    self.compile_expr(&args[1])?;
                    self.builder.emit(OpCode::ASSERTMSG);
                    return Ok(());
                }
                "abort" if args.len() == 1 => {
                    self.compile_expr(&args[0])?;
                    self.builder.emit(OpCode::ABORTMSG);
                    return Ok(());
                }
                "min" if args.len() == 2 => {
                    self.compile_expr(&args[0])?;
                    self.compile_expr(&args[1])?;
                    self.builder.emit(OpCode::MIN);
                    return Ok(());
                }
                "max" if args.len() == 2 => {
                    self.compile_expr(&args[0])?;
                    self.compile_expr(&args[1])?;
                    self.builder.emit(OpCode::MAX);
                    return Ok(());
                }
                _ => {
                    if let Some(&expect_arity) = self.package_fn_arity.get(name) {
                        if args.len() != expect_arity {
                            return Err(CodegenError::Unsupported(format!(
                                "call to `{name}` expects {expect_arity} argument(s), got {}",
                                args.len()
                            )));
                        }
                        for arg in args.iter().rev() {
                            self.compile_expr(arg)?;
                        }
                        let index = self.builder.emit_call_l_placeholder();
                        self.pending_call_l.push((index, name.clone()));
                        return Ok(());
                    }
                }
            }
        }
        Err(CodegenError::Unsupported(
            "only package-level functions, built-in functions, struct methods, and runtime.* calls are supported"
                .into(),
        ))
    }

    /// `recv.method(args...)` → `CALL_L` to lowered `Struct::method`.
    /// Stack before `CALL_L` (bottom → top) matches `codegen` module docs: for `s.add(a, b)` it is `| b | a | s |`
    /// (explicit args pushed **reverse** source order, then receiver on top).
    fn compile_struct_instance_call(
        &mut self,
        receiver_var: &str,
        method: &str,
        args: &[Expr],
    ) -> Result<(), CodegenError> {
        let struct_name = self
            .value_struct
            .get(receiver_var)
            .cloned()
            .ok_or_else(|| {
                CodegenError::Unsupported(format!(
                    "`{receiver_var}.method(...)` needs `{receiver_var}` to be a struct-typed variable"
                ))
            })?;
        let struct_decl = self
            .structs
            .iter()
            .find(|s| s.name == struct_name)
            .ok_or_else(|| CodegenError::Unsupported(format!("unknown struct `{struct_name}`")))?;
        let method_decl = struct_decl.methods.iter().find(|m| m.name == method).ok_or_else(|| {
            CodegenError::Unsupported(format!(
                "struct `{struct_name}` has no method `{method}` (for `{receiver_var}.{method}(...)`)"
            ))
        })?;
        if args.len() != method_decl.params.len() {
            return Err(CodegenError::Unsupported(format!(
                "`{struct_name}::{method}` expects {} argument(s), got {}",
                method_decl.params.len(),
                args.len()
            )));
        }
        for arg in args.iter().rev() {
            self.compile_expr(arg)?;
        }
        self.compile_expr(&Expr::Ident(receiver_var.into()))?;
        let index = self.builder.emit_call_l_placeholder();
        let target = format!("{struct_name}.{method}");
        self.pending_call_l.push((index, target));
        Ok(())
    }

    fn compile_runtime_call(&mut self, method: &str, args: &[Expr]) -> Result<(), CodegenError> {
        // `System.Contract.Call` exposed as `runtime.contractCall` with injected read-only flags.
        // Syscall stack order matches `CALL`: bottom → top is last arg … first arg (see `codegen` module docs).
        if method == "contractCall" && args.len() == 3 {
            self.compile_expr(&args[2])?;
            self.builder.push_int(i64::from(CallFlags::ReadOnly as u8));
            self.compile_expr(&args[1])?;
            self.compile_expr(&args[0])?;
            self.builder.emit_syscall(Syscall::CONTRACT_CALL);
            return Ok(());
        }
        if let Some(syscall) = runtime_syscall_for_method(method) {
            if args.len() != syscall.args.len() {
                return Err(CodegenError::Unsupported(format!(
                    "runtime.{method} expects {} argument(s), got {}",
                    syscall.args.len(),
                    args.len()
                )));
            }
            for arg in args.iter().rev() {
                self.compile_expr(arg)?;
            }
            self.builder.emit_syscall(*syscall);
            return Ok(());
        }
        Err(CodegenError::Unsupported(format!(
            "runtime.{method} is not a known System.Runtime API or wrong arity"
        )))
    }

    fn emit_default_for_type(&mut self, ty: &Type) -> Result<(), CodegenError> {
        match ty {
            Type::Bool => self.builder.push_bool(false),
            Type::Int => self.builder.push_int(0),
            Type::String | Type::Hash160 | Type::Hash256 => self.builder.push_data(&[]),
            Type::Buffer => {
                self.builder.push_int(0);
                self.builder.emit(OpCode::NEWBUFFER);
            }
            Type::Array(_) | Type::Map { .. } => {
                self.builder.push_null();
            }
            Type::Any => self.builder.push_null(),
            Type::Void | Type::Named(_) => {
                return Err(CodegenError::Unsupported(format!(
                    "no default value for field type `{ty:?}` in struct literal"
                )));
            }
        }
        Ok(())
    }

    fn emit_literal(&mut self, lit: &Literal) -> Result<(), CodegenError> {
        match lit {
            Literal::Null => {
                self.builder.push_null();
                Ok(())
            }
            Literal::Bool(b) => {
                self.builder.push_bool(*b);
                Ok(())
            }
            Literal::Int(s) => {
                let n = parse_int_literal(s)
                    .ok_or_else(|| CodegenError::BadIntegerLiteral(s.clone()))?;
                if n < i64::MIN as i128 || n > i64::MAX as i128 {
                    self.builder.push_int128(n);
                } else {
                    self.builder.push_int(n as i64);
                }
                Ok(())
            }
            Literal::String(s) => {
                self.builder.push_data(s.as_bytes());
                Ok(())
            }
            Literal::Buffer(s) => {
                self.builder.push_data(s.as_bytes());
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::syscall::Syscall;
    use crate::target::Instruction;

    fn compile_expr_stub<'a>(
        params: &[Param],
        structs: &'a [StructDecl],
        value_struct: &mut HashMap<String, String>,
        expr: &Expr,
    ) -> Result<Vec<Instruction>, CodegenError> {
        let mut env = VarEnv::new(params)?;
        let mut builder = Builder::new();
        let mut pending = Vec::new();
        let empty_pkg = HashMap::new();
        ExprGen {
            builder: &mut builder,
            env: &mut env,
            structs,
            value_struct,
            contract_fields: None,
            pending_call_l: &mut pending,
            package_fn_arity: &empty_pkg,
        }
        .compile_expr(expr)?;
        Ok(builder.into_instructions())
    }

    #[test]
    fn convert_operand_maps_primitive_types() {
        assert_eq!(
            convert_operand_for_type(&Type::Bool),
            Some(StackItemType::Boolean as u8)
        );
        assert_eq!(
            convert_operand_for_type(&Type::Int),
            Some(StackItemType::Integer as u8)
        );
        assert_eq!(
            convert_operand_for_type(&Type::String),
            Some(StackItemType::ByteString as u8)
        );
        assert_eq!(
            convert_operand_for_type(&Type::Hash160),
            Some(StackItemType::ByteString as u8)
        );
        assert_eq!(
            convert_operand_for_type(&Type::Buffer),
            Some(StackItemType::Buffer as u8)
        );
        assert_eq!(
            convert_operand_for_type(&Type::Array(Box::new(Type::Int))),
            Some(StackItemType::Array as u8)
        );
        assert_eq!(
            convert_operand_for_type(&Type::Map {
                key: Box::new(Type::Int),
                value: Box::new(Type::String),
            }),
            Some(StackItemType::Map as u8)
        );
    }

    #[test]
    fn convert_operand_named_and_void_are_none() {
        assert_eq!(convert_operand_for_type(&Type::Named("T".into())), None);
        assert_eq!(convert_operand_for_type(&Type::Void), None);
        assert_eq!(convert_operand_for_type(&Type::Any), None);
    }

    #[test]
    fn parse_int_decimal_hex_binary() {
        assert_eq!(parse_int_literal("0"), Some(0));
        assert_eq!(parse_int_literal("-42"), Some(-42));
        assert_eq!(parse_int_literal("1_000"), Some(1000));
        assert_eq!(parse_int_literal("0xFF"), Some(255));
        assert_eq!(parse_int_literal("0X10"), Some(16));
        assert_eq!(parse_int_literal("0b1010"), Some(10));
        assert_eq!(parse_int_literal("0B11"), Some(3));
    }

    #[test]
    fn parse_int_invalid() {
        assert_eq!(parse_int_literal(""), None);
        assert_eq!(parse_int_literal("0x"), None);
        assert_eq!(parse_int_literal("0b"), None);
        assert_eq!(parse_int_literal("not_a_number"), None);
    }

    #[test]
    fn expr_literal_bool_null() {
        let mut vs = HashMap::new();
        let inst =
            compile_expr_stub(&[], &[], &mut vs, &Expr::Literal(Literal::Bool(true))).unwrap();
        assert_eq!(inst.len(), 1);
        assert_eq!(inst[0].opcode, OpCode::PUSHT);

        let inst = compile_expr_stub(&[], &[], &mut vs, &Expr::Literal(Literal::Null)).unwrap();
        assert_eq!(inst[0].opcode, OpCode::PUSHNULL);
    }

    #[test]
    fn expr_literal_int_string() {
        let mut vs = HashMap::new();
        let inst =
            compile_expr_stub(&[], &[], &mut vs, &Expr::Literal(Literal::Int("7".into()))).unwrap();
        assert_eq!(inst[0].opcode, OpCode::PUSH7);

        let inst = compile_expr_stub(
            &[],
            &[],
            &mut vs,
            &Expr::Literal(Literal::String("ab".into())),
        )
        .unwrap();
        assert_eq!(inst[0].opcode, OpCode::PUSHDATA1);
        assert_eq!(inst[0].operands[1..], *b"ab");
    }

    #[test]
    fn expr_ident_loads_arg() {
        let mut vs = HashMap::new();
        let params = vec![Param {
            ty: Type::Int,
            name: "n".into(),
        }];
        let inst = compile_expr_stub(&params, &[], &mut vs, &Expr::Ident("n".into())).unwrap();
        assert_eq!(inst[0].opcode, OpCode::LDARG0);
    }

    #[test]
    fn expr_binary_add_and_compare() {
        let mut vs = HashMap::new();
        let expr = Expr::Binary {
            op: BinaryOp::Add,
            left: Box::new(Expr::Literal(Literal::Int("1".into()))),
            right: Box::new(Expr::Literal(Literal::Int("2".into()))),
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        assert!(inst.iter().any(|i| i.opcode == OpCode::PUSH1));
        assert!(inst.iter().any(|i| i.opcode == OpCode::PUSH2));
        assert!(inst.iter().any(|i| i.opcode == OpCode::ADD));

        let expr = Expr::Binary {
            op: BinaryOp::Eq,
            left: Box::new(Expr::Literal(Literal::Bool(true))),
            right: Box::new(Expr::Literal(Literal::Bool(false))),
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        assert!(inst.iter().any(|i| i.opcode == OpCode::EQUAL));
    }

    #[test]
    fn expr_unary_not() {
        let mut vs = HashMap::new();
        let expr = Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(Expr::Literal(Literal::Bool(false))),
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        assert_eq!(inst.last().unwrap().opcode, OpCode::NOT);
    }

    #[test]
    fn expr_cast_int_to_bool_emits_convert() {
        let mut vs = HashMap::new();
        let expr = Expr::Cast {
            expr: Box::new(Expr::Literal(Literal::Int("0".into()))),
            ty: Type::Bool,
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        let conv = inst.iter().find(|i| i.opcode == OpCode::CONVERT).unwrap();
        assert_eq!(conv.operands, vec![StackItemType::Boolean as u8]);
    }

    #[test]
    fn expr_array_and_map_pack() {
        let mut vs = HashMap::new();
        let expr = Expr::ArrayLit {
            ty: Type::Array(Box::new(Type::Int)),
            elements: vec![
                Expr::Literal(Literal::Int("1".into())),
                Expr::Literal(Literal::Int("2".into())),
            ],
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        assert!(inst.iter().any(|i| i.opcode == OpCode::PACK));

        let expr = Expr::MapLit {
            ty: Type::Map {
                key: Box::new(Type::String),
                value: Box::new(Type::Int),
            },
            pairs: vec![(
                Expr::Literal(Literal::String("k".into())),
                Expr::Literal(Literal::Int("1".into())),
            )],
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        assert!(inst.iter().any(|i| i.opcode == OpCode::PACKMAP));
    }

    #[test]
    fn expr_call_min_max_assert() {
        let mut vs = HashMap::new();
        let expr = Expr::Call {
            callee: Box::new(Expr::Ident("min".into())),
            args: vec![
                Expr::Literal(Literal::Int("3".into())),
                Expr::Literal(Literal::Int("5".into())),
            ],
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        assert_eq!(inst.last().unwrap().opcode, OpCode::MIN);

        let expr = Expr::Call {
            callee: Box::new(Expr::Ident("assert".into())),
            args: vec![
                Expr::Literal(Literal::Bool(true)),
                Expr::Literal(Literal::String("ok".into())),
            ],
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        assert_eq!(inst.last().unwrap().opcode, OpCode::ASSERTMSG);
    }

    #[test]
    fn expr_runtime_log_syscall() {
        let mut vs = HashMap::new();
        let expr = Expr::Call {
            callee: Box::new(Expr::Member {
                base: Box::new(Expr::Ident("runtime".into())),
                field: "log".into(),
            }),
            args: vec![Expr::Literal(Literal::String("m".into()))],
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        let sc = inst.iter().find(|i| i.opcode == OpCode::SYSCALL).unwrap();
        assert_eq!(
            sc.operands,
            Syscall::RUNTIME_LOG.token().to_le_bytes().to_vec()
        );
    }

    #[test]
    fn expr_runtime_get_network_syscall() {
        let mut vs = HashMap::new();
        let expr = Expr::Call {
            callee: Box::new(Expr::Member {
                base: Box::new(Expr::Ident("runtime".into())),
                field: "getNetwork".into(),
            }),
            args: vec![],
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        let sc = inst.iter().find(|i| i.opcode == OpCode::SYSCALL).unwrap();
        assert_eq!(
            sc.operands,
            Syscall::RUNTIME_GET_NETWORK.token().to_le_bytes().to_vec()
        );
    }

    #[test]
    fn expr_member_pickitem_with_struct_meta() {
        let structs = vec![StructDecl {
            name: "Point".into(),
            fields: vec![
                StructField {
                    ty: Type::Int,
                    name: "x".into(),
                    init: None,
                },
                StructField {
                    ty: Type::Int,
                    name: "y".into(),
                    init: None,
                },
            ],
            methods: vec![],
        }];
        let mut vs = HashMap::new();
        vs.insert("p".into(), "Point".into());
        let params = vec![Param {
            ty: Type::Named("Point".into()),
            name: "p".into(),
        }];
        let expr = Expr::Member {
            base: Box::new(Expr::Ident("p".into())),
            field: "y".into(),
        };
        let inst = compile_expr_stub(&params, &structs, &mut vs, &expr).unwrap();
        assert_eq!(inst[0].opcode, OpCode::LDARG0);
        assert!(inst.iter().any(|i| i.opcode == OpCode::PICKITEM));
        let push_idx = inst.iter().position(|i| i.opcode == OpCode::PUSH1);
        assert!(push_idx.is_some(), "field y is index 1");
    }

    #[test]
    fn expr_struct_literal_pack() {
        let structs = vec![StructDecl {
            name: "S".into(),
            fields: vec![StructField {
                ty: Type::Int,
                name: "a".into(),
                init: None,
            }],
            methods: vec![],
        }];
        let mut vs = HashMap::new();
        let expr = Expr::StructLit {
            name: "S".into(),
            fields: vec![("a".into(), Expr::Literal(Literal::Int("9".into())))],
        };
        let inst = compile_expr_stub(&[], &structs, &mut vs, &expr).unwrap();
        assert!(inst.iter().any(|i| i.opcode == OpCode::PACK));
    }

    #[test]
    fn expr_paren_passthrough() {
        let mut vs = HashMap::new();
        let expr = Expr::Paren(Box::new(Expr::Literal(Literal::Bool(true))));
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        assert_eq!(inst[0].opcode, OpCode::PUSHT);
    }

    #[test]
    fn expr_short_circuit_and_or_shape() {
        let mut vs = HashMap::new();
        let expr = Expr::Binary {
            op: BinaryOp::And,
            left: Box::new(Expr::Literal(Literal::Bool(false))),
            right: Box::new(Expr::Literal(Literal::Bool(true))),
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        assert!(inst.iter().any(|i| i.opcode == OpCode::JMPIFNOT_L));
        assert!(inst.iter().any(|i| i.opcode == OpCode::JMP_L));

        let expr = Expr::Binary {
            op: BinaryOp::Or,
            left: Box::new(Expr::Literal(Literal::Bool(true))),
            right: Box::new(Expr::Literal(Literal::Bool(false))),
        };
        let inst = compile_expr_stub(&[], &[], &mut vs, &expr).unwrap();
        assert!(inst.iter().any(|i| i.opcode == OpCode::JMPIF_L));
    }
}
