//! Shared per-source-file context for lowering/compiling a neo-lang function.

use std::collections::HashMap;

use crate::syntax::ast::{ContractField, FunctionDecl, StructDecl, Type};

/// Arity and return type for a package function or contract method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSig {
    pub arity: usize,
    pub return_ty: Type,
}

impl FnSig {
    pub fn new(arity: usize, return_ty: Type) -> Self {
        Self { arity, return_ty }
    }

    pub fn from_function(func: &FunctionDecl) -> Self {
        Self {
            arity: func.params.len(),
            return_ty: func.return_ty.clone(),
        }
    }
}

/// Read-only tables and contract layout used by IR lowering and legacy codegen.
#[derive(Debug, Clone, Copy)]
pub struct FunctionCompileContext<'a> {
    pub structs: &'a [StructDecl],
    pub contract_fields: &'a [ContractField],
    pub package_fns: &'a HashMap<String, FnSig>,
    pub contract_name: Option<&'a str>,
    pub contract_fns: Option<&'a HashMap<String, FnSig>>,
}

impl<'a> FunctionCompileContext<'a> {
    pub fn new(structs: &'a [StructDecl], package_fns: &'a HashMap<String, FnSig>) -> Self {
        Self {
            structs,
            contract_fields: &[],
            package_fns,
            contract_name: None,
            contract_fns: None,
        }
    }

    pub fn with_contract(
        mut self,
        name: &'a str,
        fields: &'a [ContractField],
        fns: &'a HashMap<String, FnSig>,
    ) -> Self {
        self.contract_name = Some(name);
        self.contract_fields = fields;
        self.contract_fns = Some(fns);
        self
    }
}
