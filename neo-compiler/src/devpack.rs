//! Import binding for the in-workspace `neo-devpack` catalog.

use std::collections::HashMap;

use neo_devpack::api::ApiCatalog;

use crate::syntax::ast::ImportDecl;
use crate::target::syscall::Syscall;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevPackModule {
    Runtime,
    Storage,
    Contract,
    Crypto,
    Iterator,
}

impl DevPackModule {
    fn from_catalog_name(name: &str) -> Option<Self> {
        Some(match name {
            "runtime" => Self::Runtime,
            "storage" => Self::Storage,
            "contract" => Self::Contract,
            "crypto" => Self::Crypto,
            "iterator" => Self::Iterator,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Storage => "storage",
            Self::Contract => "contract",
            Self::Crypto => "crypto",
            Self::Iterator => "iterator",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DevPackImports {
    aliases: HashMap<String, DevPackModule>,
}

impl DevPackImports {
    pub fn from_imports(imports: &[ImportDecl]) -> Result<Self, String> {
        let mut devpack_imports = Self::default();
        for import in imports {
            let Some(module_name) = module_name_for_import(import) else {
                continue;
            };
            let module = resolve_module(module_name)?;
            if let Some(previous) = devpack_imports.aliases.insert(import.name.clone(), module) {
                return Err(format!(
                    "duplicate neo-devpack import alias `{}` for `{}` and `{}`",
                    import.name,
                    previous.as_str(),
                    module.as_str()
                ));
            }
        }
        Ok(devpack_imports)
    }

    pub fn is_runtime_alias(&self, name: &str) -> bool {
        name == "runtime" || self.aliases.get(name) == Some(&DevPackModule::Runtime)
    }

    pub fn module_for_alias(&self, name: &str) -> Option<DevPackModule> {
        self.aliases.get(name).copied()
    }
}

fn module_name_for_import(import: &ImportDecl) -> Option<&str> {
    if import.library == "neo-devpack" {
        Some(import.name.as_str())
    } else {
        import.library.strip_prefix("neo-devpack/")
    }
}

fn resolve_module(module_name: &str) -> Result<DevPackModule, String> {
    let catalog = ApiCatalog::neo_n3();
    if catalog.module(module_name).is_none() {
        return Err(format!("unknown neo-devpack module `{module_name}`"));
    }
    DevPackModule::from_catalog_name(module_name).ok_or_else(|| {
        format!("neo-devpack module `{module_name}` is not supported by neo-compiler yet")
    })
}

pub fn syscall_for_module_method(module: DevPackModule, method: &str) -> Option<Syscall> {
    Some(match (module, method) {
        (DevPackModule::Storage, "getContext") => Syscall::STORAGE_GET_CONTEXT,
        (DevPackModule::Storage, "getReadOnlyContext") => Syscall::STORAGE_GET_READ_ONLY_CONTEXT,
        (DevPackModule::Storage, "asReadOnly") => Syscall::STORAGE_AS_READ_ONLY,
        (DevPackModule::Storage, "get") => Syscall::STORAGE_GET,
        (DevPackModule::Storage, "put") => Syscall::STORAGE_PUT,
        (DevPackModule::Storage, "delete") => Syscall::STORAGE_DELETE,
        (DevPackModule::Storage, "find") => Syscall::STORAGE_FIND,
        (DevPackModule::Storage, "localGet") => Syscall::STORAGE_LOCAL_GET,
        (DevPackModule::Storage, "localPut") => Syscall::STORAGE_LOCAL_PUT,
        (DevPackModule::Storage, "localDelete") => Syscall::STORAGE_LOCAL_DELETE,
        (DevPackModule::Storage, "localFind") => Syscall::STORAGE_LOCAL_FIND,
        (DevPackModule::Contract, "call") => Syscall::CONTRACT_CALL,
        (DevPackModule::Contract, "getCallFlags") => Syscall::CONTRACT_GET_CALL_FLAGS,
        (DevPackModule::Contract, "createStandardAccount") => {
            Syscall::CONTRACT_CREATE_STANDARD_ACCOUNT
        }
        (DevPackModule::Contract, "createMultisigAccount") => {
            Syscall::CONTRACT_CREATE_MULTISIG_ACCOUNT
        }
        (DevPackModule::Crypto, "checkSig") => Syscall::CRYPTO_CHECK_SIG,
        (DevPackModule::Crypto, "checkMultisig") => Syscall::CRYPTO_CHECK_MULTISIG,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(name: &str, library: &str) -> ImportDecl {
        ImportDecl {
            name: name.to_string(),
            library: library.to_string(),
        }
    }

    #[test]
    fn resolves_module_alias_from_subpath_import() {
        let imports = DevPackImports::from_imports(&[import("rt", "neo-devpack/runtime")])
            .expect("devpack imports");
        assert!(imports.is_runtime_alias("rt"));
        assert!(imports.is_runtime_alias("runtime"));
    }

    #[test]
    fn resolves_module_name_from_root_import() {
        let imports = DevPackImports::from_imports(&[import("storage", "neo-devpack")])
            .expect("devpack imports");
        assert_eq!(
            imports.module_for_alias("storage"),
            Some(DevPackModule::Storage)
        );
    }

    #[test]
    fn maps_supported_framework_methods_to_syscalls() {
        assert_eq!(
            syscall_for_module_method(DevPackModule::Storage, "localGet"),
            Some(Syscall::STORAGE_LOCAL_GET)
        );
        assert_eq!(
            syscall_for_module_method(DevPackModule::Contract, "getCallFlags"),
            Some(Syscall::CONTRACT_GET_CALL_FLAGS)
        );
        assert_eq!(
            syscall_for_module_method(DevPackModule::Crypto, "checkSig"),
            Some(Syscall::CRYPTO_CHECK_SIG)
        );
        assert_eq!(
            syscall_for_module_method(DevPackModule::Iterator, "next"),
            None
        );
    }
}
