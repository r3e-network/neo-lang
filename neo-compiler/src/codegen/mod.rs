//! Codegen transfers the AST to NeoVM instructions.
//!
//! For contract fileds, codegen will transfer the these fields to storage items.
//! i.e. the syscall `System.Storage.Local.Put`, `System.Storage.Local.Get`,
//! `System.Storage.Local.Delete`, `System.Storage.Local.Find` will be used to store the fields.
//! - For single value properties, the key is field name, the value is the field value.
//! - For array properties, the key is field name + the index, the value is the value of the array.
//! - For map properties, the key is the map name + the key, the value is the value of the map.
//!
//! For struct, codegen will transfer neo-lang struct to NeoVM array, but cannot push or pop it.
//! For example, the struct `{ int a; int b; }` will be transferred to the NeoVM array `[a, b]`, and `s.a` will be `s[0]`(PICKITEM 0).
//!
//! For emit event, codegen will transfer it to the syscall `System.Runtime.Notify`.
//! For example, the event `transfer(hash160 source, hash160 dest, int amount)`
//! will be transferred to the NeoVM array `[source, dest, amount]`,
//! and the syscall `System.Runtime.Notify` will be called with the array.
//!
//! For contract call(i.e. call other contract), codegen will transfer it to the syscall `System.Contract.Call`.
//! For example, the contract call `contract.transfer(source, dest, amount)`
//! will be transferred to the NeoVM array `[contract, transfer, source, dest, amount]`,
//! and the syscall `System.Contract.Call` will be called with the array.
//!
//! For map(in memory, i.e not contract field), codegen will transfer it to the NeoVM Map. Operations:
//! PICKITEM, SETITEM, REMOVE, HASKEY, KEYS, VALUES, SIZE, NEWMAP, PACKMAP, UNPACK, CLEARITEMS.
//!
//! For array(in memory, i.e not contract field), codegen will transfer it to the NeoVM Array. Operations:
//! APPEND, POPITEM, PICKITEM, SETITEM, REMOVE, SIZE, NEWARRAY, PACKARRAY, UNPACK, CLEARITEMS.
//!
//! For string(in memory, i.e not contract field), codegen will transfer it to the NeoVM ByteString. Operations:
//! SIZE.
//!
//! For buffer(in memory, i.e not contract field), codegen will transfer it to the NeoVM Buffer. Operations:
//! NEWBUFFER, MEMCPY, SUBSTR, CAT, APPEND, SETITEM, PICKITEM, REMOVE, LEFT, RIGHT.
//!
//! For function call or self contract method call, codegen will transfer it to the NeoVM call instruction.
//! The arguments will be pushed to the stack in the order of the parameters.
//! The return value will be pushed to the stack after the call.
//! For example, the function call `add(a, b)`, The top two items on the stack will be | b | a,
//! i.e. the arguments are pushed in reverse order, the first pushed is `b`, then pushed is `a`.
//! after the call, the top item on the stack will be | a + b |, i.e. the return value is pushed to the stack.
//! The callee should load arguments by LDARG opcodes.
//!
//! For struct method call, codegen will transfer it to the NeoVM call instruction.
//! Unlike function call, the first argument will be the struct instance.
//! For example, the struct method call `s.add(a, b)`, The top three items on the stack will be | b | a | s,
//! i.e. the arguments are pushed in reverse order, the first pushed is `b`, then pushed is `a`, then pushed is `s`.
//! after the call, the top item on the stack will be | s.a + s.b |.
//!
//! For syscall, codegen will transfer it to the NeoVM syscall instruction.
//! The arguments order is the same as function call, but the top of the stack is the syscall token.
//! (e.g. `System.Storage.Local.Put(key, value)` has `| value | key |` before `SYSCALL`).
//!

pub mod env;
pub mod expr;
pub mod function;
pub mod ir_codegen;
pub mod opt;
pub mod typecheck;

use std::collections::HashMap;

use crate::codegen::function::{compile_function, lower_struct_method};
use crate::codegen::opt::Optimizer;
use crate::syntax::ast::*;
use crate::target::opcode::OpCode;
use crate::target::Instruction;

/// One compiled NeoVM routine for a neo-lang function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledFunction {
    pub name: String,
    /// `None` for `package` / file-level functions; `Some(contract_name)` for contract methods.
    pub contract: Option<String>,
    pub instructions: Vec<Instruction>,
    /// Filled during codegen; [`link_call_l_patches`] rewrites these `CALL_L` sites and clears the list.
    pub call_patches: Vec<(usize, String)>,
}

impl CompiledFunction {
    /// Unique name for `CALL_L` linking across the flattened script (e.g. `Point::distanceTo`, `Example::transfer`).
    pub fn link_symbol(&self) -> String {
        match &self.contract {
            Some(c) => format!("{c}::{}", self.name),
            None => self.name.clone(),
        }
    }
}

fn bytecode_offset_in_routine(instructions: &[Instruction], instruction_index: usize) -> usize {
    instructions[..instruction_index]
        .iter()
        .map(Instruction::encoded_len)
        .sum()
}

/// All functions compiled from a [`SourceFile`] (package functions, struct methods, contract methods).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSourceFile {
    pub package_functions: Vec<CompiledFunction>,
    /// One NeoVM routine per [`StructDecl::methods`] entry (name `Struct::method`).
    pub struct_methods: Vec<CompiledFunction>,
    pub contract_methods: Vec<CompiledFunction>,
}

impl CompiledSourceFile {
    /// Encode all instructions in order (package functions, then contract methods) to a single script.
    pub fn flatten_to_bytes(&self) -> Vec<u8> {
        let cap: usize = self
            .package_functions
            .iter()
            .chain(self.struct_methods.iter())
            .chain(self.contract_methods.iter())
            .flat_map(|f| f.instructions.iter().map(Instruction::encoded_len))
            .sum();
        let mut out = Vec::with_capacity(cap);
        for f in &self.package_functions {
            for inst in &f.instructions {
                inst.encode_into(&mut out);
            }
        }
        for f in &self.struct_methods {
            for inst in &f.instructions {
                inst.encode_into(&mut out);
            }
        }
        for f in &self.contract_methods {
            for inst in &f.instructions {
                inst.encode_into(&mut out);
            }
        }
        out
    }

    pub(crate) fn link_call_l_patches(&mut self) -> Result<(), CodegenError> {
        let mut offsets = HashMap::new();
        let mut off = 0usize;
        for f in self
            .package_functions
            .iter()
            .chain(self.struct_methods.iter())
            .chain(self.contract_methods.iter())
        {
            if offsets.insert(f.link_symbol(), off).is_some() {
                return Err(CodegenError::Unsupported(format!(
                    "duplicate compiled routine `{}` (cannot link CALL_L)",
                    f.link_symbol()
                )));
            }
            off += f
                .instructions
                .iter()
                .map(Instruction::encoded_len)
                .sum::<usize>();
        }

        let apply = |f: &mut CompiledFunction| -> Result<(), CodegenError> {
            let my_start = *offsets
                .get(&f.link_symbol())
                .expect("link_symbol registered");
            for (inst_idx, target_sym) in std::mem::take(&mut f.call_patches) {
                let target_pc = offsets.get(target_sym.as_str()).ok_or_else(|| {
                    CodegenError::Unsupported(format!(
                        "CALL_L target `{target_sym}` not found (from `{}`)",
                        f.link_symbol()
                    ))
                })?;
                let call_pc = my_start + bytecode_offset_in_routine(&f.instructions, inst_idx);
                let relative = i32::try_from(*target_pc as i64 - call_pc as i64).map_err(|_| {
                    CodegenError::Unsupported("CALL_L relative offset overflow".into())
                })?;
                let inst = f
                    .instructions
                    .get_mut(inst_idx)
                    .expect("CALL_L patch index in range");
                if inst.opcode != OpCode::CALL_L || inst.operands.len() != 4 {
                    return Err(CodegenError::Unsupported(
                        "internal: CALL_L patch on wrong instruction".into(),
                    ));
                }
                inst.operands.copy_from_slice(&relative.to_le_bytes());
            }
            Ok(())
        };

        for f in self.package_functions.iter_mut() {
            apply(f)?;
        }
        for f in self.struct_methods.iter_mut() {
            apply(f)?;
        }
        for f in self.contract_methods.iter_mut() {
            apply(f)?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    #[error(transparent)]
    Typecheck(#[from] typecheck::TypeError),
    #[error("codegen: undefined variable: {0}")]
    UndefinedVariable(String),
    #[error("codegen: duplicate local `{0}` in the same block")]
    DuplicateLocal(String),
    #[error("codegen: unsupported: {0}")]
    Unsupported(String),
    #[error("codegen: invalid integer literal `{0}`")]
    BadIntegerLiteral(String),
    #[error("codegen: too many locals or parameters (max 255)")]
    LocalLimitExceeded,
}

pub struct Codegen {}

impl Codegen {
    pub fn new() -> Self {
        Self {}
    }

    /// Compiles all top-level [`SourceFile::functions`] and every [`ContractMember::Function`]
    /// under [`SourceFile::contract`], if present.
    pub fn codegen_source_file(
        &mut self,
        source: &SourceFile,
    ) -> Result<CompiledSourceFile, CodegenError> {
        source.type_check()?;
        let get_contract_fields = |contract: &ContractDecl| {
            contract
                .members
                .iter()
                .filter_map(|m| match m {
                    ContractMember::Field(f) => Some(f.clone()),
                    _ => None,
                })
                .collect::<Vec<ContractField>>()
        };
        let contract_fields = source
            .contract
            .as_ref()
            .map(get_contract_fields)
            .unwrap_or_default();

        let storage_fields = (!contract_fields.is_empty()).then_some(contract_fields.as_slice());
        let mut package_fn_arity = HashMap::new();
        for func in &source.functions {
            if package_fn_arity
                .insert(func.name.clone(), func.params.len())
                .is_some()
            {
                return Err(CodegenError::Unsupported(format!(
                    "duplicate top-level function `{}` in the same file",
                    func.name
                )));
            }
        }

        let mut package_functions = Vec::with_capacity(source.functions.len());
        for func in &source.functions {
            let compiled = compile_function(func, &source.structs, None, &package_fn_arity)?;
            package_functions.push(CompiledFunction {
                name: func.name.clone(),
                contract: None,
                instructions: compiled.instructions,
                call_patches: compiled.call_patches,
            });
        }

        let mut struct_methods = Vec::new();
        for struct_decl in &source.structs {
            for method in &struct_decl.methods {
                let lowered = lower_struct_method(&struct_decl.name, method);
                let compiled =
                    compile_function(&lowered, &source.structs, None, &package_fn_arity)?;
                struct_methods.push(CompiledFunction {
                    name: lowered.name.clone(),
                    contract: None,
                    instructions: compiled.instructions,
                    call_patches: compiled.call_patches,
                });
            }
        }

        let mut contract_methods = Vec::new();
        if let Some(contract_decl) = &source.contract {
            for member in &contract_decl.members {
                if let ContractMember::Function(method) = member {
                    let compiled = compile_function(
                        method,
                        &source.structs,
                        storage_fields,
                        &package_fn_arity,
                    )?;
                    contract_methods.push(CompiledFunction {
                        name: method.name.clone(),
                        contract: Some(contract_decl.name.clone()),
                        instructions: compiled.instructions,
                        call_patches: compiled.call_patches,
                    });
                }
            }
        }

        let mut compiled = CompiledSourceFile {
            package_functions,
            struct_methods,
            contract_methods,
        };
        compiled.optimize();
        compiled.link_call_l_patches()?;
        Ok(compiled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parser::parse_source_file;
    use crate::target::opcode::OpCode;

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
}
