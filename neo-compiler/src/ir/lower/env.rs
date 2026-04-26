use std::collections::{HashMap, HashSet};

use crate::ir::ValueRef;

#[derive(Clone, Debug)]
pub struct Env {
    pub locals: HashMap<String, ValueRef>,
    pub declared: HashSet<String>,
    pub struct_vars: HashMap<String, String>,
}

impl Env {
    pub fn new() -> Self {
        Self {
            locals: HashMap::new(),
            declared: HashSet::new(),
            struct_vars: HashMap::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<ValueRef> {
        self.locals.get(name).copied()
    }

    pub fn set(&mut self, name: &str, v: ValueRef) {
        self.locals.insert(name.to_string(), v);
        self.declared.insert(name.to_string());
    }

    pub fn set_struct_var(&mut self, name: &str, struct_name: &str) {
        self.struct_vars
            .insert(name.to_string(), struct_name.to_string());
    }

    pub fn get_struct_var(&self, name: &str) -> Option<&str> {
        self.struct_vars.get(name).map(|s| s.as_str())
    }
}

