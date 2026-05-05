use super::*;

use crate::syntax::parser::parse_source_file;
use std::collections::HashMap;

fn pkg_arity(sf: &crate::syntax::ast::SourceFile) -> HashMap<String, usize> {
    let mut pkg = HashMap::new();
    for f in &sf.functions {
        pkg.insert(f.name.clone(), f.params.len());
    }
    pkg
}

#[test]
fn lowering_supports_for_in_array() {
    let src = r#"
        package demo;
        int f(int[] xs) {
            var sum = 0;
            for x in xs {
                sum += x;
            }
            return sum;
        }
    "#;
    let source_file = parse_source_file(src).unwrap();
    let pkg = pkg_arity(&source_file);
    let func = source_file
        .functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap();
    let ir = lower_function_to_ir(func, &[], None, &pkg).expect("for-array should lower to IR");
    let has_size = ir.blocks.values().any(|bb| {
        bb.instrs
            .iter()
            .any(|(_, instr)| matches!(instr, Instr::Size { .. }))
    });
    assert!(has_size, "expected for-array lowering to use Instr::Size");
}

#[test]
fn lowering_supports_for_in_map() {
    let src = r#"
        package demo;
        int f(map[int, int] m) {
            var sum = 0;
            for k, v in m {
                sum += k;
                sum += v;
            }
            return sum;
        }
    "#;
    let source_file = parse_source_file(src).unwrap();
    let pkg = pkg_arity(&source_file);
    let func = source_file
        .functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap();
    let ir = lower_function_to_ir(func, &[], None, &pkg).expect("for-map should lower to IR");
    let has_keys = ir.blocks.values().any(|bb| {
        bb.instrs
            .iter()
            .any(|(_, instr)| matches!(instr, Instr::Keys { .. }))
    });
    assert!(has_keys, "expected for-map lowering to use Instr::Keys");
}

#[test]
fn lowering_supports_builtin_method_calls() {
    let src = r#"
        package demo;
        int f(map[int, int] m, int[] xs, string s) {
            var a = xs.size();
            var b = m.keys().size();
            var c = m.values().size();
            var d = m.has(1) as int;
            var e = s.sub(0, 1).size();
            xs.push(1);
            xs.pop();
            xs.clear();
            m.remove(1);
            return a + b + c + d + e;
        }
    "#;
    let source_file = parse_source_file(src).unwrap();
    let pkg = pkg_arity(&source_file);
    let func = source_file
        .functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap();
    let ir =
        lower_function_to_ir(func, &[], None, &pkg).expect("builtin method calls should lower");
    let has_remove = ir.blocks.values().any(|bb| {
        bb.instrs
            .iter()
            .any(|(_, instr)| matches!(instr, Instr::Remove { .. }))
    });
    assert!(
        has_remove,
        "expected map.remove(...) lowering to use Instr::Remove"
    );
}

#[test]
fn lowering_supports_runtime_notify_and_contract_call() {
    let src = r#"
        package demo;
        void f(hash160 c) {
            runtime.notify("evt", any[] { 1 });
            runtime.contractCall(c, "method", any[] { 1 });
            return;
        }
    "#;
    let source_file = parse_source_file(src).unwrap();
    let pkg = pkg_arity(&source_file);
    let func = source_file
        .functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap();
    lower_function_to_ir(func, &[], None, &pkg).expect("runtime calls should lower");
}

#[test]
fn lowering_supports_short_circuit_with_assignment_and() {
    let src = r#"
        package demo;
        int f() {
            var x = 0;
            var y = 0;
            var r = ((x = 1) == 1) && ((y = 2) == 2);
            return x + y + (r as int);
        }
    "#;
    let source_file = parse_source_file(src).unwrap();
    let pkg = pkg_arity(&source_file);
    let func = source_file
        .functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap();
    lower_function_to_ir(func, &[], None, &pkg)
        .expect("short-circuit && with assignment should lower");
}

#[test]
fn lowering_supports_short_circuit_with_assignment_or() {
    let src = r#"
        package demo;
        int f() {
            var x = 0;
            var y = 0;
            var r = ((x = 1) == 1) || ((y = 2) == 2);
            return x + y + (r as int);
        }
    "#;
    let source_file = parse_source_file(src).unwrap();
    let pkg = pkg_arity(&source_file);
    let func = source_file
        .functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap();
    lower_function_to_ir(func, &[], None, &pkg)
        .expect("short-circuit || with assignment should lower");
}
