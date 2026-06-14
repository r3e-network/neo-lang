//! Emit NeoVM [`Instruction`]s for a `FunctionDecl` (see README.md and `target::`).
//!
//! Layout: `INITSLOT(locals, args)`, then body. Arguments use `LDARG*`, locals `LDLOC*`.
//! `RET` with no preceding push for `void` functions (no return value on stack).
//!
//! Locals/args: [`super::env`] (`VarEnv`). Expressions: [`super::expr`] (`ExprGen`).

use std::collections::HashMap;

use crate::codegen::context::FunctionCompileContext;
use crate::codegen::env::VarEnv;
use crate::codegen::expr::stack_effect::{expr_stmt_leaves_stack_value, CallStackEffectCtx};
use crate::codegen::expr::ExprGen;
use crate::codegen::CodegenError;
use crate::ir;
use crate::syntax::ast::*;
use crate::target::method_token::MethodTokenRegistry;
use crate::target::opcode::OpCode;
use crate::target::syscall::Syscall;
use crate::target::{Builder, Instruction};

pub struct FunctionCompiler<'a> {
    func: &'a FunctionDecl,
    ctx: &'a FunctionCompileContext<'a>,
    builder: Builder,
    env: VarEnv,

    /// `local_or_param_name` → neo-lang struct type name (for `s.field` → PICKITEM index).
    value_struct: HashMap<String, String>,
    initslot_instruction_index: usize,

    /// `(instruction_index, callee_link_symbol)` for [`OpCode::CALL_L`] placeholders
    pending_call_l: Vec<(usize, String)>,

    method_tokens: &'a mut MethodTokenRegistry,
}

pub struct CompliledFunction {
    pub instructions: Vec<Instruction>,

    // `(instruction_index, callee_link_symbol)` for [`OpCode::CALL_L`] placeholders
    pub call_patches: Vec<(usize, String)>,
}

impl<'a> FunctionCompiler<'a> {
    pub fn new(
        func: &'a FunctionDecl,
        ctx: &'a FunctionCompileContext<'a>,
        method_tokens: &'a mut MethodTokenRegistry,
    ) -> Result<Self, CodegenError> {
        let env = VarEnv::new(&func.params)?;
        let arg_count = func.params.len() as u8;
        let mut value_struct = HashMap::new();
        for p in &func.params {
            if let Type::Named(sn) = &p.ty {
                value_struct.insert(p.name.clone(), sn.clone());
            }
        }
        let mut builder = Builder::new();
        let initslot_instruction_index = builder.instruction_count();
        builder.emit_initslot(0, arg_count);
        Ok(Self {
            func,
            ctx,
            builder,
            env,
            value_struct,
            initslot_instruction_index,
            pending_call_l: Vec::new(),
            method_tokens,
        })
    }

    pub(crate) fn compile(mut self) -> Result<CompliledFunction, CodegenError> {
        self.env.enter_block();
        for stmt in &self.func.body.stmts {
            self.compile_stmt(stmt)?;
        }
        self.env.exit_block();

        if matches!(self.func.return_ty, Type::Void) {
            let ends_with_ret = self
                .builder
                .instructions()
                .last()
                .is_some_and(|instruction| instruction.opcode == OpCode::RET);
            if !ends_with_ret {
                self.builder.emit(OpCode::RET);
            }
        }

        let locals = self.env.local_count();
        self.builder
            .patch_initslot_local_count(self.initslot_instruction_index, locals);
        let pending = self.pending_call_l;
        Ok(CompliledFunction {
            instructions: self.builder.into_instructions(),
            call_patches: pending,
        })
    }

    fn expr_gen(&mut self) -> ExprGen<'a, '_> {
        ExprGen {
            builder: &mut self.builder,
            env: &mut self.env,
            structs: self.ctx.structs,
            value_struct: &mut self.value_struct,
            contract_fields: self.ctx.contract_fields,
            contract_name: self.ctx.contract_name,
            contract_fns: self.ctx.contract_fns,
            pending_call_l: &mut self.pending_call_l,
            package_fns: self.ctx.package_fns,
            method_tokens: self.method_tokens,
        }
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<(), CodegenError> {
        self.expr_gen().compile_expr(expr)
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), CodegenError> {
        match stmt {
            Stmt::Var { name, init } => {
                let slot = self.env.declare_local(name)?;
                if let Some(expr) = init {
                    if let Expr::StructLit { name: sn, .. } = expr {
                        self.value_struct.insert(name.clone(), sn.clone());
                    }
                    self.compile_expr(expr)?;
                } else {
                    self.builder.push_null();
                }
                self.builder.emit_stloc(slot);
            }
            Stmt::Expr(expr) => {
                self.compile_expr(expr)?;
                let stack_ctx = CallStackEffectCtx {
                    package_fns: self.ctx.package_fns,
                    contract_fns: self.ctx.contract_fns,
                };
                if expr_stmt_leaves_stack_value(expr, &stack_ctx) {
                    self.builder.emit(OpCode::DROP);
                }
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                self.compile_expr(cond)?;
                let jmp_no_then = self.builder.emit_jmpifnot_l_placeholder();
                self.compile_block(then_block)?;
                if let Some(else_b) = else_block {
                    let jmp_end = self.builder.emit_jmp_l_placeholder();
                    let else_start = self.builder.cursor();
                    self.builder
                        .patch_jmp_target_at_instruction(jmp_no_then, else_start);
                    self.compile_block(else_b)?;
                    let end = self.builder.cursor();
                    self.builder.patch_jmp_target_at_instruction(jmp_end, end);
                } else {
                    let end = self.builder.cursor();
                    self.builder
                        .patch_jmp_target_at_instruction(jmp_no_then, end);
                }
            }
            Stmt::While { cond, body } => {
                let loop_start = self.builder.cursor();
                self.compile_expr(cond)?;
                let jmp_out = self.builder.emit_jmpifnot_l_placeholder();
                self.compile_block(body)?;
                let jmp_pc = self.builder.cursor();
                let relative = i32::try_from(loop_start as i64 - jmp_pc as i64).map_err(|_| {
                    CodegenError::Unsupported("while loop backward jump offset overflow".into())
                })?;
                self.builder
                    .emit_with_operands(OpCode::JMP_L, &relative.to_le_bytes());
                let after = self.builder.cursor();
                self.builder.patch_jmp_target_at_instruction(jmp_out, after);
            }
            Stmt::Return(opt) => {
                if let Some(expr) = opt {
                    self.compile_expr(expr)?;
                }
                self.builder.emit(OpCode::RET);
            }
            Stmt::Block(block) => self.compile_block(block)?,
            Stmt::ForArray { item, iter, body } => {
                self.compile_stmt_for_array(item, iter, body)?;
            }
            Stmt::ForMap {
                key,
                value,
                map,
                body,
            } => {
                self.compile_stmt_for_map(key, value, map, body)?;
            }
            Stmt::Emit { name, args } => {
                // `Notify(eventName, state)`: same stack layout as calls — `| state | eventName |` (top = first param).
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.builder.push_int(
                    args.len().try_into().map_err(|_| {
                        CodegenError::Unsupported("emit: too many arguments".into())
                    })?,
                );
                self.builder.emit(OpCode::PACK);
                self.builder.push_data(name.as_bytes());
                self.builder.emit_syscall(Syscall::RUNTIME_NOTIFY);
            }
        }
        Ok(())
    }

    fn compile_block(&mut self, block: &Block) -> Result<(), CodegenError> {
        self.env.enter_block();
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        self.env.exit_block();
        Ok(())
    }

    fn compile_stmt_for_array(
        &mut self,
        item: &str,
        iter: &Expr,
        body: &Block,
    ) -> Result<(), CodegenError> {
        let array = self.env.alloc_slot()?;
        let index = self.env.alloc_slot()?;
        self.compile_expr(iter)?;
        self.builder.emit_stloc(array);
        self.builder.push_int(0);
        self.builder.emit_stloc(index);
        let loop_start = self.builder.cursor();
        self.builder.emit_ldloc(index);
        self.builder.emit_ldloc(array);
        self.builder.emit(OpCode::SIZE);
        self.builder.emit(OpCode::LT);
        let jmp_out = self.builder.emit_jmpifnot_l_placeholder();
        self.env.enter_block();
        let item_slot = self.env.declare_local(item)?;
        self.builder.emit_ldloc(array);
        self.builder.emit_ldloc(index);
        self.builder.emit(OpCode::PICKITEM);
        self.builder.emit_stloc(item_slot);
        for s in &body.stmts {
            self.compile_stmt(s)?;
        }
        self.env.exit_block();
        self.builder.emit_ldloc(index);
        self.builder.emit(OpCode::INC);
        self.builder.emit_stloc(index);
        let jmp_pc = self.builder.cursor();
        let relative = i32::try_from(loop_start as i64 - jmp_pc as i64)
            .map_err(|_| CodegenError::Unsupported("for-in-array backward jump overflow".into()))?;
        self.builder
            .emit_with_operands(OpCode::JMP_L, &relative.to_le_bytes());
        let after = self.builder.cursor();
        self.builder.patch_jmp_target_at_instruction(jmp_out, after);
        Ok(())
    }

    fn compile_stmt_for_map(
        &mut self,
        key: &str,
        value: &str,
        map: &Expr,
        body: &Block,
    ) -> Result<(), CodegenError> {
        let temp_map = self.env.alloc_slot()?;
        let keys = self.env.alloc_slot()?;
        let index = self.env.alloc_slot()?;
        self.compile_expr(map)?;
        self.builder.emit_stloc(temp_map);
        self.builder.emit_ldloc(temp_map);
        self.builder.emit(OpCode::KEYS);
        self.builder.emit_stloc(keys);
        self.builder.push_int(0);
        self.builder.emit_stloc(index);
        let loop_start = self.builder.cursor();
        self.builder.emit_ldloc(index);
        self.builder.emit_ldloc(keys);
        self.builder.emit(OpCode::SIZE);
        self.builder.emit(OpCode::LT);
        let jmp_out = self.builder.emit_jmpifnot_l_placeholder();
        self.env.enter_block();
        let key_slot = self.env.declare_local(key)?;
        let val_slot = self.env.declare_local(value)?;
        self.builder.emit_ldloc(keys);
        self.builder.emit_ldloc(index);
        self.builder.emit(OpCode::PICKITEM);
        self.builder.emit_stloc(key_slot);
        self.builder.emit_ldloc(temp_map);
        self.builder.emit_ldloc(key_slot);
        self.builder.emit(OpCode::PICKITEM);
        self.builder.emit_stloc(val_slot);
        for s in &body.stmts {
            self.compile_stmt(s)?;
        }
        self.env.exit_block();
        self.builder.emit_ldloc(index);
        self.builder.emit(OpCode::INC);
        self.builder.emit_stloc(index);
        let jmp_pc = self.builder.cursor();
        let relative = i32::try_from(loop_start as i64 - jmp_pc as i64)
            .map_err(|_| CodegenError::Unsupported("for-in-map backward jump overflow".into()))?;
        self.builder
            .emit_with_operands(OpCode::JMP_L, &relative.to_le_bytes());
        let after = self.builder.cursor();
        self.builder.patch_jmp_target_at_instruction(jmp_out, after);
        Ok(())
    }
}

/// Compile a standalone [`FunctionDecl`] to NeoVM instructions (no contract layout / static slots).
///
/// For contract methods, pass mutable storage fields so `self.name` / `self.map[key]` lower to
/// `System.Storage.Local.*` (see `codegen` module docs).
pub fn compile_function(
    func: &FunctionDecl,
    ctx: &FunctionCompileContext<'_>,
    method_tokens: &mut MethodTokenRegistry,
) -> Result<CompliledFunction, CodegenError> {
    // First-phase SSA IR pipeline: only for basic statements (var/assign/if/while/return) and
    // a limited expression subset. On unsupported constructs, fall back to the legacy AST codegen.
    if should_use_ir_pipeline(func) {
        if let Ok(mut fir) = ir::lower::lower_function_to_ir(func, ctx) {
            fir.optimize();
            if let Ok(compiled) =
                fir.compile_ir(func.params.len() as u8, &func.return_ty, method_tokens)
            {
                return Ok(compiled);
            }
        }
    }

    FunctionCompiler::new(func, ctx, method_tokens)?.compile()
}

fn should_use_ir_pipeline(func: &FunctionDecl) -> bool {
    fn expr_triggers_ir(expr: &Expr) -> bool {
        match expr {
            Expr::Assign { .. } => true,
            Expr::Member { .. } => true,
            Expr::Binary { left, right, .. } => expr_triggers_ir(left) || expr_triggers_ir(right),
            Expr::Unary { expr, .. } => expr_triggers_ir(expr),
            Expr::Call { callee, args } => {
                expr_triggers_ir(callee) || args.iter().any(expr_triggers_ir)
            }
            // Index reads are lowered to IR; keep pipeline even for `a[0]` (no nested IR triggers).
            Expr::Index { .. } => true,
            Expr::Cast { .. } => true,
            Expr::Paren(x) => expr_triggers_ir(x),
            Expr::StructLit { fields, .. } => fields.iter().any(|(_, expr)| expr_triggers_ir(expr)),
            Expr::MapLit { pairs, .. } => pairs
                .iter()
                .any(|(k, v)| expr_triggers_ir(k) || expr_triggers_ir(v)),
            Expr::ArrayLit { elements, .. } => elements.iter().any(expr_triggers_ir),
            Expr::Literal(_) | Expr::Ident(_) | Expr::Self_ => false,
        }
    }

    fn block_needs_ir(block: &Block) -> bool {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Var { .. } | Stmt::If { .. } | Stmt::While { .. } => return true,
                Stmt::Expr(expr) => {
                    if expr_triggers_ir(expr) {
                        return true;
                    }
                }
                Stmt::ForArray { .. } | Stmt::ForMap { .. } => {}
                Stmt::Emit { .. } => return true,
                Stmt::Return(return_expr) => {
                    if let Some(expr) = return_expr {
                        if expr_triggers_ir(expr) {
                            return true;
                        }
                    }
                }
                Stmt::Block(inner) => {
                    if block_needs_ir(inner) {
                        return true;
                    }
                }
            }
        }
        false
    }

    block_needs_ir(&func.body)
}

/// [`StructDecl`] method with implicit receiver: becomes `StructName::methodName(self, ...)` for codegen.
pub fn lower_struct_method(struct_name: &str, method: &FunctionDecl) -> FunctionDecl {
    let mut params = vec![Param {
        ty: Type::Named(struct_name.to_string()),
        name: "self".into(),
    }];
    params.extend(method.params.iter().cloned());
    FunctionDecl {
        attributes: method.attributes.clone(),
        return_ty: method.return_ty.clone(),
        name: format!("{struct_name}::{}", method.name),
        params,
        body: method.body.clone(),
    }
}
