//! Lower neo-lang AST to SSA-form IR (block-parameter SSA).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ir::*;
use crate::syntax::ast::*;

#[derive(Debug, thiserror::Error)]
pub enum LowerError {
    #[error("lower-error: {0}")]
    Message(String),
}

fn err(s: impl std::fmt::Display) -> LowerError {
    LowerError::Message(s.to_string())
}

#[derive(Clone, Debug)]
struct Env {
    locals: HashMap<String, ValueRef>,
    declared: HashSet<String>,
    struct_vars: HashMap<String, String>,
}

impl Env {
    fn new() -> Self {
        Self {
            locals: HashMap::new(),
            declared: HashSet::new(),
            struct_vars: HashMap::new(),
        }
    }

    fn get(&self, name: &str) -> Option<ValueRef> {
        self.locals.get(name).copied()
    }

    fn set(&mut self, name: &str, v: ValueRef) {
        self.locals.insert(name.to_string(), v);
        self.declared.insert(name.to_string());
    }

    fn set_struct_var(&mut self, name: &str, struct_name: &str) {
        self.struct_vars
            .insert(name.to_string(), struct_name.to_string());
    }

    fn get_struct_var(&self, name: &str) -> Option<&str> {
        self.struct_vars.get(name).map(|s| s.as_str())
    }
}

pub fn lower_function_to_ir(
    func: &FunctionDecl,
    structs: &[StructDecl],
    contract_fields: Option<&[ContractField]>,
    package_fn_arity: &std::collections::HashMap<String, usize>,
) -> Result<FunctionIr, LowerError> {
    let mut b = Builder {
        blocks: BTreeMap::new(),
        current_block: BlockId(0),
        next_block: 0,
        next_value: 0,
        structs,
        contract_fields,
        package_fn_arity,
    };
    let entry = b.new_block();
    b.current_block = entry;

    let mut env = Env::new();
    // Entry block parameters represent function arguments (SSA values originating from VM args).
    {
        let bb = b.blocks.get_mut(&entry).unwrap();
        for p in &func.params {
            bb.params.push(BlockParam {
                name: p.name.clone(),
                ty: PrimTy::Any,
            });
        }
    }
    // Copy entry params into SSA values so they are usable in all blocks without threading params everywhere.
    for (index, param) in func.params.iter().enumerate() {
        let pid = ValueRef::Param(ParamId(index));
        let out = b.new_value();
        b.emit(out, Instr::Copy(pid));
        env.set(&param.name, ValueRef::Value(out));
        if let Type::Named(struct_name) = &param.ty {
            env.set_struct_var(&param.name, struct_name.as_str());
        }
    }

    b.lower_block(&func.body, &mut env, &func.return_ty)?;
    Ok(b.finish(entry))
}

struct Builder<'a> {
    blocks: BTreeMap<BlockId, BasicBlock>,
    current_block: BlockId,
    next_block: usize,
    next_value: usize,
    structs: &'a [StructDecl],
    contract_fields: Option<&'a [ContractField]>,
    package_fn_arity: &'a HashMap<String, usize>,
}

impl<'a> Builder<'a> {
    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.blocks.insert(
            id,
            BasicBlock {
                params: Vec::new(),
                instrs: Vec::new(),
                term: Terminator::Return(None),
            },
        );
        id
    }

    fn new_value(&mut self) -> ValueId {
        let v = ValueId(self.next_value);
        self.next_value += 1;
        v
    }

    fn emit(&mut self, out: ValueId, instr: Instr) {
        self.blocks
            .get_mut(&self.current_block)
            .unwrap()
            .instrs
            .push((out, instr));
    }

    fn set_term(&mut self, bb: BlockId, terminator: Terminator) {
        self.blocks.get_mut(&bb).unwrap().term = terminator;
    }

    fn finish(self, entry: BlockId) -> FunctionIr {
        FunctionIr {
            entry,
            blocks: self.blocks,
            value_count: self.next_value,
        }
    }

    fn lower_block(
        &mut self,
        block: &Block,
        env: &mut Env,
        return_ty: &Type,
    ) -> Result<(), LowerError> {
        for s in &block.stmts {
            self.lower_stmt(s, env, return_ty)?;
            // Only stop on an explicit `return` statement we just lowered. The default terminator
            // for a new block is `Return(None)` as a placeholder.
            if matches!(s, Stmt::Return(_)) {
                break;
            }
        }
        Ok(())
    }

    fn lower_stmt(
        &mut self,
        stmt: &Stmt,
        env: &mut Env,
        return_ty: &Type,
    ) -> Result<(), LowerError> {
        match stmt {
            Stmt::Var { name, init } => {
                let value = if let Some(expr) = init {
                    self.lower_expr(expr, env)?
                } else {
                    let out = self.new_value();
                    self.emit(out, Instr::Const(Literal::Null));
                    ValueRef::Value(out)
                };
                env.set(name, value);
                if let Some(Expr::StructLit {
                    name: struct_name, ..
                }) = init.as_ref()
                {
                    env.set_struct_var(name, struct_name);
                }
                Ok(())
            }
            Stmt::Expr(expr) => {
                let _ = self.lower_expr(expr, env)?;
                Ok(())
            }
            Stmt::Return(opt) => {
                let return_value = match opt {
                    Some(expr) => Some(self.lower_expr(expr, env)?),
                    None => None,
                };
                let bb = self.current_block;
                self.set_term(bb, Terminator::Return(return_value));
                let _ = return_ty;
                Ok(())
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => self.lower_if(cond, then_block, else_block.as_ref(), env, return_ty),
            Stmt::While { cond, body } => self.lower_while(cond, body, env, return_ty),
            Stmt::Block(block) => self.lower_block(block, env, return_ty),
            Stmt::Emit { name, args } => {
                let mut values = Vec::new();
                for arg in args {
                    values.push(self.lower_expr(arg, env)?);
                }
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::Emit {
                        name: name.clone(),
                        args: values,
                    },
                );
                Ok(())
            }
            // First-phase scope: keep it small.
            _ => Err(err(
                "IR lowering not implemented for this statement kind yet",
            )),
        }
    }

    fn lower_if(
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

        // Snapshot environment entering branches.
        let env_in = env.clone();

        // then
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

        // else
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

        // join params: only for vars that differ.
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

        // install join params
        {
            let join = self.blocks.get_mut(&join_bb).unwrap();
            for name in &join_param_names {
                join.params.push(BlockParam {
                    name: name.clone(),
                    ty: PrimTy::Any,
                });
            }
        }

        // patch jumps to carry args
        if let Terminator::Jump { args, .. } = &mut self.blocks.get_mut(&then_bb).unwrap().term {
            *args = then_args;
        }
        if let Terminator::Jump { args, .. } = &mut self.blocks.get_mut(&else_bb).unwrap().term {
            *args = else_args;
        }

        // header branch
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

        // update env to refer to join params
        let mut new_env = env_in.clone();
        for (index, name) in join_param_names.into_iter().enumerate() {
            new_env.set(&name, ValueRef::Param(ParamId(index)));
        }

        *env = new_env;
        self.current_block = join_bb;
        Ok(())
    }

    fn lower_while(
        &mut self,
        cond: &Expr,
        body: &Block,
        env: &mut Env,
        return_ty: &Type,
    ) -> Result<(), LowerError> {
        // CFG:
        // header_bb (current) -> loop_header -> (body_bb, exit_bb)
        // body_bb -> loop_header
        let preheader = self.current_block;
        let loop_header = self.new_block();
        let body_bb = self.new_block();
        let exit_bb = self.new_block();

        // Pick loop-carried vars: vars assigned in body (simple scan).
        let mut assigned: HashSet<String> = HashSet::new();
        collect_assigned_vars_in_block(body, &mut assigned);

        // Add params to loop_header for loop-carried vars that exist at entry.
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

        // preheader jumps to loop_header with initial values
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

        // In loop_header, bind env vars to params.
        let env_in = env.clone();
        let mut env_hdr = env_in.clone();
        for (index, var) in loop_vars.iter().enumerate() {
            env_hdr.set(var, ValueRef::Param(ParamId(index)));
        }

        // lower condition + branch
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

        // lower body
        self.current_block = body_bb;
        let mut env_body = env_hdr.clone();
        self.lower_block(body, &mut env_body, return_ty)?;
        if !matches!(self.blocks[&body_bb].term, Terminator::Return(_)) {
            // backedge args are the updated versions for loop vars
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

        // after loop: env uses the params from loop_header (current iteration), conservatively.
        *env = env_hdr;
        self.current_block = exit_bb;
        Ok(())
    }

    fn contract_field_by_name(&self, name: &str) -> Option<&'a ContractField> {
        self.contract_fields?.iter().find(|f| f.name == name)
    }

    /// `self.map` as base of `[key]` for a contract storage map field.
    fn contract_self_map_types(&self, base: &Expr) -> Option<(String, Type, Type)> {
        let Expr::Member {
            base: inner,
            field: fname,
        } = base
        else {
            return None;
        };
        if !matches!(inner.as_ref(), Expr::Self_) {
            return None;
        }
        let cf = self.contract_field_by_name(fname)?;
        let Type::Map { key, value } = &cf.ty else {
            return None;
        };
        Some((
            cf.name.clone(),
            (*key.as_ref()).clone(),
            (*value.as_ref()).clone(),
        ))
    }

    fn lower_assign(
        &mut self,
        target: &Expr,
        op: AssignOp,
        value: &Expr,
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        match op {
            AssignOp::Assign => match target {
                Expr::Ident(name) => {
                    let rhs = self.lower_expr(value, env)?;
                    let out = self.new_value();
                    self.emit(out, Instr::Copy(rhs));
                    env.set(name, ValueRef::Value(out));
                    Ok(ValueRef::Value(out))
                }
                Expr::Member { base, field } => {
                    if matches!(base.as_ref(), Expr::Self_) {
                        if let Some(cf) = self.contract_field_by_name(field) {
                            let ty = cf.ty.clone();
                            if ty.is_map() {
                                return Err(err(
                                    "cannot assign to a contract map field without `[key]`",
                                ));
                            }
                            if ty.is_array() {
                                return Err(err(
                                    "contract array storage assignment is not implemented yet",
                                ));
                            }
                            let rhs = self.lower_expr(value, env)?;
                            let _sid = self.new_value();
                            self.emit(
                                _sid,
                                Instr::ContractStoragePut {
                                    field: field.clone(),
                                    value_ty: ty,
                                    value: rhs,
                                },
                            );
                            return Ok(rhs);
                        }
                    }
                    let rhs = self.lower_expr(value, env)?;
                    let (base_ref, base_name) = match base.as_ref() {
                        Expr::Ident(n) => (
                            env.get(n)
                                .ok_or_else(|| err(format!("undefined variable `{n}`")))?,
                            n.as_str(),
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
                    let struct_name = env.get_struct_var(base_name).ok_or_else(|| {
                        err("IR lowering: member assign needs a struct-typed variable")
                    })?;
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
                Expr::Index { base, index } => {
                    if let Some((map_name, key_ty, val_ty)) =
                        self.contract_self_map_types(base.as_ref())
                    {
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
                _ => Err(err("IR lowering: assignment target not supported")),
            },
            _ => {
                let bin_op = op
                    .to_binary_op()
                    .ok_or_else(|| err("IR lowering: bad assign op"))?;
                if let Expr::Index { base, index } = target {
                    if let Some((map_name, key_ty, val_ty)) =
                        self.contract_self_map_types(base.as_ref())
                    {
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
                }
                if let Expr::Member { base, field } = target {
                    if matches!(base.as_ref(), Expr::Self_) {
                        if let Some(cf) = self.contract_field_by_name(field) {
                            let ty = cf.ty.clone();
                            if ty.is_map() {
                                return Err(err(
                                    "compound assignment on contract map field needs `[key]`",
                                ));
                            }
                            if ty.is_array() {
                                return Err(err(
                                    "compound assignment on contract array storage not implemented",
                                ));
                            }
                            let current_value = self.new_value();
                            self.emit(
                                current_value,
                                Instr::ContractStorageGet {
                                    field: field.clone(),
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
                                    field: field.clone(),
                                    value_ty: ty,
                                    value: ValueRef::Value(new_value),
                                },
                            );
                            return Ok(ValueRef::Value(new_value));
                        }
                    }
                }
                Err(err(
                    "IR lowering: compound assignment is only implemented for contract storage",
                ))
            }
        }
    }

    fn lower_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        if let Expr::Member { base, field } = callee {
            if let Expr::Ident(pkg) = base.as_ref() {
                if pkg == "runtime" && field == "log" && args.len() == 1 {
                    let message = self.lower_expr(&args[0], env)?;
                    let out = self.new_value();
                    self.emit(out, Instr::RuntimeLog { message });
                    return Ok(ValueRef::Value(out));
                }
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
                        Instr::StructInstanceCall {
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
            if name == "abort" && args.len() == 1 {
                let message = self.lower_expr(&args[0], env)?;
                let out = self.new_value();
                self.emit(out, Instr::Abort { message });
                return Ok(ValueRef::Value(out));
            }
            if name == "min" && args.len() == 2 {
                let left = self.lower_expr(&args[0], env)?;
                let right = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(out, Instr::Min { left, right });
                return Ok(ValueRef::Value(out));
            }
            if name == "max" && args.len() == 2 {
                let left = self.lower_expr(&args[0], env)?;
                let right = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(out, Instr::Max { left, right });
                return Ok(ValueRef::Value(out));
            }
            if name == "assert" && args.len() == 2 {
                let cond = self.lower_expr(&args[0], env)?;
                let message = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(out, Instr::Assert { cond, message });
                return Ok(ValueRef::Value(out));
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

    fn lower_expr(&mut self, expr: &Expr, env: &mut Env) -> Result<ValueRef, LowerError> {
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
                if crate::codegen::expr::convert_operand_for_type(ty).is_none() {
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
                                "use `self.{field}[key]` to read contract map `{field}` entries",
                            )));
                        }
                        if ty.is_array() {
                            return Err(err(
                                "contract array storage field read is not implemented yet",
                            ));
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
                // `var.field` and `self.field` for struct-typed vars.
                let (base_ref, base_name) = match base.as_ref() {
                    Expr::Ident(n) => (
                        env.get(n)
                            .ok_or_else(|| err(format!("undefined variable `{n}`")))?,
                        n.as_str(),
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
                let index = field_index_of(self.structs, struct_name, field)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::StructFieldGet {
                        base: base_ref,
                        index,
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
            Expr::Assign { target, op, value } => {
                self.lower_assign(target.as_ref(), *op, value.as_ref(), env)
            }
            Expr::Call { callee, args } => self.lower_call(callee.as_ref(), args, env),
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

    fn lower_short_circuit(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        // Short-circuit lowering using CFG:
        // - For `a && b`: if a is false => result false; else evaluate b.
        // - For `a || b`: if a is true  => result true;  else evaluate b.
        //
        // For now, disallow assignments inside short-circuit operands (keep env unchanged across edges).
        if expr_contains_assign(left) || expr_contains_assign(right) {
            return Err(err(
                "IR lowering: assignments inside `&&`/`||` not supported yet",
            ));
        }

        let header_bb = self.current_block;
        let left_value = self.lower_expr(left, env)?;
        let eval_right_bb = self.new_block();
        let const_bb = self.new_block();
        let join_bb = self.new_block();
        // join has one param: the boolean result
        {
            let join = self.blocks.get_mut(&join_bb).unwrap();
            join.params.push(BlockParam {
                name: "_sc".into(),
                ty: PrimTy::Bool,
            });
        }

        // Header branch based on `a`.
        // For AND: true -> eval_right, false -> const(false)
        // For OR : true -> const(true),  false -> eval_right
        let (then_bb, else_bb) = match op {
            BinaryOp::And => (eval_right_bb, const_bb),
            BinaryOp::Or => (const_bb, eval_right_bb),
            _ => unreachable!(),
        };
        self.set_term(
            header_bb,
            Terminator::Branch {
                cond: left_value,
                then_bb,
                then_args: Vec::new(),
                else_bb,
                else_args: Vec::new(),
            },
        );

        // const block produces const boolean and jumps to join with it
        self.current_block = const_bb;
        let const_value = self.new_value();
        let lit = match op {
            BinaryOp::And => Literal::Bool(false),
            BinaryOp::Or => Literal::Bool(true),
            _ => unreachable!(),
        };
        self.emit(const_value, Instr::Const(lit));
        self.set_term(
            const_bb,
            Terminator::Jump {
                target: join_bb,
                args: vec![ValueRef::Value(const_value)],
            },
        );

        // eval-right block evaluates right and jumps to join
        self.current_block = eval_right_bb;
        let right_value = self.lower_expr(right, env)?;
        self.set_term(
            eval_right_bb,
            Terminator::Jump {
                target: join_bb,
                args: vec![right_value],
            },
        );

        // Patch branch args (single param) for both edges.
        let then_args_v = self.blocks[&then_bb]
            .term
            .clone()
            .into_jump_args()
            .unwrap_or_default();
        let else_args_v = self.blocks[&else_bb]
            .term
            .clone()
            .into_jump_args()
            .unwrap_or_default();
        if let Terminator::Branch {
            then_args,
            else_args,
            ..
        } = &mut self.blocks.get_mut(&header_bb).unwrap().term
        {
            *then_args = then_args_v;
            *else_args = else_args_v;
        }

        // Continue in join
        self.current_block = join_bb;
        Ok(ValueRef::Param(ParamId(0)))
    }
}

fn field_index_of(
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

trait JumpArgsExt {
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

fn collect_assigned_vars_in_block(block: &Block, out: &mut HashSet<String>) {
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

fn collect_assigned_vars_in_expr(expr: &Expr, out: &mut HashSet<String>) {
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

fn expr_contains_assign(expr: &Expr) -> bool {
    match expr {
        Expr::Assign { .. } => true,
        Expr::Binary { left, right, .. } => {
            expr_contains_assign(left) || expr_contains_assign(right)
        }
        Expr::Unary { expr, .. } => expr_contains_assign(expr),
        Expr::Call { callee, args } => {
            expr_contains_assign(callee) || args.iter().any(expr_contains_assign)
        }
        Expr::Member { base, .. } => expr_contains_assign(base),
        Expr::Index { base, index } => expr_contains_assign(base) || expr_contains_assign(index),
        Expr::Cast { expr, .. } => expr_contains_assign(expr),
        Expr::Paren(expr) => expr_contains_assign(expr),
        Expr::StructLit { fields, .. } => fields.iter().any(|(_, expr)| expr_contains_assign(expr)),
        Expr::MapLit { pairs, .. } => pairs
            .iter()
            .any(|(k, v)| expr_contains_assign(k) || expr_contains_assign(v)),
        Expr::ArrayLit { elements, .. } => elements.iter().any(expr_contains_assign),
        Expr::Literal(_) | Expr::Ident(_) | Expr::Self_ => false,
    }
}
