use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ir::lower::env::Env;
use crate::ir::lower::helpers::*;
use crate::ir::*;
use crate::syntax::ast::*;

pub struct Builder<'a> {
    pub blocks: BTreeMap<BlockId, BasicBlock>,
    pub current_block: BlockId,
    pub next_block: usize,
    pub next_value: usize,
    pub tmp_counter: usize,
    pub structs: &'a [StructDecl],
    pub contract_fields: Option<&'a [ContractField]>,
    pub package_fn_arity: &'a HashMap<String, usize>,
}

impl<'a> Builder<'a> {
    pub fn fresh_tmp(&mut self, prefix: &str) -> String {
        let n = self.tmp_counter;
        self.tmp_counter += 1;
        format!("__ir_{prefix}_{n}")
    }

    pub fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.blocks.insert(
            id,
            BasicBlock {
                params: Vec::new(),
                instrs: Vec::new(),
                term: Terminator::Unset,
            },
        );
        id
    }

    pub fn new_value(&mut self) -> ValueId {
        let v = ValueId(self.next_value);
        self.next_value += 1;
        v
    }

    pub fn emit(&mut self, out: ValueId, instr: Instr) {
        self.blocks
            .get_mut(&self.current_block)
            .unwrap()
            .instrs
            .push((out, instr));
    }

    pub fn set_term(&mut self, bb: BlockId, terminator: Terminator) {
        self.blocks.get_mut(&bb).unwrap().term = terminator;
    }

    pub fn finish(self, entry: BlockId) -> FunctionIr {
        FunctionIr {
            entry,
            blocks: self.blocks,
            value_count: self.next_value,
        }
    }

    pub fn lower_block(
        &mut self,
        block: &Block,
        env: &mut Env,
        return_ty: &Type,
    ) -> Result<(), LowerError> {
        for stmt in &block.stmts {
            self.lower_stmt(stmt, env, return_ty)?;
            if matches!(stmt, Stmt::Return(_)) {
                break;
            }
        }
        Ok(())
    }

    pub fn lower_if(
        &mut self,
        cond: &Expr,
        then_block: &Block,
        else_block: Option<&Block>,
        env: &mut Env,
        return_ty: &Type,
    ) -> Result<(), LowerError> {
        let header_bb = self.current_block;
        let cond_v = self.lower_expr(cond, env)?;

        let then_bb = self.new_block();
        let else_bb = self.new_block();
        let join_bb = self.new_block();

        let env_in = env.clone();

        self.current_block = then_bb;
        let mut env_then = env_in.clone();
        self.lower_block(then_block, &mut env_then, return_ty)?;
        if !matches!(self.blocks[&then_bb].term, Terminator::Return(_)) {
            self.set_term(
                then_bb,
                Terminator::Jump {
                    target: join_bb,
                    args: Vec::new(),
                },
            );
        }

        self.current_block = else_bb;
        let mut env_else = env_in.clone();
        if let Some(eb) = else_block {
            self.lower_block(eb, &mut env_else, return_ty)?;
        }
        if !matches!(self.blocks[&else_bb].term, Terminator::Return(_)) {
            self.set_term(
                else_bb,
                Terminator::Jump {
                    target: join_bb,
                    args: Vec::new(),
                },
            );
        }

        let mut join_param_names: Vec<String> = Vec::new();
        let mut then_args: Vec<ValueRef> = Vec::new();
        let mut else_args: Vec<ValueRef> = Vec::new();

        for name in env_in.declared.iter() {
            let then_value = env_then.get(name).or_else(|| env_in.get(name));
            let else_value = env_else.get(name).or_else(|| env_in.get(name));
            let (Some(then_value), Some(else_value)) = (then_value, else_value) else {
                continue;
            };
            if then_value != else_value {
                join_param_names.push(name.clone());
                then_args.push(then_value);
                else_args.push(else_value);
            }
        }

        {
            let join = self.blocks.get_mut(&join_bb).unwrap();
            for name in &join_param_names {
                join.params.push(BlockParam {
                    name: name.clone(),
                    ty: PrimTy::Any,
                });
            }
        }

        if let Terminator::Jump { args, .. } = &mut self.blocks.get_mut(&then_bb).unwrap().term {
            *args = then_args;
        }
        if let Terminator::Jump { args, .. } = &mut self.blocks.get_mut(&else_bb).unwrap().term {
            *args = else_args;
        }

        self.set_term(
            header_bb,
            Terminator::Branch {
                cond: cond_v,
                then_bb,
                then_args: self.blocks[&then_bb]
                    .term
                    .clone()
                    .into_jump_args()
                    .unwrap_or_default(),
                else_bb,
                else_args: self.blocks[&else_bb]
                    .term
                    .clone()
                    .into_jump_args()
                    .unwrap_or_default(),
            },
        );

        let mut new_env = env_in.clone();
        for (index, name) in join_param_names.into_iter().enumerate() {
            new_env.set(&name, ValueRef::Param(ParamId(index)));
        }

        *env = new_env;
        self.current_block = join_bb;
        Ok(())
    }

    pub fn lower_while(
        &mut self,
        cond: &Expr,
        body: &Block,
        env: &mut Env,
        return_ty: &Type,
    ) -> Result<(), LowerError> {
        let preheader = self.current_block;
        let loop_header = self.new_block();
        let body_bb = self.new_block();
        let exit_bb = self.new_block();

        let mut assigned: HashSet<String> = HashSet::new();
        collect_assigned_vars_in_block(body, &mut assigned);

        let mut loop_vars: Vec<String> = Vec::new();
        for v in assigned {
            if env.declared.contains(&v) {
                loop_vars.push(v);
            }
        }
        loop_vars.sort();

        {
            let hdr = self.blocks.get_mut(&loop_header).unwrap();
            for v in &loop_vars {
                hdr.params.push(BlockParam {
                    name: v.clone(),
                    ty: PrimTy::Any,
                });
            }
        }

        let mut init_args = Vec::new();
        for v in &loop_vars {
            let vv = env
                .get(v)
                .ok_or_else(|| err(format!("undefined loop var `{v}`")))?;
            init_args.push(vv);
        }
        self.set_term(
            preheader,
            Terminator::Jump {
                target: loop_header,
                args: init_args,
            },
        );

        let env_in = env.clone();
        let mut env_hdr = env_in.clone();
        for (index, var) in loop_vars.iter().enumerate() {
            env_hdr.set(var, ValueRef::Param(ParamId(index)));
        }

        self.current_block = loop_header;
        let cond_v = self.lower_expr(cond, &mut env_hdr)?;
        self.set_term(
            loop_header,
            Terminator::Branch {
                cond: cond_v,
                then_bb: body_bb,
                then_args: Vec::new(),
                else_bb: exit_bb,
                else_args: Vec::new(),
            },
        );

        self.current_block = body_bb;
        let mut env_body = env_hdr.clone();
        self.lower_block(body, &mut env_body, return_ty)?;
        if !matches!(self.blocks[&body_bb].term, Terminator::Return(_)) {
            let mut back_args = Vec::new();
            for v in &loop_vars {
                let vv = env_body.get(v).or_else(|| env_hdr.get(v)).unwrap();
                back_args.push(vv);
            }
            self.set_term(
                body_bb,
                Terminator::Jump {
                    target: loop_header,
                    args: back_args,
                },
            );
        }

        *env = env_hdr;
        self.current_block = exit_bb;
        Ok(())
    }

    pub fn contract_field_by_name(&self, name: &str) -> Option<&'a ContractField> {
        self.contract_fields?.iter().find(|f| f.name == name)
    }

    pub fn contract_self_map_types(&self, base: &Expr) -> Option<(String, Type, Type)> {
        let Expr::Member { base: inner, field } = base else {
            return None;
        };
        if !matches!(inner.as_ref(), Expr::Self_) {
            return None;
        }
        let cf = self.contract_field_by_name(field)?;
        let Type::Map { key, value } = &cf.ty else {
            return None;
        };
        Some((
            cf.name.clone(),
            (*key.as_ref()).clone(),
            (*value.as_ref()).clone(),
        ))
    }

    pub fn lower_assign(
        &mut self,
        target: &Expr,
        op: AssignOp,
        value: &Expr,
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        // Keep logic in one place: copied from the historical monolithic `lower.rs`.
        match op {
            AssignOp::Assign => match target {
                Expr::Ident(name) => {
                    let rhs = self.lower_expr(value, env)?;
                    let out = self.new_value();
                    self.emit(out, Instr::Copy(rhs));
                    env.set(name, ValueRef::Value(out));
                    Ok(ValueRef::Value(out))
                }
                Expr::Member { base, field } => self.lower_member_assign(base, field, value, env),
                Expr::Index { base, index } => self.lower_index_assign(base, index, value, env),
                _ => Err(err("IR lowering: assignment target not supported")),
            },
            _ => {
                let bin_op = op
                    .to_binary_op()
                    .ok_or_else(|| err("IR lowering: bad assign op"))?;

                match target {
                    Expr::Ident(name) => self.lower_compound_assign_ident(name, bin_op, value, env),
                    Expr::Index { base, index } => {
                        self.lower_compound_assign_index(base, index, op, value, env)
                    }
                    Expr::Member { base, field } => {
                        self.lower_compound_assign_member(base, field, op, value, env)
                    }
                    _ => Err(err("IR lowering: compound assignment target not supported")),
                }
            }
        }
    }

    fn lower_member_assign(
        &mut self,
        base: &Expr,
        field: &str,
        value: &Expr,
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        if matches!(base, Expr::Self_) {
            if let Some(cf) = self.contract_field_by_name(field) {
                let ty = cf.ty.clone();
                if ty.is_map() {
                    return Err(err("cannot assign to a contract map field without `[key]`"));
                }
                let rhs = self.lower_expr(value, env)?;
                let _sid = self.new_value();
                self.emit(
                    _sid,
                    Instr::ContractStoragePut {
                        field: field.into(),
                        value_ty: ty,
                        value: rhs,
                    },
                );
                return Ok(rhs);
            }
        }
        let rhs = self.lower_expr(value, env)?;
        let (base_ref, base_name) = match base {
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
            _ => {
                return Err(err(
                    "IR lowering: assignment member base must be identifier or self",
                ));
            }
        };
        let struct_name = env
            .get_struct_var(base_name)
            .ok_or_else(|| err("IR lowering: member assign needs a struct-typed variable"))?;
        let index = field_index_of(self.structs, struct_name, field)?;
        let out = self.new_value();
        self.emit(
            out,
            Instr::StructFieldSet {
                base: base_ref,
                index,
                value: rhs,
            },
        );
        Ok(ValueRef::Value(out))
    }

    fn lower_index_assign(
        &mut self,
        base: &Expr,
        index: &Expr,
        value: &Expr,
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        if let Some((map_name, key_ty, val_ty)) = self.contract_self_map_types(base) {
            let key = self.lower_expr(index, env)?;
            let value = self.lower_expr(value, env)?;
            let _sid = self.new_value();
            self.emit(
                _sid,
                Instr::ContractMapStoragePut {
                    field: map_name,
                    key_ty,
                    val_ty,
                    key,
                    value,
                },
            );
            return Ok(value);
        }
        let base_value = self.lower_expr(base, env)?;
        let index_value = self.lower_expr(index, env)?;
        let value = self.lower_expr(value, env)?;
        let out = self.new_value();
        self.emit(
            out,
            Instr::IndexSet {
                base: base_value,
                index: index_value,
                value,
            },
        );
        Ok(ValueRef::Value(out))
    }

    fn lower_compound_assign_ident(
        &mut self,
        name: &str,
        op: BinaryOp,
        value: &Expr,
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        let current_value = env
            .get(name)
            .ok_or_else(|| err(format!("undefined variable `{name}`")))?;
        let rhs = self.lower_expr(value, env)?;
        let new_value_out = self.new_value();
        self.emit(
            new_value_out,
            Instr::Binary {
                op,
                left: current_value,
                right: rhs,
            },
        );
        // No `Copy`: a second `ValueId` would often get its own spill slot and duplicate `STLOC`s.
        env.set(name, ValueRef::Value(new_value_out));
        Ok(ValueRef::Value(new_value_out))
    }

    fn lower_compound_assign_index(
        &mut self,
        base: &Expr,
        index: &Expr,
        op: AssignOp,
        value: &Expr,
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        if let Some((map_name, key_ty, val_ty)) = self.contract_self_map_types(base) {
            let key = self.lower_expr(index, env)?;
            let value = self.lower_expr(value, env)?;
            let out = self.new_value();
            self.emit(
                out,
                Instr::ContractMapStorageCompound {
                    field: map_name,
                    key_ty,
                    val_ty,
                    key,
                    value,
                    op,
                },
            );
            return Ok(ValueRef::Value(out));
        }

        let base_value = self.lower_expr(base, env)?;
        let index_value = self.lower_expr(index, env)?;
        let current_value_out = self.new_value();
        self.emit(
            current_value_out,
            Instr::IndexGet {
                base: base_value,
                index: index_value,
            },
        );
        let rhs = self.lower_expr(value, env)?;
        let new_value_out = self.new_value();
        self.emit(
            new_value_out,
            Instr::Binary {
                op: op
                    .to_binary_op()
                    .ok_or_else(|| err("IR lowering: bad compound-assign op"))?,
                left: ValueRef::Value(current_value_out),
                right: rhs,
            },
        );
        let out = self.new_value();
        self.emit(
            out,
            Instr::IndexSet {
                base: base_value,
                index: index_value,
                value: ValueRef::Value(new_value_out),
            },
        );
        Ok(ValueRef::Value(out))
    }

    fn lower_compound_assign_member(
        &mut self,
        base: &Expr,
        field: &str,
        op: AssignOp,
        value: &Expr,
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        let bin_op = op
            .to_binary_op()
            .ok_or_else(|| err("IR lowering: bad compound-assign op"))?;
        if matches!(base, Expr::Self_) {
            if let Some(cf) = self.contract_field_by_name(field) {
                let ty = cf.ty.clone();
                if ty.is_map() {
                    return Err(err(
                        "compound assignment for contract map field needs `[key]`",
                    ));
                }
                if ty.is_array() {
                    return Err(err("contract cannot have array fields"));
                }
                let current_value = self.new_value();
                self.emit(
                    current_value,
                    Instr::ContractStorageGet {
                        field: field.into(),
                        value_ty: ty.clone(),
                    },
                );
                let rhs = self.lower_expr(value, env)?;
                let new_value = self.new_value();
                self.emit(
                    new_value,
                    Instr::Binary {
                        op: bin_op,
                        left: ValueRef::Value(current_value),
                        right: rhs,
                    },
                );
                let _sid = self.new_value();
                self.emit(
                    _sid,
                    Instr::ContractStoragePut {
                        field: field.into(),
                        value_ty: ty,
                        value: ValueRef::Value(new_value),
                    },
                );
                return Ok(ValueRef::Value(new_value));
            }
        }

        let (base_ref, base_name) = match base {
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
            _ => {
                return Err(err(
                    "IR lowering: compound member base must be identifier or self",
                ));
            }
        };
        let struct_name = env.get_struct_var(base_name).ok_or_else(|| {
            err("IR lowering: compound member assign needs a struct-typed variable")
        })?;
        let field_index = field_index_of(self.structs, struct_name, field)?;

        let current_value_out = self.new_value();
        self.emit(
            current_value_out,
            Instr::StructFieldGet {
                base: base_ref,
                index: field_index,
            },
        );
        let rhs = self.lower_expr(value, env)?;
        let new_value_out = self.new_value();
        self.emit(
            new_value_out,
            Instr::Binary {
                op: bin_op,
                left: ValueRef::Value(current_value_out),
                right: rhs,
            },
        );
        let out = self.new_value();
        self.emit(
            out,
            Instr::StructFieldSet {
                base: base_ref,
                index: field_index,
                value: ValueRef::Value(new_value_out),
            },
        );
        Ok(ValueRef::Value(out))
    }

    pub fn lower_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        if let Expr::Member { base, field } = callee {
            if let Expr::Ident(pkg) = base.as_ref() {
                if pkg == "runtime" {
                    return self.lower_runtime_call(field, args, env);
                }
            }

            if let Some(value) = self.lower_builtin_method_call(base, field, args, env)? {
                return Ok(value);
            }

            if let Expr::Ident(recv) = base.as_ref() {
                if let Some(struct_name) = env.get_struct_var(recv) {
                    let struct_name = struct_name.to_string();
                    let mut values = Vec::new();
                    for arg in args {
                        values.push(self.lower_expr(arg, env)?);
                    }
                    let receiver_ref = env
                        .get(recv)
                        .ok_or_else(|| err(format!("undefined variable `{recv}`")))?;
                    let out = self.new_value();
                    self.emit(
                        out,
                        Instr::StructCall {
                            struct_name,
                            method: field.clone(),
                            recv: receiver_ref,
                            args: values,
                        },
                    );
                    return Ok(ValueRef::Value(out));
                }
            }
        }

        if let Expr::Ident(name) = callee {
            if let Some(value) = self.lower_builtin_call(name, args, env)? {
                return Ok(value);
            }
            if let Some(&arity) = self.package_fn_arity.get(name) {
                if args.len() == arity {
                    let mut values = Vec::new();
                    for arg in args {
                        values.push(self.lower_expr(arg, env)?);
                    }
                    let out = self.new_value();
                    self.emit(
                        out,
                        Instr::PackageCall {
                            name: name.clone(),
                            args: values,
                        },
                    );
                    return Ok(ValueRef::Value(out));
                }
            }
        }

        Err(err("IR lowering: call not supported yet"))
    }
}
