//! Tests for the codegen module.

use super::*;
use crate::codegen::context::{FnSig, FunctionCompileContext};
use crate::syntax::parser::parse_source_file;
use crate::target::opcode::OpCode;
use crate::target::syscall::Syscall;

fn package_fns_from_source(sf: &SourceFile) -> HashMap<String, FnSig> {
    sf.functions
        .iter()
        .map(|f| (f.name.clone(), FnSig::from_function(f)))
        .collect()
}

#[test]
fn codegen_package_and_contract_functions() {
    let src = r#"
        package demo;

        int double(int x) {
            return x + x;
        }

        contract C {
            void nop() { }
        }
    "#;
    let sf = parse_source_file(src).unwrap();
    let out = Codegen::new().codegen_source_file(&sf).unwrap();
    assert_eq!(out.package_functions.len(), 1);
    assert_eq!(out.package_functions[0].name, "double");
    assert_eq!(out.package_functions[0].contract, None);
    assert!(!out.package_functions[0].instructions.is_empty());

    assert_eq!(out.contract_methods.len(), 1);
    assert_eq!(out.contract_methods[0].name, "nop");
    assert_eq!(out.contract_methods[0].contract.as_deref(), Some("C"));
    assert_eq!(
        out.contract_methods[0].instructions[0].opcode,
        OpCode::INITSLOT
    );

    let flat = out.flatten_to_bytes();
    let n0: usize = out.package_functions[0]
        .instructions
        .iter()
        .map(Instruction::encoded_len)
        .sum();
    let n1: usize = out.contract_methods[0]
        .instructions
        .iter()
        .map(Instruction::encoded_len)
        .sum();
    assert_eq!(flat.len(), n0 + n1);
    assert!(out.struct_methods.is_empty());
}

#[test]
fn codegen_map_remove_emits_remove_with_key_on_top() {
    let src = r#"
        package demo;
        contract C {
            int f(map[int, int] m) {
                m.remove(1);
                return 0;
            }
        }
    "#;
    let sf = parse_source_file(src).unwrap();
    let out = Codegen::new().codegen_source_file(&sf).unwrap();
    let method = &out.contract_methods[0];
    let remove_idx = method
        .instructions
        .iter()
        .position(|i| i.opcode == OpCode::REMOVE)
        .expect("expected REMOVE opcode for map.remove");
    assert!(remove_idx >= 2, "expected map/key pushes before REMOVE");
    assert_eq!(method.instructions[remove_idx - 2].opcode, OpCode::LDARG0);
    assert_eq!(method.instructions[remove_idx - 1].opcode, OpCode::PUSH1);
}

#[test]
fn codegen_empty_source_no_functions() {
    let sf = parse_source_file("package p;").unwrap();
    let out = Codegen::new().codegen_source_file(&sf).unwrap();
    assert!(out.package_functions.is_empty());
    assert!(out.struct_methods.is_empty());
    assert!(out.contract_methods.is_empty());
}

#[test]
fn codegen_contract_without_methods() {
    let sf = parse_source_file("contract X { int x; }").unwrap();
    let out = Codegen::new().codegen_source_file(&sf).unwrap();
    assert!(out.package_functions.is_empty());
    assert!(out.struct_methods.is_empty());
    assert!(out.contract_methods.is_empty());
}

#[test]
fn codegen_struct_call_emits_linked_call_l() {
    let src = r#"
        struct Point {
            int x;
            int y;
            int dist2(Point other) {
                return (self.x - other.x) * (self.x - other.x);
            }
        }
        contract C {
            int test() {
                var p = Point { x: 1, y: 2 };
                var q = Point { x: 4, y: 6 };
                return p.dist2(q);
            }
        }
    "#;
    let sf = parse_source_file(src).unwrap();
    let out = Codegen::new().codegen_source_file(&sf).unwrap();
    let func = &out.contract_methods[0];
    assert!(
        func.instructions.iter().any(|i| i.opcode == OpCode::CALL_L),
        "expected CALL_L for p.dist2(q)"
    );
    assert!(
        func.call_patches.is_empty(),
        "linker should apply and clear CALL_L patches"
    );
}

#[test]
fn codegen_contract_self_method_call_emits_call_l_without_self_arg() {
    let src = r#"
        contract C {
            bool helper(int x) { return x >= 0; }
            bool m(int x) {
                if !self.helper(x) { return false; }
                return true;
            }
        }
    "#;
    let sf = parse_source_file(src).unwrap();
    let out = Codegen::new().codegen_source_file(&sf).unwrap();
    let helper = out
        .contract_methods
        .iter()
        .find(|f| f.name == "helper")
        .expect("helper");
    let m = out
        .contract_methods
        .iter()
        .find(|f| f.name == "m")
        .expect("m");
    let call_index = m
        .instructions
        .iter()
        .position(|i| i.opcode == OpCode::CALL_L)
        .expect("CALL_L for self.helper(x)");
    assert_eq!(call_index, 2, "expected INITSLOT, LDARG0, CALL_L, ...");
    assert_eq!(m.instructions[call_index - 1].opcode, OpCode::LDARG0);
    assert_eq!(helper.link_symbol(), "C::helper");
    assert!(m.call_patches.is_empty());
}

#[test]
fn codegen_package_function_call_from_contract_method() {
    let src = r#"
        int add(int a, int b) { return a + b; }
        contract C {
            int test() { return add(1, 2); }
        }
    "#;
    let sf = parse_source_file(src).unwrap();
    let out = Codegen::new().codegen_source_file(&sf).unwrap();
    assert_eq!(out.package_functions.len(), 1);
    let func = &out.contract_methods[0];
    assert!(
        func.instructions.iter().any(|i| i.opcode == OpCode::CALL_L),
        "expected CALL_L for add(1, 2)"
    );
    assert!(
        func.call_patches.is_empty(),
        "linker should resolve CALL_L to package `add`"
    );
}

/// `add(1, 2)` must push `b` then `a` (|2|1| bottom → top) before `CALL_L`, matching normal calls.
#[test]
fn codegen_package_call_pushes_args_in_reverse_parameter_order() {
    let src = r#"
        int add(int a, int b) { return a + b; }
        contract C {
            int test() { return add(1, 2); }
        }
    "#;
    let sf = parse_source_file(src).unwrap();
    let out = Codegen::new().codegen_source_file(&sf).unwrap();
    let func = &out.contract_methods[0];
    let call_index = func
        .instructions
        .iter()
        .position(|i| i.opcode == OpCode::CALL_L)
        .expect("CALL_L");
    assert!(
        call_index >= 2,
        "expected two push instructions before CALL_L"
    );
    assert_eq!(func.instructions[call_index - 2].opcode, OpCode::PUSH2);
    assert_eq!(func.instructions[call_index - 1].opcode, OpCode::PUSH1);
}

#[test]
fn codegen_package_function_calls_sibling_package_function() {
    let src = r#"
        int two() { return 2; }
        int four() { return two() + two(); }
        contract C { void g() { } }
    "#;
    let sf = parse_source_file(src).unwrap();
    let out = Codegen::new().codegen_source_file(&sf).unwrap();
    let func = out
        .package_functions
        .iter()
        .find(|f| f.name == "four")
        .expect("four()");
    let call_ls = func
        .instructions
        .iter()
        .filter(|i| i.opcode == OpCode::CALL_L)
        .count();
    assert_eq!(call_ls, 2, "two() + two() → two CALL_L sites");
    assert!(
        func.call_patches.is_empty(),
        "CALL_L from package fn to package fn should link"
    );
}

#[test]
fn codegen_package_call_wrong_arity_is_rejected() {
    let src = r#"
        int add(int a, int b) { return a + b; }
        contract C { int t() { return add(1); } }
    "#;
    let sf = parse_source_file(src).unwrap();
    let err = Codegen::new().codegen_source_file(&sf).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("add") && msg.contains('2') && msg.contains('1'),
        "expected arity mismatch in message, got: {msg}"
    );
}

#[test]
fn codegen_duplicate_top_level_function_name_is_rejected() {
    let src = r#"
        int f() { return 0; }
        int f() { return 1; }
        contract C { void g() { } }
    "#;
    let sf = parse_source_file(src).unwrap();
    let err = Codegen::new().codegen_source_file(&sf).unwrap_err();
    assert!(
        err.to_string().contains("duplicate top-level function `f`"),
        "got: {err}"
    );
}

#[test]
fn codegen_struct_method_lowers_and_emits_routine() {
    let src = r#"
        struct Point {
            int x;
            int y;
            int sqDist(Point other) {
                return (self.x - other.x) * (self.x - other.x);
            }
        }
        contract C { void f() { } }
    "#;
    let sf = parse_source_file(src).unwrap();
    let out = Codegen::new().codegen_source_file(&sf).unwrap();
    assert_eq!(out.struct_methods.len(), 1);
    assert_eq!(out.struct_methods[0].name, "Point::sqDist");
    assert!(!out.struct_methods[0].instructions.is_empty());
    assert_eq!(
        out.struct_methods[0].instructions[0].opcode,
        OpCode::INITSLOT
    );
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
    let fns = HashMap::new();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(&simple_add(), &ctx).unwrap();
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
fn void_function_implicit_return_emits_ret_without_pushnull() {
    let f = FunctionDecl {
        attributes: vec![],
        return_ty: Type::Void,
        name: "noop".into(),
        params: vec![],
        body: Block { stmts: vec![] },
    };
    let fns = HashMap::new();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(&f, &ctx).expect("compile function should not fail");
    let inst = compiled.instructions;
    assert_eq!(inst[0].opcode, OpCode::INITSLOT);
    assert_eq!(inst[inst.len() - 1].opcode, OpCode::RET);
    assert!(
        !inst.iter().any(|i| i.opcode == OpCode::PUSHNULL),
        "void function should not push null before RET"
    );
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
    let fns = package_fns_from_source(&sf);
    let f = sf.functions.iter().find(|f| f.name == "f").unwrap();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(f, &ctx).unwrap();
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
    let fns = package_fns_from_source(&sf);
    let f = sf.functions.iter().find(|f| f.name == "f").unwrap();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(f, &ctx).unwrap();
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
    let fns = package_fns_from_source(&sf);
    let f = sf.functions.iter().find(|f| f.name == "f").unwrap();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(f, &ctx).unwrap();
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
    let fns = package_fns_from_source(&sf);
    let point = sf.structs.iter().find(|s| s.name == "Point").unwrap();
    let m = point
        .methods
        .iter()
        .find(|m| m.name == "distanceTo")
        .unwrap();
    let lowered = lower_struct_method("Point", m);
    let ctx = FunctionCompileContext::new(&sf.structs, &fns);
    let compiled = compile_function(&lowered, &ctx).unwrap();
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
    let fns = package_fns_from_source(&sf);
    let point = sf.structs.iter().find(|s| s.name == "Point").unwrap();
    let m = point
        .methods
        .iter()
        .find(|m| m.name == "distanceTo")
        .unwrap();
    let lowered = lower_struct_method("Point", m);
    let ctx = FunctionCompileContext::new(&sf.structs, &fns);
    let compiled = compile_function(&lowered, &ctx).unwrap();
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
            w[0].opcode == OpCode::SUB && w[1].opcode == OpCode::DUP && w[2].opcode == OpCode::MUL
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
    let fns = package_fns_from_source(&sf);
    let f = sf.functions.iter().find(|f| f.name == "f").unwrap();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(f, &ctx).unwrap();
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
    let fns = package_fns_from_source(&sf);
    let func = sf.functions.iter().find(|f| f.name == "f").unwrap();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(func, &ctx).unwrap();
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
    let method = &sf.structs[0].methods[0];
    let func = lower_struct_method("P", method);
    let fns = HashMap::new();
    let ctx = FunctionCompileContext::new(structs, &fns);
    let compiled = compile_function(&func, &ctx).unwrap();
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
    let fns = package_fns_from_source(&sf);
    let func = sf.functions.iter().find(|f| f.name == "f").unwrap();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(func, &ctx).unwrap();
    let has_and_opcode = compiled
        .instructions
        .iter()
        .any(|i| i.opcode == OpCode::AND);
    assert!(
        !has_and_opcode,
        "expected short-circuit lowering to avoid AND opcode"
    );

    let has_cond_branch = compiled.instructions.iter().any(|i| {
        matches!(
            i.opcode,
            OpCode::JMPIFNOT_L | OpCode::JMPIFNOT | OpCode::JMPIF_L | OpCode::JMPIF
        )
    });
    assert!(
        has_cond_branch,
        "expected short-circuit lowering to branch on condition (JMPIF* / JMPIFNOT*)"
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
    let fns = HashMap::new();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(&f, &ctx).expect("compile function should not fail");
    let inst = compiled.instructions;
    assert!(inst.iter().any(|i| i.opcode == OpCode::ASSERTMSG));
    assert!(
        !inst
            .windows(2)
            .any(|w| w[0].opcode == OpCode::ASSERTMSG && w[1].opcode == OpCode::DROP),
        "void assert statement must not DROP after ASSERTMSG"
    );
}

#[test]
fn min_stmt_emits_drop_after_min() {
    let f = FunctionDecl {
        attributes: vec![],
        return_ty: Type::Void,
        name: "c".into(),
        params: vec![],
        body: Block {
            stmts: vec![Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("min".into())),
                args: vec![
                    Expr::Literal(Literal::Int("1".into())),
                    Expr::Literal(Literal::Int("2".into())),
                ],
            })],
        },
    };
    let fns = HashMap::new();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(&f, &ctx).expect("compile function should not fail");
    let inst = compiled.instructions;
    assert!(inst.iter().any(|i| i.opcode == OpCode::MIN));
    assert!(
        inst.windows(2)
            .any(|w| w[0].opcode == OpCode::MIN && w[1].opcode == OpCode::DROP),
        "min(...) statement must DROP its int result"
    );
}

#[test]
fn emit_statement_uses_runtime_notify() {
    let src = r#"
    void f() {
        emit transfer(1, 2);
    }
    "#;
    let sf = parse_source_file(src).expect("parse source file should not fail");
    let fns = HashMap::new();
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled =
        compile_function(&sf.functions[0], &ctx).expect("compile function should not fail");
    let inst = compiled.instructions;
    assert!(inst.iter().any(|i| i.opcode == OpCode::PACK));
    assert!(inst.iter().any(|i| i.opcode == OpCode::SYSCALL
        && i.operands == Syscall::RUNTIME_NOTIFY.token().to_le_bytes().to_vec()));
}

#[test]
fn package_level_body_call_emits_call_l_when_add_is_in_arity_map() {
    let mut fns = HashMap::new();
    fns.insert("add".into(), FnSig::new(2, Type::Int));
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
    let ctx = FunctionCompileContext::new(&[], &fns);
    let compiled = compile_function(&f, &ctx).unwrap();
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
    let fns = HashMap::new();
    let ctx = FunctionCompileContext::new(structs, &fns);
    let compiled =
        compile_function(&sf.functions[0], &ctx).expect("compile function should not fail");
    let instructions = compiled.instructions;
    assert!(instructions.iter().any(|i| i.opcode == OpCode::PACK));
    let mut pick = 0u32;
    for w in instructions.windows(2) {
        if w[0].opcode == OpCode::PUSH0 && w[1].opcode == OpCode::PICKITEM {
            pick += 1;
        }
    }
    assert!(pick >= 1, "expected index 0 + PICKITEM for p.x");
}
