//! Integration-style tests for IR stackify + block/branch emission (`compile_ir` pipeline).

use std::collections::HashMap;

use crate::codegen::context::{FnSig, FunctionCompileContext};
use crate::codegen::function::compile_function;
use crate::ir::lower::lower_function_to_ir;
use crate::ir::Terminator;
use crate::syntax::parser::parse_source_file;
use crate::target::opcode::OpCode;

fn package_fns_from_source(sf: &crate::syntax::ast::SourceFile) -> HashMap<String, FnSig> {
    sf.functions
        .iter()
        .map(|f| (f.name.clone(), FnSig::from_function(f)))
        .collect()
}

fn compile_named_fn(src: &str, name: &str) -> crate::codegen::function::CompliledFunction {
    let sf = parse_source_file(src).expect("parse");
    let fns = package_fns_from_source(&sf);
    let func = sf
        .functions
        .iter()
        .find(|f| f.name == name)
        .expect("find fn");
    let ctx = FunctionCompileContext::new(&sf.structs, &fns);
    compile_function(func, &ctx).expect("compile")
}

fn opcodes(inst: &[crate::target::Instruction]) -> Vec<OpCode> {
    inst.iter().map(|i| i.opcode).collect()
}

#[test]
fn ir_early_return_guard_uses_jmpif_not_jmpifnot_for_then_return() {
    let src = r#"
        package demo;
        bool early(int x) {
            if (x < 0) { return false; }
            return true;
        }
    "#;
    let c = compile_named_fn(src, "early");
    let ops = opcodes(&c.instructions);
    assert!(
        ops.iter()
            .any(|o| matches!(o, OpCode::JMPIF | OpCode::JMPIF_L)),
        "early-return branch fold should emit JMPIF* toward then (return), got: {ops:?}"
    );
    let first_conditional = ops.iter().position(|o| {
        matches!(
            o,
            OpCode::JMPIF | OpCode::JMPIF_L | OpCode::JMPIFNOT | OpCode::JMPIFNOT_L
        )
    });
    let Some(ix) = first_conditional else {
        panic!("expected a conditional branch opcode");
    };
    assert!(
        matches!(ops[ix], OpCode::JMPIF | OpCode::JMPIF_L),
        "first conditional on early-return if should be JMPIF*, got {:?} at {}",
        ops[ix],
        ix
    );
}

#[test]
fn ir_if_else_two_returns_no_jmpifnot_when_join_args_empty() {
    let src = r#"
        package demo;
        int pick(int x) {
            if (x == 0) { return 1; }
            else { return 2; }
        }
    "#;
    let c = compile_named_fn(src, "pick");
    let ops = opcodes(&c.instructions);
    assert!(
        !ops.iter()
            .any(|o| matches!(o, OpCode::JMPIFNOT | OpCode::JMPIFNOT_L)),
        "both arms return with no join phis: expect JMPIF+JMP only, got: {ops:?}"
    );
    assert!(
        ops.iter()
            .any(|o| matches!(o, OpCode::JMPIF | OpCode::JMPIF_L)),
        "expected JMPIF*: {ops:?}"
    );
    assert!(
        ops.iter().any(|o| matches!(o, OpCode::JMP | OpCode::JMP_L)),
        "expected JMP*: {ops:?}"
    );
}

#[test]
fn ir_if_else_assign_both_sides_uses_jmpifnot_when_join_has_phi() {
    let src = r#"
        package demo;
        int g(int x) {
            var a = 0;
            if (x > 0) { a = 1; }
            else { a = 2; }
            return a;
        }
    "#;
    let c = compile_named_fn(src, "g");
    let ops = opcodes(&c.instructions);
    assert!(
        ops.iter()
            .any(|o| matches!(o, OpCode::JMPIFNOT | OpCode::JMPIFNOT_L)),
        "non-empty then_args and else_args should use JMPIFNOT* header, got: {ops:?}"
    );
}

#[test]
fn branch_try_detects_foldable_early_return_branch_in_ir() {
    let src = r#"
        package demo;
        bool f(int x) {
            if (x < 0) { return false; }
            return true;
        }
    "#;
    let sf = parse_source_file(src).expect("parse");
    let func = sf.functions.iter().find(|f| f.name == "f").unwrap();
    let fns = package_fns_from_source(&sf);
    let ctx = FunctionCompileContext::new(&sf.structs, &fns);
    let fir = lower_function_to_ir(func, &ctx).expect("lower");
    let mut found = false;
    for b in fir.blocks.values() {
        if let Terminator::Branch {
            then_bb,
            then_args,
            else_bb,
            ..
        } = &b.term
        {
            if fir
                .branch_try_jmpif_then_return_else_relay(*then_bb, *else_bb, then_args)
                .is_some()
            {
                found = true;
            }
        }
    }
    assert!(
        found,
        "expected IR to contain a Branch matching early-return+empty-else-relay pattern"
    );
}

#[test]
fn compile_ir_pipeline_void_body_has_initslot_and_ret() {
    let src = r#"
        package demo;
        void noop() { }
    "#;
    let c = compile_named_fn(src, "noop");
    assert_eq!(c.instructions[0].opcode, OpCode::INITSLOT);
    assert_eq!(c.instructions.last().unwrap().opcode, OpCode::RET);
}
