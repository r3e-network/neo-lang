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
    let mut builder = Builder {
        blocks: BTreeMap::new(),
        current_block: BlockId(0),
        next_block: 0,
        next_value: 0,
        tmp_counter: 0,
        structs,
        contract_fields,
        package_fn_arity,
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
    Ok(builder.finish(entry))
}
