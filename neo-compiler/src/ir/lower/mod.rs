//! Lower neo-lang AST to SSA-form IR (block-parameter SSA).

mod builder;
mod builtin;
mod env;
mod expr;
mod helpers;
mod short_circuit;
mod stmt;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::devpack::DevPackImports;
use crate::ir::lower::env::Env;
use crate::ir::*;
use crate::syntax::ast::*;

pub use self::builder::Builder;
pub use self::helpers::LowerError;

pub fn lower_function_to_ir(
    func: &FunctionDecl,
    structs: &[StructDecl],
    contract_fields: Option<&[ContractField]>,
    package_fn_arity: &std::collections::HashMap<String, usize>,
) -> Result<FunctionIr, LowerError> {
    let devpack_imports = DevPackImports::default();
    lower_function_to_ir_with_devpack_imports(
        func,
        structs,
        contract_fields,
        package_fn_arity,
        &devpack_imports,
    )
}

pub fn lower_function_to_ir_with_devpack_imports(
    func: &FunctionDecl,
    structs: &[StructDecl],
    contract_fields: Option<&[ContractField]>,
    package_fn_arity: &std::collections::HashMap<String, usize>,
    devpack_imports: &DevPackImports,
) -> Result<FunctionIr, LowerError> {
    let mut builder = Builder {
        blocks: BTreeMap::new(),
        current_block: BlockId(0),
        next_block: 0,
        next_value: 0,
        tmp_counter: 0,
        structs,
        contract_fields,
        package_fn_arity,
        devpack_imports,
    };
    let entry = builder.new_block();
    builder.current_block = entry;

    let mut env = Env::new();
    {
        let bb = builder.blocks.get_mut(&entry).unwrap();
        for param in &func.params {
            bb.params.push(BlockParam {
                name: param.name.clone(),
                ty: PrimTy::Any,
            });
        }
    }

    for (index, param) in func.params.iter().enumerate() {
        let param_ref = ValueRef::Param(ParamId(index));
        let out = builder.new_value();
        builder.emit(out, Instr::Copy(param_ref));
        env.set(&param.name, ValueRef::Value(out));
        if let Type::Named(struct_name) = &param.ty {
            env.set_struct_var(&param.name, struct_name.as_str());
        }
    }

    builder.lower_block(&func.body, &mut env, &func.return_ty)?;
    let current = builder.current_block;
    if matches!(builder.blocks[&current].term, Terminator::Unset) {
        builder.set_term(current, Terminator::Return(None));
    }
    Ok(builder.finish(entry))
}
