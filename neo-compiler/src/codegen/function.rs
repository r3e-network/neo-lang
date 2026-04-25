//! Emit NeoVM [`Instruction`]s for a `FunctionDecl` (see README.md and `target::`).
//!
//! Layout: `INITSLOT(locals, args)`, then body. Arguments use `LDARG*`, locals `LDLOC*`.
//! `RET` expects one return value on the stack; `void` uses `PUSHNULL` first.
//!
//! Locals/args: [`super::env`] (`VarEnv`). Expressions: [`super::expr`] (`ExprGen`).

use std::collections::HashMap;

use crate::codegen::env::VarEnv;
use crate::codegen::expr::ExprGen;
use crate::codegen::CodegenError;
use crate::ir;
use crate::syntax::ast::*;
use crate::target::opcode::OpCode;
use crate::target::syscall::Syscall;
use crate::target::{Builder, Instruction};

pub struct FunctionCompiler<'a> {
    func: &'a FunctionDecl,
    structs: &'a [StructDecl],

    /// Cloned from the enclosing contract for methods; `None` for package functions.
    contract_fields: Option<&'a [ContractField]>,
    builder: Builder,
    env: VarEnv,

    /// `local_or_param_name` → neo-lang struct type name (for `s.field` → PICKITEM index).
    value_struct: HashMap<String, String>,
    initslot_instruction_index: usize,

    /// `(instruction_index, callee_link_symbol)` for [`OpCode::CALL_L`] placeholders
    pending_call_l: Vec<(usize, String)>,

    /// Same-file top-level functions: `name` → arity (see [`ExprGen::package_fn_arity`]).
    package_fn_arity: &'a HashMap<String, usize>,
}

pub struct CompliledFunction {
    pub instructions: Vec<Instruction>,

    // `(instruction_index, callee_link_symbol)` for [`OpCode::CALL_L`] placeholders
    pub call_patches: Vec<(usize, String)>,
}

impl<'a> FunctionCompiler<'a> {
    pub fn new(
        func: &'a FunctionDecl,
        structs: &'a [StructDecl],
        contract_fields: Option<&'a [ContractField]>,
        package_fn_arity: &'a HashMap<String, usize>,
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
            structs,
            contract_fields,
            builder,
            env,
            value_struct,
            initslot_instruction_index,
            pending_call_l: Vec::new(),
            package_fn_arity,
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
                self.builder.push_null();
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
            structs: self.structs,
            value_struct: &mut self.value_struct,
            contract_fields: self.contract_fields.as_deref(),
            pending_call_l: &mut self.pending_call_l,
            package_fn_arity: self.package_fn_arity,
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
                self.builder.emit(OpCode::DROP);
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
                } else {
                    self.builder.push_null();
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
    structs: &[StructDecl],
    contract_fields: Option<&[ContractField]>,
    package_fn_arity: &HashMap<String, usize>,
) -> Result<CompliledFunction, CodegenError> {
    // First-phase SSA IR pipeline: only for basic statements (var/assign/if/while/return) and
    // a limited expression subset. On unsupported constructs, fall back to the legacy AST codegen.
    if should_use_ir_pipeline(func) {
        if let Ok(mut fir) =
            ir::lower::lower_function_to_ir(func, structs, contract_fields, package_fn_arity)
        {
            fir.optimize();
            let compiled = fir.compile_ir(func.params.len() as u8)?;
            return Ok(compiled);
        }
    }

    FunctionCompiler::new(func, structs, contract_fields, package_fn_arity)?.compile()
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
pub fn lower_struct_method(struct_name: &str, m: &FunctionDecl) -> FunctionDecl {
    let mut params = vec![Param {
        ty: Type::Named(struct_name.to_string()),
        name: "self".into(),
    }];
    params.extend(m.params.iter().cloned());
    FunctionDecl {
        attributes: m.attributes.clone(),
        return_ty: m.return_ty.clone(),
        name: format!("{struct_name}::{}", m.name),
        params,
        body: m.body.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::ast::{Literal, Stmt, Type};
    use crate::syntax::parser::parse_source_file;
    use crate::target::syscall::Syscall;
    use crate::target::Instruction;

    fn empty_pkg() -> HashMap<String, usize> {
        HashMap::new()
    }

    fn simple_add() -> FunctionDecl {
        FunctionDecl {
            attributes: vec![],
            return_ty: Type::Int,
            name: "add".into(),
            params: vec![
                Param {
                    ty: Type::Int,
                    name: "a".into(),
                },
                Param {
                    ty: Type::Int,
                    name: "b".into(),
                },
            ],
            body: Block {
                stmts: vec![Stmt::Return(Some(Expr::Binary {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Ident("a".into())),
                    right: Box::new(Expr::Ident("b".into())),
                }))],
            },
        }
    }

    #[test]
    fn compile_add_returns_ldarg_add_ret() {
        let compiled = compile_function(&simple_add(), &[], None, &empty_pkg()).unwrap();
        assert!(compiled.call_patches.is_empty());
        assert!(matches!(
            compiled.instructions.as_slice(),
            [
                Instruction {
                    opcode: OpCode::INITSLOT,
                    operands: locals,
                },
                Instruction {
                    opcode: OpCode::LDARG0,
                    operands: a0,
                },
                Instruction {
                    opcode: OpCode::LDARG1,
                    operands: a1,
                },
                Instruction {
                    opcode: OpCode::ADD,
                    operands: a2,
                },
                Instruction {
                    opcode: OpCode::RET,
                    operands: a3,
                },
            ] if locals == &vec![0, 2]
                && a0.is_empty()
                && a1.is_empty()
                && a2.is_empty()
                && a3.is_empty()
        ));
    }

    #[test]
    fn void_function_implicit_return_pushes_null() {
        let f = FunctionDecl {
            attributes: vec![],
            return_ty: Type::Void,
            name: "noop".into(),
            params: vec![],
            body: Block { stmts: vec![] },
        };
        let compiled = compile_function(&f, &[], None, &empty_pkg())
            .expect("compile function should not fail");
        let inst = compiled.instructions;
        assert_eq!(inst[0].opcode, OpCode::INITSLOT);
        assert_eq!(inst[inst.len() - 2].opcode, OpCode::PUSHNULL);
        assert_eq!(inst[inst.len() - 1].opcode, OpCode::RET);
    }

    #[test]
    fn ssa_const_folding_eliminates_add_for_simple_var_init() {
        // Triggers IR pipeline via `var`.
        let src = r#"
            package demo;
            int f() {
                var x = 1 + 2;
                return x;
            }
        "#;
        let sf = parse_source_file(src).unwrap();
        let mut pkg = HashMap::new();
        for f in &sf.functions {
            pkg.insert(f.name.clone(), f.params.len());
        }
        let f = sf.functions.iter().find(|f| f.name == "f").unwrap();
        let compiled = compile_function(f, &[], None, &pkg).unwrap();
        let has_add = compiled
            .instructions
            .iter()
            .any(|i| i.opcode == OpCode::ADD);
        assert!(!has_add, "expected SSA const folding to remove ADD");
    }

    #[test]
    fn ssa_cse_eliminates_duplicate_add() {
        // `x + 1` appears twice; CSE should compute it once.
        let src = r#"
            package demo;
            int f(int x) {
                var a = x + 1;
                var b = x + 1;
                return a + b;
            }
        "#;
        let sf = parse_source_file(src).unwrap();
        let mut pkg = HashMap::new();
        for f in &sf.functions {
            pkg.insert(f.name.clone(), f.params.len());
        }
        let f = sf.functions.iter().find(|f| f.name == "f").unwrap();
        let compiled = compile_function(f, &[], None, &pkg).unwrap();
        let add_count = compiled
            .instructions
            .iter()
            .filter(|i| i.opcode == OpCode::ADD)
            .count();
        // With CSE: one ADD for (x+1), one ADD for (a+b) => 2.
        assert_eq!(add_count, 2, "expected CSE to reduce ADD count");
    }

    #[test]
    fn ssa_dce_removes_unused_computation() {
        let src = r#"
            package demo;
            int f(int x) {
                var a = x + 1;
                var b = x + 2;
                return a;
            }
        "#;
        let sf = parse_source_file(src).unwrap();
        let mut pkg = HashMap::new();
        for f in &sf.functions {
            pkg.insert(f.name.clone(), f.params.len());
        }
        let f = sf.functions.iter().find(|f| f.name == "f").unwrap();
        let compiled = compile_function(f, &[], None, &pkg).unwrap();
        let add_count = compiled
            .instructions
            .iter()
            .filter(|i| i.opcode == OpCode::ADD)
            .count();
        // Only `x+1` remains => 1 ADD.
        assert_eq!(add_count, 1, "expected DCE to remove unused add");
    }

    #[test]
    fn ssa_cse_reuses_struct_member_subexpr_in_distance() {
        let src = r#"
            package demo;
            struct Point {
                int x;
                int y;

                int distanceTo(Point other) {
                    return (self.x - other.x) * (self.x - other.x) + (self.y - other.y) * (self.y - other.y);
                }
            }
        "#;
        let sf = parse_source_file(src).unwrap();
        let mut pkg = HashMap::new();
        for f in &sf.functions {
            pkg.insert(f.name.clone(), f.params.len());
        }
        let point = sf.structs.iter().find(|s| s.name == "Point").unwrap();
        let m = point
            .methods
            .iter()
            .find(|m| m.name == "distanceTo")
            .unwrap();
        let lowered = lower_struct_method("Point", m);
        let compiled = compile_function(&lowered, &sf.structs, None, &pkg).unwrap();
        let sub_count = compiled
            .instructions
            .iter()
            .filter(|i| i.opcode == OpCode::SUB)
            .count();
        // With CSE on member loads/subexpr, we expect only `dx` and `dy` subtractions once each.
        assert_eq!(
            sub_count, 2,
            "expected CSE to avoid duplicated (self.x-other.x) SUBs"
        );
    }

    #[test]
    fn ssa_distance_to_stackify_min_locals_and_dup_square() {
        let src = r#"
            package demo;
            struct Point {
                int x;
                int y;

                int distanceTo(Point other) {
                    return (self.x - other.x) * (self.x - other.x) + (self.y - other.y) * (self.y - other.y);
                }
            }
        "#;
        let sf = parse_source_file(src).unwrap();
        let mut pkg = HashMap::new();
        for f in &sf.functions {
            pkg.insert(f.name.clone(), f.params.len());
        }
        let point = sf.structs.iter().find(|s| s.name == "Point").unwrap();
        let m = point
            .methods
            .iter()
            .find(|m| m.name == "distanceTo")
            .unwrap();
        let lowered = lower_struct_method("Point", m);
        let compiled = compile_function(&lowered, &sf.structs, None, &pkg).unwrap();
        let initslot = compiled
            .instructions
            .iter()
            .find(|i| i.opcode == OpCode::INITSLOT)
            .expect("INITSLOT");
        // Operand 0 = local slots; operand 1 = arg count. Args live in argument slots (`LDARG*`),
        // not as extra locals, so `dx*dx` / `dy*dy` should not require spill locals here.
        assert_eq!(
            initslot.operands.first().copied(),
            Some(0),
            "expected no locals beyond scratch/phi slots for this method body"
        );
        assert_eq!(
            initslot.operands.get(1).copied(),
            Some(2),
            "expected two VM arguments (receiver + `other`)"
        );
        assert!(
            compiled.instructions.windows(3).any(|w| {
                w[0].opcode == OpCode::SUB
                    && w[1].opcode == OpCode::DUP
                    && w[2].opcode == OpCode::MUL
            }),
            "expected `SUB; DUP; MUL` for squaring without intermediate STLOC"
        );
    }

    #[test]
    fn ssa_cse_eliminates_duplicate_index_load() {
        let src = r#"
            package demo;
            int f(int[] a) {
                var x = a[0];
                var y = a[0];
                return x + y;
            }
        "#;
        let sf = parse_source_file(src).unwrap();
        let mut pkg = HashMap::new();
        for f in &sf.functions {
            pkg.insert(f.name.clone(), f.params.len());
        }
        let f = sf.functions.iter().find(|f| f.name == "f").unwrap();
        let compiled = compile_function(f, &[], None, &pkg).unwrap();
        let pick = compiled
            .instructions
            .iter()
            .filter(|i| i.opcode == OpCode::PICKITEM)
            .count();
        assert_eq!(pick, 1, "expected CSE to share one `a[0]` load");
    }

    #[test]
    fn ssa_dce_keeps_index_store_without_use() {
        let src = r#"
            package demo;
            void f(int[] a) {
                a[0] = 1;
            }
        "#;
        let sf = parse_source_file(src).unwrap();
        let mut pkg = HashMap::new();
        for f in &sf.functions {
            pkg.insert(f.name.clone(), f.params.len());
        }
        let f = sf.functions.iter().find(|f| f.name == "f").unwrap();
        let compiled = compile_function(f, &[], None, &pkg).unwrap();
        assert!(
            compiled
                .instructions
                .iter()
                .any(|i| i.opcode == OpCode::SETITEM),
            "expected index store to survive DCE"
        );
    }

    #[test]
    fn ssa_struct_self_field_assign_emits_setitem() {
        let src = r#"
            package demo;
            struct P {
                int x;
                int y;
                void m() {
                    self.x = 1;
                }
            }
        "#;
        let sf = parse_source_file(src).unwrap();
        let structs = &sf.structs;
        let m = &sf.structs[0].methods[0];
        let f = lower_struct_method("P", m);
        let compiled = compile_function(&f, structs, None, &empty_pkg()).unwrap();
        assert!(
            compiled
                .instructions
                .iter()
                .any(|i| i.opcode == OpCode::SETITEM),
            "expected `self.field =` to lower to SETITEM"
        );
    }

    #[test]
    fn ssa_short_circuit_and_uses_branch_shape() {
        let src = r#"
            package demo;
            bool f(int x) {
                var b = (x > 0) && (x < 10);
                return b;
            }
        "#;
        let sf = parse_source_file(src).unwrap();
        let mut pkg = HashMap::new();
        for f in &sf.functions {
            pkg.insert(f.name.clone(), f.params.len());
        }
        let f = sf.functions.iter().find(|f| f.name == "f").unwrap();
        let compiled = compile_function(f, &[], None, &pkg).unwrap();

        let has_and_opcode = compiled
            .instructions
            .iter()
            .any(|i| i.opcode == OpCode::AND);
        assert!(
            !has_and_opcode,
            "expected short-circuit lowering to avoid AND opcode"
        );

        let has_jmpifnot = compiled
            .instructions
            .iter()
            .any(|i| i.opcode == OpCode::JMPIFNOT_L);
        assert!(
            has_jmpifnot,
            "expected short-circuit lowering to use JMPIFNOT_L"
        );
    }

    #[test]
    fn assert_lowers_to_assertmsg() {
        let f = FunctionDecl {
            attributes: vec![],
            return_ty: Type::Void,
            name: "c".into(),
            params: vec![],
            body: Block {
                stmts: vec![Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("assert".into())),
                    args: vec![
                        Expr::Literal(Literal::Bool(true)),
                        Expr::Literal(Literal::String("ok".into())),
                    ],
                })],
            },
        };
        let compiled = compile_function(&f, &[], None, &empty_pkg())
            .expect("compile function should not fail");
        let inst = compiled.instructions;
        assert!(inst.iter().any(|i| i.opcode == OpCode::ASSERTMSG));
    }

    #[test]
    fn emit_statement_uses_runtime_notify() {
        let src = r#"
        void f() {
            emit transfer(1, 2);
        }
        "#;
        let sf = parse_source_file(src).expect("parse source file should not fail");
        let compiled = compile_function(&sf.functions[0], &[], None, &empty_pkg())
            .expect("compile function should not fail");
        let inst = compiled.instructions;
        assert!(inst.iter().any(|i| i.opcode == OpCode::PACK));
        assert!(inst.iter().any(|i| i.opcode == OpCode::SYSCALL
            && i.operands == Syscall::RUNTIME_NOTIFY.token().to_le_bytes().to_vec()));
    }

    #[test]
    fn package_level_body_call_emits_call_l_when_add_is_in_arity_map() {
        let mut pkg = HashMap::new();
        pkg.insert("add".into(), 2usize);
        let f = FunctionDecl {
            attributes: vec![],
            return_ty: Type::Int,
            name: "caller".into(),
            params: vec![],
            body: Block {
                stmts: vec![Stmt::Return(Some(Expr::Call {
                    callee: Box::new(Expr::Ident("add".into())),
                    args: vec![
                        Expr::Literal(Literal::Int("10".into())),
                        Expr::Literal(Literal::Int("20".into())),
                    ],
                }))],
            },
        };
        let compiled = compile_function(&f, &[], None, &pkg).unwrap();
        assert!(
            compiled
                .instructions
                .iter()
                .any(|i| i.opcode == OpCode::CALL_L),
            "expected CALL_L for add(...)"
        );
        assert_eq!(compiled.call_patches.len(), 1);
    }

    #[test]
    fn struct_literal_and_member_pickitem() {
        let src = r#"
        struct Point { int x; int y; }

        int main() {
            var p = Point { x: 3, y: 4 };
            return p.x;
        }
        "#;
        let sf = parse_source_file(src).expect("parse source file should not fail");
        let structs = &sf.structs;
        let compiled = compile_function(&sf.functions[0], structs, None, &empty_pkg())
            .expect("compile function should not fail");
        let inst = compiled.instructions;
        assert!(inst.iter().any(|i| i.opcode == OpCode::PACK));
        let mut pick = 0u32;
        for w in inst.windows(2) {
            if w[0].opcode == OpCode::PUSH0 && w[1].opcode == OpCode::PICKITEM {
                pick += 1;
            }
        }
        assert!(pick >= 1, "expected index 0 + PICKITEM for p.x");
    }
}
