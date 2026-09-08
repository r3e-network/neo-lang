use std::collections::HashMap;

use crate::diagnostic::Span;
use crate::syntax::ast::{EventDecl, Expr, ExprSpanMap, FunctionDecl, StructDecl, Type};

use super::error::{err, err_at};

pub(super) struct TypeCheckContext<'a> {
    pub(super) structs: &'a HashMap<String, &'a StructDecl>,
    pub(super) package_fns: &'a HashMap<String, &'a FunctionDecl>,
    pub(super) events: &'a HashMap<String, &'a EventDecl>,
    pub(super) contract_fields: &'a [crate::syntax::ast::ContractField],
    pub(super) contract_fns: &'a HashMap<String, &'a FunctionDecl>,
}

pub(super) enum FnType {
    Package,
    StructMethod { struct_name: String },
    ContractMethod { contract_name: String },
}

pub(super) struct FnEnv<'a> {
    pub(super) scopes: Vec<HashMap<String, Type>>,
    pub(super) value_struct: HashMap<String, String>,
    pub(super) is_contract_fn: bool,
    pub(super) current_span: Span,
    pub(super) expr_spans: Option<&'a ExprSpanMap>,
}

impl<'a> FnEnv<'a> {
    pub(super) fn new(is_contract_fn: bool) -> Self {
        Self {
            scopes: vec![HashMap::new()],
            value_struct: HashMap::new(),
            is_contract_fn,
            current_span: Span::default(),
            expr_spans: None,
        }
    }

    pub(super) fn expr_span(&self, expr: &Expr) -> Span {
        self.expr_spans
            .and_then(|spans| spans.get(expr))
            .unwrap_or(self.current_span)
    }

    pub(super) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(super) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub(super) fn declare(
        &mut self,
        name: &str,
        ty: Type,
    ) -> Result<(), crate::typecheck::TypeError> {
        let top = self
            .scopes
            .last_mut()
            .ok_or_else(|| err("internal: scope stack empty"))?;
        if top.contains_key(name) {
            return Err(err_at(
                self.current_span,
                format!("duplicate local `{name}` in the same block"),
            ));
        }
        top.insert(name.to_string(), ty);
        Ok(())
    }

    pub(super) fn resolve(&self, name: &str) -> Option<Type> {
        for map in self.scopes.iter().rev() {
            if let Some(t) = map.get(name) {
                return Some(t.clone());
            }
        }
        None
    }
}
