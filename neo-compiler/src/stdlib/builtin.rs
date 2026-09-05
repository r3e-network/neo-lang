//! Built-in struct definitions from `std/*.neo` (implicitly available, like native contracts).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::syntax::ast::StructDecl;
use crate::syntax::parser::{parse_struct_file, ParseError};

#[derive(Debug, thiserror::Error)]
pub enum StructScopeError {
    #[error("parse error at line {line}: {message}")]
    Parse { line: usize, message: String },
    #[error("{0}")]
    Message(String),
}

impl From<ParseError> for StructScopeError {
    fn from(value: ParseError) -> Self {
        Self::Parse {
            line: value.line,
            message: value.message,
        }
    }
}

static BUILTIN_STRUCTS: OnceLock<Vec<StructDecl>> = OnceLock::new();

const BUILTIN_STRUCT_SOURCES: &[(&str, &str)] =
    &[("builtin", include_str!("../../../std/builtin.neo"))];

fn load_builtin_structs() -> Vec<StructDecl> {
    let mut structs = Vec::new();
    for (label, src) in BUILTIN_STRUCT_SOURCES {
        let loaded = parse_struct_file(src)
            .unwrap_or_else(|e| panic!("failed to load built-in struct `{label}`: {e:?}"));
        for s in loaded {
            if structs
                .iter()
                .any(|existing: &StructDecl| existing.name == s.name)
            {
                panic!("duplicate built-in struct `{}` in `{label}`", s.name);
            }
            structs.push(s);
        }
    }
    structs
}

/// All built-in std structs (loaded once, always available without `import`).
pub fn builtin_structs() -> &'static [StructDecl] {
    BUILTIN_STRUCTS.get_or_init(load_builtin_structs)
}

/// User structs from a source file merged with built-in std structs.
#[derive(Debug, Clone)]
pub struct StructScope {
    merged: Vec<StructDecl>,
}

impl StructScope {
    pub fn from_source_structs(user_structs: &[StructDecl]) -> Result<Self, StructScopeError> {
        let mut merged = builtin_structs().to_vec();
        for user in user_structs {
            if merged.iter().any(|b| b.name == user.name) {
                return Err(StructScopeError::Message(format!(
                    "struct `{}` conflicts with built-in std struct",
                    user.name
                )));
            }
            merged.push(user.clone());
        }
        Ok(Self { merged })
    }

    pub fn from_decl_structs(decl_structs: &[StructDecl]) -> Result<Self, StructScopeError> {
        let mut merged = builtin_structs().to_vec();
        for decl in decl_structs {
            if merged.iter().any(|b| b.name == decl.name) {
                return Err(StructScopeError::Message(format!(
                    "struct `{}` conflicts with built-in std struct",
                    decl.name
                )));
            }
            merged.push(decl.clone());
        }
        Ok(Self { merged })
    }

    pub fn slice(&self) -> &[StructDecl] {
        &self.merged
    }

    pub fn index(&self) -> HashMap<String, &StructDecl> {
        self.merged.iter().map(|s| (s.name.clone(), s)).collect()
    }

    pub fn is_struct_type(&self, name: &str) -> bool {
        self.merged.iter().any(|s| s.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::ast::Type;

    #[test]
    fn builtin_includes_transaction() {
        let tx = builtin_structs()
            .iter()
            .find(|s| s.name == "Transaction")
            .expect("Transaction");
        assert_eq!(tx.fields.len(), 8);
        assert_eq!(tx.fields[0].name, "hash");
        assert_eq!(tx.fields[0].ty, Type::Hash256);
    }

    #[test]
    fn user_struct_cannot_shadow_builtin() {
        let user = vec![StructDecl {
            name: "Transaction".into(),
            fields: vec![],
            methods: vec![],
        }];
        assert!(StructScope::from_source_structs(&user).is_err());
    }
}
