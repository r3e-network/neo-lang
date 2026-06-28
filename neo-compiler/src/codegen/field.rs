//! Synthesized parameterless getters for public contract properties (see README.md).

use crate::syntax::ast::*;

/// One compiler-generated getter for a public contract property.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldGetterSpec {
    pub func: FunctionDecl,
    /// Const-property getters are side-effect free (manifest `safe`).
    pub is_pure: bool,
}

/// Public contract properties that expose a parameterless getter in the ABI.
///
/// Skips map fields (whole-map load is not supported) and private (`_` prefix) names.
pub fn field_getter_specs(contract: &ContractDecl) -> Vec<FieldGetterSpec> {
    let mut specs = Vec::new();
    for member in &contract.members {
        match member {
            ContractMember::ConstProp(prop) if !prop.name.starts_with('_') => {
                specs.push(FieldGetterSpec {
                    is_pure: true,
                    func: FunctionDecl {
                        attributes: vec![],
                        return_ty: prop.ty.clone(),
                        name: prop.name.clone(),
                        params: vec![],
                        body: Block {
                            stmts: vec![Stmt::Return(Some(prop.init.clone()))],
                        },
                    },
                });
            }
            ContractMember::Field(field)
                if !field.name.starts_with('_') && field.ty.is_primitive() =>
            {
                specs.push(FieldGetterSpec {
                    is_pure: false,
                    func: FunctionDecl {
                        attributes: vec![],
                        return_ty: field.ty.clone(),
                        name: field.name.clone(),
                        params: vec![],
                        body: Block {
                            stmts: vec![Stmt::Return(Some(Expr::Member {
                                base: Box::new(Expr::Self_),
                                field: field.name.clone(),
                            }))],
                        },
                    },
                });
            }
            _ => {}
        }
    }
    specs
}
