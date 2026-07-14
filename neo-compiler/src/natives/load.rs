//! Load `.d.neo` files into [`ExternContract`] values and the built-in registry.

use std::collections::HashMap;
use std::sync::OnceLock;

use thiserror::Error;

use crate::stdlib::builtin::StructScopeError;
use crate::stdlib::StructScope;
use crate::syntax::ast::{Attribute, DeclFile, ExternContractDecl, ExternMethodDecl, Type};
use crate::syntax::parser::{parse_decl_file, ParseError};
use crate::target::syscall::CallFlags;

#[derive(Debug, Error)]
pub enum ExternContractError {
    #[error("parse error at line {line}: {message}")]
    Parse { line: usize, message: String },
    #[error("{0}")]
    Message(String),
}

impl From<StructScopeError> for ExternContractError {
    fn from(value: StructScopeError) -> Self {
        Self::Message(value.to_string())
    }
}

impl From<ParseError> for ExternContractError {
    fn from(value: ParseError) -> Self {
        Self::Parse {
            line: value.line,
            message: value.message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternMethod {
    pub name: String,
    pub params: Vec<(Type, String)>,
    pub return_ty: Type,
}

impl ExternMethod {
    pub fn from_decl(decl: &ExternMethodDecl) -> Self {
        Self {
            name: decl.name.clone(),
            params: decl
                .params
                .iter()
                .map(|param| (param.ty.clone(), param.name.clone()))
                .collect(),
            return_ty: decl.return_ty.clone(),
        }
    }

    pub fn leaves_stack_value(&self) -> bool {
        !matches!(self.return_ty, Type::Void)
    }

    pub fn arg_type_matches(&self, index: usize, ty: &Type) -> bool {
        lang_type_assignable_to(ty, &self.params[index].0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternContract {
    pub name: String,
    pub hash: [u8; 20],
    pub methods: Vec<ExternMethod>,
    pub call_flags: u8,
}

impl ExternContract {
    pub fn from_decl(decl: &ExternContractDecl) -> Result<Self, ExternContractError> {
        let hash = hash160_from_attributes(&decl.attributes)?;
        let call_flags =
            call_flags_from_attributes(&decl.attributes).unwrap_or(CallFlags::All as u8);
        Ok(Self {
            name: decl.name.clone(),
            hash,
            methods: decl.methods.iter().map(ExternMethod::from_decl).collect(),
            call_flags,
        })
    }

    pub fn resolve_method(&self, method: &str, arg_count: usize) -> Option<&ExternMethod> {
        self.methods
            .iter()
            .find(|m| m.name == method && m.params.len() == arg_count)
    }

    pub fn infer_return_type(&self, method: &str, arg_count: usize) -> Type {
        self.resolve_method(method, arg_count)
            .map(|m| m.return_ty.clone())
            .unwrap_or(Type::Any)
    }
}

/// Whether `from` can be passed where `to` is expected (neo-lang assignability).
pub fn lang_type_assignable_to(from: &Type, to: &Type) -> bool {
    if from.can_assign_to(to) {
        return true;
    }
    // hash160/hash256/buffer are passed as VM byte strings to external contracts.
    matches!(
        (from, to),
        (Type::String, Type::Hash160 | Type::Hash256)
            | (Type::Hash160 | Type::Hash256 | Type::Buffer, Type::String)
            | (Type::Hash160, Type::Hash256)
            | (Type::Hash256, Type::Hash160)
    )
}

pub fn parse_hash160_hex(raw: &str) -> Result<[u8; 20], ExternContractError> {
    let hex_str = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    if hex_str.len() != 40 {
        return Err(ExternContractError::Message(format!(
            "hash160 literal must be 20 bytes (40 hex digits), got `{raw}`"
        )));
    }
    let bytes = hex::decode(hex_str)
        .map_err(|e| ExternContractError::Message(format!("invalid hash160 hex `{raw}`: {e}")))?;
    let mut hash = [0u8; 20];
    hash.copy_from_slice(&bytes);
    Ok(hash)
}

fn hash160_from_attributes(attrs: &[Attribute]) -> Result<[u8; 20], ExternContractError> {
    for attr in attrs {
        if attr.name == "hash160" {
            let Some(hex) = attr.args.first() else {
                return Err(ExternContractError::Message(
                    "#[hash160(...)] requires one string argument".into(),
                ));
            };
            return parse_hash160_hex(hex);
        }
    }
    Err(ExternContractError::Message(
        "declare contract requires #[hash160(\"0x...\")] attribute".into(),
    ))
}

fn call_flags_from_attributes(attrs: &[Attribute]) -> Option<u8> {
    for attr in attrs {
        if attr.name != "call_flags" {
            continue;
        }
        let flag = attr.args.first()?;
        return Some(match flag.as_str() {
            "All" => CallFlags::All as u8,
            "ReadOnly" => CallFlags::ReadOnly as u8,
            "AllowCall" => CallFlags::AllowCall as u8,
            "AllowNotify" => CallFlags::AllowNotify as u8,
            other => {
                if let Ok(value) = other.parse::<u8>() {
                    value
                } else {
                    CallFlags::All as u8
                }
            }
        });
    }
    None
}

pub fn load_decl_file(src: &str) -> Result<Vec<ExternContract>, ExternContractError> {
    let decl_file = parse_decl_file(src)?;
    load_decl_ast(&decl_file)
}

pub fn load_decl_ast(decl_file: &DeclFile) -> Result<Vec<ExternContract>, ExternContractError> {
    let struct_scope = StructScope::from_decl_structs(&decl_file.structs)?;
    let struct_index = struct_scope.index();
    decl_file
        .contracts
        .iter()
        .map(|decl| {
            validate_extern_contract_types(&struct_index, decl)?;
            ExternContract::from_decl(decl)
        })
        .collect()
}

fn validate_extern_contract_types(
    structs: &std::collections::HashMap<String, &crate::syntax::ast::StructDecl>,
    decl: &ExternContractDecl,
) -> Result<(), ExternContractError> {
    for method in &decl.methods {
        validate_extern_type(structs, &method.return_ty, "return type")?;
        for param in &method.params {
            validate_extern_type(structs, &param.ty, "parameter")?;
        }
    }
    Ok(())
}

fn validate_extern_type(
    structs: &std::collections::HashMap<String, &crate::syntax::ast::StructDecl>,
    ty: &Type,
    context: &str,
) -> Result<(), ExternContractError> {
    match ty {
        Type::Named(name) => {
            if !structs.contains_key(name) {
                return Err(ExternContractError::Message(format!(
                    "unknown struct type `{name}` in extern contract {context}"
                )));
            }
        }
        Type::Array(inner) => validate_extern_type(structs, inner, context)?,
        Type::Map { key, value } => {
            validate_extern_type(structs, key, context)?;
            validate_extern_type(structs, value, context)?;
        }
        _ => {}
    }
    Ok(())
}

static BUILTIN_CONTRACTS: OnceLock<HashMap<String, ExternContract>> = OnceLock::new();

const BUILTIN_DECL_SOURCES: &[(&str, &str)] = &[
    ("neo", include_str!("../../../std/native/neo.d.neo")),
    ("gas", include_str!("../../../std/native/gas.d.neo")),
    ("stdlib", include_str!("../../../std/native/stdlib.d.neo")),
    (
        "contract_management",
        include_str!("../../../std/native/contract_management.d.neo"),
    ),
    (
        "cryptolib",
        include_str!("../../../std/native/cryptolib.d.neo"),
    ),
    ("ledger", include_str!("../../../std/native/ledger.d.neo")),
    ("policy", include_str!("../../../std/native/policy.d.neo")),
    ("oracle", include_str!("../../../std/native/oracle.d.neo")),
    ("notary", include_str!("../../../std/native/notary.d.neo")),
    (
        "role_management",
        include_str!("../../../std/native/role_management.d.neo"),
    ),
    (
        "treasury",
        include_str!("../../../std/native/treasury.d.neo"),
    ),
];

fn load_builtin_contracts() -> HashMap<String, ExternContract> {
    let mut contracts = HashMap::new();
    for (label, src) in BUILTIN_DECL_SOURCES {
        let loaded = load_decl_file(src)
            .unwrap_or_else(|e| panic!("failed to load built-in native contract `{label}`: {e}"));
        for contract in loaded {
            if contracts.insert(contract.name.clone(), contract).is_some() {
                panic!("duplicate built-in native contract name in `{label}`");
            }
        }
    }
    contracts
}

/// Look up a built-in or previously registered external contract by name.
///
/// Native contracts from `std/native/*.d.neo` are always available without
/// an explicit `import` (similar to Java's implicit `java.lang` import).
pub fn native_contract_by_name(name: &str) -> Option<&'static ExternContract> {
    BUILTIN_CONTRACTS
        .get_or_init(load_builtin_contracts)
        .get(name)
}
