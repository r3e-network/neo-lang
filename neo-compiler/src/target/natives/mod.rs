//! Predefined Neo native contracts (`NEO`, `ContractManagement`, ...).

pub mod contract_management;
pub mod crypto;
pub mod gas;
pub mod ledger;
pub mod neo;
pub mod notary;
pub mod oracle;
pub mod policy;
pub mod role_management;
pub mod stdlib;
pub mod treasury;

use crate::syntax::ast::Type;
use crate::target::syscall::CallFlags;
use crate::target::StackItemType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeMethod {
    pub name: &'static str,
    pub args: &'static [StackItemType],
    pub return_type: Option<StackItemType>,
}

impl NativeMethod {
    pub const fn new(
        name: &'static str,
        args: &'static [StackItemType],
        return_type: Option<StackItemType>,
    ) -> Self {
        Self {
            name,
            args,
            return_type,
        }
    }

    pub fn return_lang_type(self) -> Type {
        match self.return_type {
            None => Type::Void,
            Some(sit) => sit.to_lang_type(),
        }
    }

    pub fn leaves_stack_value(self) -> bool {
        self.return_type.is_some()
    }

    pub fn arg_type_matches(self, index: usize, ty: &Type) -> bool {
        self.args[index].satisfies_lang_type(ty)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeContract {
    pub name: &'static str,
    pub hash: [u8; 20],
    pub methods: &'static [NativeMethod],
}

impl NativeContract {
    pub fn resolve_method(self, method: &str, arg_count: usize) -> Option<&'static NativeMethod> {
        self.methods
            .iter()
            .find(|m| m.name == method && m.args.len() == arg_count)
    }

    pub fn infer_return_type(self, method: &str, arg_count: usize) -> Type {
        self.resolve_method(method, arg_count)
            .map(|m| m.return_lang_type())
            .unwrap_or(Type::Any)
    }

    pub fn default_call_flags(self) -> u8 {
        CallFlags::All as u8
    }
}

const NATIVE_CONTRACTS: [&'static NativeContract; 11] = [
    &neo::NEO,
    &gas::GAS,
    &policy::POLICY,
    &crypto::CRYPTO_LIB,
    &stdlib::STD_LIB,
    &ledger::LEDGER,
    &notary::NOTARY,
    &contract_management::CONTRACT_MANAGEMENT,
    &role_management::ROLE_MANAGEMENT,
    &oracle::ORACLE,
    &treasury::TREASURY,
];

pub fn native_contract_by_name(name: &str) -> Option<&'static NativeContract> {
    NATIVE_CONTRACTS.iter().find(|c| c.name == name).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn native_contract_names_are_unique() {
        let mut seen = HashMap::new();
        for contract in NATIVE_CONTRACTS {
            assert!(
                seen.insert(contract.name, contract.hash).is_none(),
                "duplicate native contract `{}`",
                contract.name
            );
        }
    }

    #[test]
    fn stdlib_resolves_overloads_by_arg_count() {
        assert!(stdlib::STD_LIB.resolve_method("MemorySearch", 2).is_some());
        assert!(stdlib::STD_LIB.resolve_method("MemorySearch", 3).is_some());
        assert!(stdlib::STD_LIB.resolve_method("MemorySearch", 4).is_some());
        assert!(stdlib::STD_LIB.resolve_method("MemorySearch", 1).is_none());
    }
}
