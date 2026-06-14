//! Type checking runs inside [`super::Codegen::codegen_source_file`] before lowering.
//!
//! Expression types must match declarations and operators (strong typing).

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use thiserror::Error;

use crate::syntax::ast::*;
use crate::target::builtin::BuiltinMethod;
use crate::target::natives::{native_contract_by_name, NativeContract};
use crate::target::syscall::RuntimeMethod;

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("type-error: {0}")]
    Message(String),
}

#[inline]
fn err(s: impl std::fmt::Display) -> TypeError {
    TypeError::Message(s.to_string())
}

impl SourceFile {
    pub(crate) fn type_check(&self) -> Result<(), TypeError> {
        let mut structs: HashMap<String, &StructDecl> = HashMap::new();
        for struct_decl in &self.structs {
            if structs
                .insert(struct_decl.name.clone(), struct_decl)
                .is_some()
            {
                return Err(err(format!("duplicate struct `{}`", struct_decl.name)));
            }
        }

        let mut package_fns: HashMap<String, &FunctionDecl> = HashMap::new();
        for func in &self.functions {
            if package_fns.insert(func.name.clone(), func).is_some() {
                return Err(err(format!(
                    "duplicate top-level function `{}` in the same file",
                    func.name
                )));
            }
        }

        let mut events: HashMap<String, &EventDecl> = HashMap::new();
        let contract_field_storage: Vec<ContractField> = self
            .contract
            .as_ref()
            .map(|contract_decl| {
                for member in &contract_decl.members {
                    if let ContractMember::Event(event) = member {
                        events.insert(event.name.clone(), event);
                    }
                }
                contract_decl
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        ContractMember::Field(field) => Some(field.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        for field in &contract_field_storage {
            if field.ty.is_array() {
                return Err(err("contract cannot have array fields"));
            }
        }

        let contract_fields = contract_field_storage.as_slice();
        let contract_fns: HashMap<String, &FunctionDecl> = self
            .contract
            .as_ref()
            .map(|contract_decl| {
                contract_decl
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        ContractMember::Function(func) => Some((func.name.clone(), func)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ctx = TypeCheckContext {
            structs: &structs,
            package_fns: &package_fns,
            events: &events,
            contract_fields,
            contract_fns: &contract_fns,
        };

        self.check_source_file_map_types()?;

        for func in &self.functions {
            ctx.check_function(func, FnType::Package)?;
        }

        for struct_decl in &self.structs {
            for method in &struct_decl.methods {
                ctx.check_function(
                    method,
                    FnType::StructMethod {
                        struct_name: struct_decl.name.clone(),
                    },
                )?;
            }
        }

        if let Some(contract_decl) = &self.contract {
            let contract_name = contract_decl.name.clone();
            for member in &contract_decl.members {
                if let ContractMember::Function(func) = member {
                    ctx.check_function(
                        func,
                        FnType::ContractMethod {
                            contract_name: contract_name.clone(),
                        },
                    )?;
                }
            }
        }

        Ok(())
    }

    fn check_source_file_map_types(&self) -> Result<(), TypeError> {
        for func in &self.functions {
            check_map_key_rules_in_type(&func.return_ty)?;
            for param in &func.params {
                check_map_key_rules_in_type(&param.ty)?;
            }
        }
        for struct_decl in &self.structs {
            for field in &struct_decl.fields {
                check_map_key_rules_in_type(&field.ty)?;
            }
            for method in &struct_decl.methods {
                check_map_key_rules_in_type(&method.return_ty)?;
                for param in &method.params {
                    check_map_key_rules_in_type(&param.ty)?;
                }
            }
        }
        if let Some(contract_decl) = &self.contract {
            for member in &contract_decl.members {
                match member {
                    ContractMember::ConstProp(prop) => check_map_key_rules_in_type(&prop.ty)?,
                    ContractMember::Field(field) => check_map_key_rules_in_type(&field.ty)?,
                    ContractMember::Event(event) => {
                        for param in &event.params {
                            check_map_key_rules_in_type(&param.ty)?;
                        }
                    }
                    ContractMember::Function(func) => {
                        check_map_key_rules_in_type(&func.return_ty)?;
                        for param in &func.params {
                            check_map_key_rules_in_type(&param.ty)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

struct TypeCheckContext<'a> {
    structs: &'a HashMap<String, &'a StructDecl>,
    package_fns: &'a HashMap<String, &'a FunctionDecl>,
    events: &'a HashMap<String, &'a EventDecl>,
    contract_fields: &'a [ContractField],
    contract_fns: &'a HashMap<String, &'a FunctionDecl>,
}

enum FnType {
    Package,
    StructMethod { struct_name: String },
    ContractMethod { contract_name: String },
}

struct FnEnv {
    scopes: Vec<HashMap<String, Type>>,

    /// Variable name → struct type name (for `var.field`)
    value_struct: HashMap<String, String>,

    /// Whether the function is a contract method.
    is_contract_fn: bool,
}

impl FnEnv {
    fn new(is_contract_fn: bool) -> Self {
        Self {
            scopes: vec![HashMap::new()],
            value_struct: HashMap::new(),
            is_contract_fn,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &str, ty: Type) -> Result<(), TypeError> {
        let top = self
            .scopes
            .last_mut()
            .ok_or_else(|| err("internal: scope stack empty"))?;
        if top.contains_key(name) {
            return Err(err(format!("duplicate local `{name}` in the same block")));
        }
        top.insert(name.to_string(), ty);
        Ok(())
    }

    fn resolve(&self, name: &str) -> Option<Type> {
        for map in self.scopes.iter().rev() {
            if let Some(t) = map.get(name) {
                return Some(t.clone());
            }
        }
        None
    }
}

/// Every `map[K, V]` in a type must use an allowed key type; recurse into `V` and array elements.
fn check_map_key_rules_in_type(ty: &Type) -> Result<(), TypeError> {
    match ty {
        Type::Map { key, value } => {
            if !key.is_valid_map_key_type() {
                return Err(err(format!(
                    "map key type must be bool, int, string, hash160, or hash256, got `{key:?}`"
                )));
            }
            check_map_key_rules_in_type(value)?;
        }
        Type::Array(el) => check_map_key_rules_in_type(el)?,
        _ => {}
    }
    Ok(())
}

impl<'a> TypeCheckContext<'a> {
    fn check_function(&self, func: &FunctionDecl, fn_type: FnType) -> Result<(), TypeError> {
        let is_contract_fn = matches!(fn_type, FnType::ContractMethod { .. });
        let mut env = FnEnv::new(is_contract_fn);
        match &fn_type {
            FnType::Package => {}
            FnType::StructMethod { struct_name } => {
                env.declare("self", Type::Named(struct_name.clone()))?;
                env.value_struct.insert("self".into(), struct_name.clone());
            }
            FnType::ContractMethod { contract_name } => {
                env.declare("self", Type::Named(contract_name.clone()))?;
            }
        }

        for p in &func.params {
            env.declare(&p.name, p.ty.clone())?;
            if let Type::Named(sn) = &p.ty {
                env.value_struct.insert(p.name.clone(), sn.clone());
            }
        }

        self.check_block(&mut env, &func.body, &func.return_ty)?;

        Ok(())
    }

    fn check_block(
        &self,
        env: &mut FnEnv,
        block: &Block,
        return_ty: &Type,
    ) -> Result<(), TypeError> {
        env.push_scope();
        for stmt in &block.stmts {
            self.check_stmt(env, stmt, return_ty)?;
        }
        env.pop_scope();
        Ok(())
    }

    fn check_stmt(&self, env: &mut FnEnv, stmt: &Stmt, return_ty: &Type) -> Result<(), TypeError> {
        match stmt {
            Stmt::Var { name, init } => {
                let ty = if let Some(expr) = init {
                    let ty = self.infer_expr(env, expr)?;
                    if let Expr::StructLit {
                        name: struct_name, ..
                    } = expr
                    {
                        env.value_struct.insert(name.clone(), struct_name.clone());
                    }
                    ty
                } else {
                    Type::Any
                };
                env.declare(name, ty)?;
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.infer_expr(env, expr)?;
                Ok(())
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                let ty = self.infer_expr(env, cond)?;
                if ty != Type::Bool {
                    return Err(err(format!("`if` condition must be bool, got `{ty:?}`")));
                }
                self.check_block(env, then_block, return_ty)?;
                if let Some(else_block) = else_block {
                    self.check_block(env, else_block, return_ty)?;
                }
                Ok(())
            }
            Stmt::While { cond, body } => {
                let ty = self.infer_expr(env, cond)?;
                if ty != Type::Bool {
                    return Err(err(format!("`while` condition must be bool, got `{ty:?}`")));
                }
                self.check_block(env, body, return_ty)?;
                Ok(())
            }
            Stmt::ForArray { item, iter, body } => {
                let iter_ty = self.infer_expr(env, iter)?;
                let elem_ty = match iter_ty {
                    Type::Array(ty) => *ty,
                    _ => {
                        return Err(err(format!(
                            "for-in-array expects an array, got `{iter_ty:?}`"
                        )));
                    }
                };
                env.push_scope();
                env.declare(item, elem_ty)?;
                self.check_block(env, body, return_ty)?;
                env.pop_scope();
                Ok(())
            }
            Stmt::ForMap {
                key,
                value,
                map,
                body,
            } => {
                let map_ty = self.infer_expr(env, map)?;
                let (key_ty, value_ty) = match map_ty {
                    Type::Map { key, value } => (*key, *value),
                    _ => {
                        return Err(err(format!("for-in-map expects a map, got `{map_ty:?}`")));
                    }
                };
                env.push_scope();
                env.declare(key, key_ty)?;
                env.declare(value, value_ty)?;
                self.check_block(env, body, return_ty)?;
                env.pop_scope();
                Ok(())
            }
            Stmt::Return(opt) => match opt {
                None => {
                    if !matches!(return_ty, Type::Void) {
                        Err(err(format!(
                            "missing return value (expected `{return_ty:?}`)"
                        )))
                    } else {
                        Ok(())
                    }
                }
                Some(expr) => {
                    if matches!(return_ty, Type::Void) {
                        return Err(err("void function must not return a value"));
                    }
                    let ty = self.infer_expr(env, expr)?;
                    if !ty.can_assign_to(return_ty) {
                        Err(err(format!(
                            "return type mismatch: expected `{return_ty:?}`, got `{ty:?}`"
                        )))
                    } else {
                        Ok(())
                    }
                }
            },
            Stmt::Emit { name, args } => {
                let event_decl = self
                    .events
                    .get(name)
                    .ok_or_else(|| err(format!("unknown event `{name}` for `emit`")))?;
                if args.len() != event_decl.params.len() {
                    return Err(err(format!(
                        "event `{name}` expects {} argument(s), got {}",
                        event_decl.params.len(),
                        args.len()
                    )));
                }
                for (expr, param) in args.iter().zip(event_decl.params.iter()) {
                    let ty = self.infer_expr(env, expr)?;
                    if !ty.can_assign_to(&param.ty) {
                        return Err(err(format!(
                        "`emit {name}` argument `{}` type mismatch: expected `{:?}`, got `{ty:?}`",
                        param.name, param.ty
                    )));
                    }
                }
                Ok(())
            }
            Stmt::Block(block) => self.check_block(env, block, return_ty),
        }
    }

    fn infer_expr(&self, env: &mut FnEnv, expr: &Expr) -> Result<Type, TypeError> {
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::Null => Ok(Type::Any),
                Literal::Bool(_) => Ok(Type::Bool),
                Literal::Int(_) => Ok(Type::Int),
                Literal::String(_) => Ok(Type::String),
                Literal::Buffer(_) => Ok(Type::Buffer),
            },
            Expr::Ident(name) => env
                .resolve(name)
                .ok_or_else(|| err(format!("unknown variable or parameter `{name}`"))),
            Expr::Self_ => Err(err(
                "`self` cannot stand alone; use `self.field` or `self.method(...)` instead",
            )),
            Expr::Cast { expr: inner, ty } => {
                self.infer_expr(env, inner)?;
                Ok(ty.clone())
            }
            Expr::Paren(inner) => self.infer_expr(env, inner),
            Expr::Unary { op, expr: inner } => {
                let ty = self.infer_expr(env, inner)?;
                match op {
                    UnaryOp::Not => self.infer_expr_unary_not(ty),
                    UnaryOp::Positive | UnaryOp::Negative | UnaryOp::BitNot => {
                        self.infer_expr_unary_int(ty)
                    }
                }
            }
            Expr::Binary { op, left, right } => {
                if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                    if left.is_null_literal() {
                        return self.infer_expr(env, right).map(|_| Type::Bool);
                    }
                    if right.is_null_literal() {
                        return self.infer_expr(env, left).map(|_| Type::Bool);
                    }
                }
                let lt = self.infer_expr(env, left)?;
                let rt = self.infer_expr(env, right)?;
                match op {
                    BinaryOp::And | BinaryOp::Or => self.infer_expr_binary_logical(lt, rt),
                    BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge => self.infer_expr_binary_compare(lt, rt),
                    BinaryOp::Add => self.infer_expr_binary_add(lt, rt),
                    BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod
                    | BinaryOp::Sub
                    | BinaryOp::Shl
                    | BinaryOp::Shr
                    | BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor => self.infer_expr_binary_arith(lt, rt),
                }
            }
            Expr::Member { base, field } => match base.as_ref() {
                Expr::Ident(var) => self.infer_expr_member_ident(env, var, field),
                Expr::Self_ => self.infer_expr_member_self(env, field),
                _ => Err(err(
                    "only `variable.field` or `self.field` member access is allowed",
                )),
            },
            Expr::Index { base, index } => self.infer_expr_index(env, base, index),
            Expr::StructLit { name, fields } => self.infer_expr_struct_lit(env, name, fields),
            Expr::MapLit { ty, pairs } => self.infer_expr_map_lit(env, ty, pairs),
            Expr::ArrayLit { ty, elements } => self.infer_expr_array_lit(env, ty, elements),
            Expr::Assign { target, op, value } => self.infer_expr_assign(env, target, *op, value),
            Expr::Call { callee, args } => self.check_call(env, callee, args),
        }
    }

    fn infer_expr_unary_not(&self, ty: Type) -> Result<Type, TypeError> {
        if ty != Type::Bool {
            return Err(err(format!("`!` expects bool, got `{ty:?}`")));
        }
        Ok(Type::Bool)
    }

    fn infer_expr_unary_int(&self, ty: Type) -> Result<Type, TypeError> {
        if ty != Type::Int {
            return Err(err(format!("unary op expects int, got `{ty:?}`")));
        }
        Ok(Type::Int)
    }

    fn infer_expr_binary_logical(&self, lt: Type, rt: Type) -> Result<Type, TypeError> {
        if lt != Type::Bool || rt != Type::Bool {
            return Err(err(format!(
                "logical op expects bool operands, got `{lt:?}` and `{rt:?}`"
            )));
        }
        Ok(Type::Bool)
    }

    fn infer_expr_binary_compare(&self, lt: Type, rt: Type) -> Result<Type, TypeError> {
        let ok = lt == rt && lt.is_primitive();
        if !ok {
            return Err(err(format!(
                "comparison requires matching primitive types, got `{lt:?}` and `{rt:?}`"
            )));
        }
        Ok(Type::Bool)
    }

    fn infer_expr_binary_add(&self, lt: Type, rt: Type) -> Result<Type, TypeError> {
        if lt == Type::Int && rt == Type::Int {
            Ok(Type::Int)
        } else if lt == Type::String && rt == Type::String {
            Ok(Type::String)
        } else {
            Err(err(format!(
                "`+` expects int+int or string+string, got `{lt:?}` and `{rt:?}`"
            )))
        }
    }

    fn infer_expr_binary_arith(&self, lt: Type, rt: Type) -> Result<Type, TypeError> {
        if lt != Type::Int || rt != Type::Int {
            return Err(err(format!(
                "arithmetic op expects int operands, got `{lt:?}` and `{rt:?}`"
            )));
        }
        Ok(Type::Int)
    }

    fn infer_expr_member_ident(
        &self,
        env: &FnEnv,
        var: &str,
        field: &str,
    ) -> Result<Type, TypeError> {
        let struct_name = env
            .value_struct
            .get(var)
            .ok_or_else(|| err("member access needs a variable with struct type"))?;
        let struct_decl = self
            .structs
            .get(struct_name)
            .ok_or_else(|| err(format!("unknown struct type `{struct_name}`")))?;
        let struct_field = struct_decl
            .fields
            .iter()
            .find(|f| f.name == field)
            .ok_or_else(|| err(format!("struct `{struct_name}` has no field `{field}`")))?;
        Ok(struct_field.ty.clone())
    }

    fn infer_expr_member_self(&self, env: &FnEnv, field: &str) -> Result<Type, TypeError> {
        if env.is_contract_fn {
            if !self.contract_fields.is_empty() {
                if let Some(cf) = self.contract_fields.iter().find(|f| f.name == field) {
                    if cf.ty.is_map() {
                        return Err(err(format!(
                            "use `self.{field}[key]` for contract map fields (whole-field load is not supported)"
                        )));
                    }
                    return Ok(cf.ty.clone());
                }
            }
        }
        let struct_name = env.value_struct.get("self").ok_or_else(|| {
            err("`self.member` needs a contract field or struct `self` parameter")
        })?;
        let struct_decl = self
            .structs
            .get(struct_name)
            .ok_or_else(|| err(format!("unknown struct type `{struct_name}`")))?;
        let struct_field = struct_decl
            .fields
            .iter()
            .find(|f| f.name == field)
            .ok_or_else(|| err(format!("struct `{struct_name}` has no field `{field}`")))?;
        Ok(struct_field.ty.clone())
    }

    fn infer_expr_index_array_elem(&self, index_ty: Type, elem: Type) -> Result<Type, TypeError> {
        if index_ty != Type::Int {
            return Err(err(format!("array index must be int, got `{index_ty:?}`")));
        }
        Ok(elem)
    }

    fn infer_expr_index_map_value(
        &self,
        index_ty: Type,
        key: Type,
        value: Type,
    ) -> Result<Type, TypeError> {
        if !index_ty.can_assign_to(&key) {
            return Err(err(format!(
                "map index type mismatch: expected `{key:?}`, got `{index_ty:?}`"
            )));
        }
        Ok(value)
    }

    fn infer_expr_index(
        &self,
        env: &mut FnEnv,
        base: &Expr,
        index: &Expr,
    ) -> Result<Type, TypeError> {
        if env.is_contract_fn {
            if let Some((key_ty, val_ty)) = self.try_contract_self_map_types(base)? {
                let index_ty = self.infer_expr(env, index)?;
                if !index_ty.can_assign_to(&key_ty) {
                    return Err(err(format!(
                        "map index type mismatch: expected `{key_ty:?}`, got `{index_ty:?}`"
                    )));
                }
                return Ok(val_ty);
            }
        }
        let base_ty = self.infer_expr(env, base)?;
        let index_ty = self.infer_expr(env, index)?;
        match base_ty {
            Type::Array(elem) => self.infer_expr_index_array_elem(index_ty, *elem),
            Type::Map { key, value } => self.infer_expr_index_map_value(index_ty, *key, *value),
            _ => Err(err(format!(
                "indexing requires array or map, got `{base_ty:?}`"
            ))),
        }
    }

    fn infer_expr_struct_lit(
        &self,
        env: &mut FnEnv,
        name: &str,
        fields: &[(String, Expr)],
    ) -> Result<Type, TypeError> {
        let struct_decl = self
            .structs
            .get(name)
            .ok_or_else(|| err(format!("unknown struct `{name}` in struct literal")))?;
        for struct_field in &struct_decl.fields {
            let init = fields
                .iter()
                .find(|(n, _)| n == &struct_field.name)
                .map(|(_, expr)| expr)
                .or(struct_field.init.as_ref());
            if let Some(expr) = init {
                let ty = self.infer_expr(env, expr)?;
                if !ty.can_assign_to(&struct_field.ty) {
                    return Err(err(format!(
                        "field `{}` type mismatch: expected `{:?}`, got `{ty:?}`",
                        struct_field.name, struct_field.ty
                    )));
                }
            }
        }
        for (field_name, _) in fields {
            if !struct_decl.fields.iter().any(|f| f.name == *field_name) {
                return Err(err(format!("struct `{name}` has no field `{field_name}`")));
            }
        }
        Ok(Type::Named(name.to_string()))
    }

    fn infer_expr_map_lit(
        &self,
        env: &mut FnEnv,
        ty: &Type,
        pairs: &[(Expr, Expr)],
    ) -> Result<Type, TypeError> {
        check_map_key_rules_in_type(ty)?;
        let Type::Map { key, value } = ty else {
            return Err(err("internal: MapLit without map type"));
        };
        let key_ty = *key.clone();
        let value_ty = *value.clone();
        for (key_expr, value_expr) in pairs {
            let kt = self.infer_expr(env, key_expr)?;
            let vt = self.infer_expr(env, value_expr)?;
            if !kt.can_assign_to(&key_ty) {
                return Err(err(format!(
                    "map literal key type mismatch: expected `{key_ty:?}`, got `{kt:?}`"
                )));
            }
            if !vt.can_assign_to(&value_ty) {
                return Err(err(format!(
                    "map literal value type mismatch: expected `{value_ty:?}`, got `{vt:?}`"
                )));
            }
        }
        Ok(ty.clone())
    }

    fn infer_expr_array_lit(
        &self,
        env: &mut FnEnv,
        ty: &Type,
        elements: &[Expr],
    ) -> Result<Type, TypeError> {
        let Type::Array(elem) = ty else {
            return Err(err("internal: ArrayLit without array type"));
        };
        let elem_ty = *elem.clone();
        for expr in elements {
            let ty = self.infer_expr(env, expr)?;
            if !ty.can_assign_to(&elem_ty) {
                return Err(err(format!(
                    "array element type mismatch: expected `{elem_ty:?}`, got `{ty:?}`"
                )));
            }
        }
        Ok(ty.clone())
    }

    fn infer_expr_assign(
        &self,
        env: &mut FnEnv,
        target: &Expr,
        op: AssignOp,
        value: &Expr,
    ) -> Result<Type, TypeError> {
        let value_ty = self.infer_expr(env, value)?;
        let target_ty = self.infer_lvalue_type(env, target)?;
        if matches!(op, AssignOp::Assign) {
            if !value_ty.can_assign_to(&target_ty) {
                return Err(err(format!(
                    "assignment type mismatch: target `{target_ty:?}`, value `{value_ty:?}`"
                )));
            }
        } else if target_ty != Type::Int || value_ty != Type::Int {
            return Err(err(format!(
                "compound assignment expects int target and int value, got `{target_ty:?}` and `{value_ty:?}`"
            )));
        }
        Ok(value_ty)
    }

    fn try_contract_self_map_types(&self, base: &Expr) -> Result<Option<(Type, Type)>, TypeError> {
        let Expr::Member {
            base: inner,
            field: fname,
        } = base
        else {
            return Ok(None);
        };
        if !matches!(inner.as_ref(), Expr::Self_) {
            return Ok(None);
        }
        if self.contract_fields.is_empty() {
            return Ok(None);
        }
        let Some(cf) = self.contract_fields.iter().find(|f| f.name == *fname) else {
            return Ok(None);
        };
        match &cf.ty {
            Type::Map { key, value } => {
                Ok(Some(((*key.as_ref()).clone(), (*value.as_ref()).clone())))
            }
            _ => Err(err(format!(
                "only `map` contract fields support `[`index`]`; field `{fname}` is not a map"
            ))),
        }
    }

    fn infer_lvalue_type(&self, env: &mut FnEnv, target: &Expr) -> Result<Type, TypeError> {
        match target {
            Expr::Ident(name) => env
                .resolve(name)
                .ok_or_else(|| err(format!("unknown assignment target `{name}`"))),
            Expr::Member { base, field } => match base.as_ref() {
                Expr::Ident(var) => {
                    let struct_name = env.value_struct.get(var).cloned().ok_or_else(|| {
                        err("member assignment needs a variable with struct type")
                    })?;
                    let struct_decl = self
                        .structs
                        .get(&struct_name)
                        .ok_or_else(|| err(format!("unknown struct type `{struct_name}`")))?;
                    let struct_field = struct_decl
                        .fields
                        .iter()
                        .find(|f| f.name == *field)
                        .ok_or_else(|| {
                            err(format!("struct `{struct_name}` has no field `{field}`"))
                        })?;
                    Ok(struct_field.ty.clone())
                }
                Expr::Self_ => {
                    if env.is_contract_fn && !self.contract_fields.is_empty() {
                        if let Some(contract_field) =
                            self.contract_fields.iter().find(|f| f.name == *field)
                        {
                            if contract_field.ty.is_map() {
                                return Err(err(
                                    "cannot assign to a contract map field without `[key]`",
                                ));
                            }
                            return Ok(contract_field.ty.clone());
                        }
                    }
                    let struct_name = env.value_struct.get("self").cloned().ok_or_else(|| {
                    err("`self.member` assignment needs a contract field or struct `self` parameter")
                })?;
                    let struct_decl = self
                        .structs
                        .get(&struct_name)
                        .ok_or_else(|| err(format!("unknown struct type `{struct_name}`")))?;
                    let struct_field = struct_decl
                        .fields
                        .iter()
                        .find(|f| f.name == *field)
                        .ok_or_else(|| {
                            err(format!("struct `{struct_name}` has no field `{field}`"))
                        })?;
                    Ok(struct_field.ty.clone())
                }
                _ => Err(err(
                    "only `variable.field` or `self.field` member assignment is allowed",
                )),
            },
            Expr::Index { base, index } => {
                if env.is_contract_fn {
                    if let Some((key_ty, val_ty)) =
                        self.try_contract_self_map_types(base.as_ref())?
                    {
                        let index_ty = self.infer_expr(env, index)?;
                        if !index_ty.can_assign_to(&key_ty) {
                            return Err(err(format!("map index type mismatch: expected `{key_ty:?}`, got `{index_ty:?}`")));
                        }
                        return Ok(val_ty);
                    }
                }
                let base_ty = self.infer_expr(env, base)?;
                let index_ty = self.infer_expr(env, index)?;
                match base_ty {
                    Type::Array(elem) => {
                        if index_ty != Type::Int {
                            return Err(err("array index must be int"));
                        }
                        Ok(*elem)
                    }
                    Type::Map { key, value } => {
                        if !index_ty.can_assign_to(&key) {
                            return Err(err("map index type mismatch"));
                        }
                        Ok(*value)
                    }
                    _ => Err(err("invalid index assignment target")),
                }
            }
            _ => Err(err("invalid assignment target")),
        }
    }

    fn check_contract_method_call(
        &self,
        env: &mut FnEnv,
        method: &str,
        args: &[Expr],
    ) -> Result<Type, TypeError> {
        let method_decl = self
            .contract_fns
            .get(method)
            .ok_or_else(|| err(format!("contract has no method `{method}`")))?;
        if args.len() != method_decl.params.len() {
            return Err(err(format!(
                "`self.{method}` expects {} argument(s), got {}",
                method_decl.params.len(),
                args.len()
            )));
        }
        for (expr, param) in args.iter().zip(&method_decl.params) {
            let ty = self.infer_expr(env, expr)?;
            if !ty.can_assign_to(&param.ty) {
                return Err(err(format!(
                    "argument `{}` to `self.{method}` type mismatch: expected `{:?}`, got `{ty:?}`",
                    param.name, param.ty
                )));
            }
        }
        Ok(method_decl.return_ty.clone())
    }

    fn check_call(&self, env: &mut FnEnv, callee: &Expr, args: &[Expr]) -> Result<Type, TypeError> {
        if let Expr::Member { base, field } = callee {
            if let Expr::Ident(pkg) = base.as_ref() {
                if pkg == "runtime" {
                    return self.check_runtime_call(field, args, env);
                }
                if let Some(contract) = native_contract_by_name(pkg) {
                    return self.check_native_contract_call(contract, field, args, env);
                }
            }
            if matches!(base.as_ref(), Expr::Self_) && env.is_contract_fn {
                return self.check_contract_method_call(env, field, args);
            }
            if let Some(ty) = self.check_builtin_method_call(env, base.as_ref(), field, args)? {
                return Ok(ty);
            }
            if let Expr::Ident(recv) = base.as_ref() {
                if let Some(struct_name) = env.value_struct.get(recv).cloned() {
                    let struct_decl = self
                        .structs
                        .get(&struct_name)
                        .ok_or_else(|| err(format!("unknown struct `{struct_name}`")))?;
                    let method = struct_decl
                        .methods
                        .iter()
                        .find(|m| m.name == *field)
                        .ok_or_else(|| {
                            err(format!("struct `{struct_name}` has no method `{field}`"))
                        })?;
                    if args.len() != method.params.len() {
                        return Err(err(format!(
                            "`{struct_name}::{field}` expects {} argument(s), got {}",
                            method.params.len(),
                            args.len()
                        )));
                    }
                    for (expr, param) in args.iter().zip(&method.params) {
                        let ty = self.infer_expr(env, expr)?;
                        if !ty.can_assign_to(&param.ty) {
                            return Err(err(format!(
                            "argument `{}` to `{struct_name}.{field}` type mismatch: expected `{:?}`, got `{ty:?}`",
                            param.name, param.ty
                        )));
                        }
                    }
                    return Ok(method.return_ty.clone());
                }
            }
        }

        if let Expr::Ident(name) = callee {
            if let Some(ty) = self.check_builtin_call(env, name, args)? {
                return Ok(ty);
            }
            if let Some(fn_decl) = self.package_fns.get(name) {
                if args.len() != fn_decl.params.len() {
                    return Err(err(format!(
                        "call to `{name}` expects {} argument(s), got {}",
                        fn_decl.params.len(),
                        args.len()
                    )));
                }
                for (expr, param) in args.iter().zip(&fn_decl.params) {
                    let ty = self.infer_expr(env, expr)?;
                    if !ty.can_assign_to(&param.ty) {
                        return Err(err(format!(
                        "argument `{}` to `{name}` type mismatch: expected `{:?}`, got `{ty:?}`",
                        param.name, param.ty
                    )));
                    }
                }
                return Ok(fn_decl.return_ty.clone());
            }
        }

        Err(err(
            "only package-level functions, built-in functions, native contracts, struct methods, and runtime.* calls are supported",
        ))
    }

    /// `self.<map>.has` / `self.<map>.remove` on a contract storage `map` field; otherwise [`None`].
    fn check_contract_storage_map_method(
        &self,
        key: &Type,
        method: &str,
        args: &[Expr],
        env: &mut FnEnv,
    ) -> Result<Option<Type>, TypeError> {
        let err_method =
            |msg: &str| -> TypeError { err(format!("built-in method `{method}`: {msg}")) };
        match method {
            "has" => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                if !t0.can_assign_to(key) {
                    return Err(err_method("type mismatch"));
                }
                Ok(Some(Type::Bool))
            }
            "remove" => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                if !t0.can_assign_to(key) {
                    return Err(err_method("type mismatch"));
                }
                Ok(Some(Type::Void))
            }
            _ => Err(err(format!("contract storage map does not support `{method}`(only `has`, `remove`, and index access)"))),
        }
    }

    fn check_builtin_method_call(
        &self,
        env: &mut FnEnv,
        receiver: &Expr,
        method: &str,
        args: &[Expr],
    ) -> Result<Option<Type>, TypeError> {
        if let Expr::Member { base, field } = receiver {
            if matches!(base.as_ref(), Expr::Self_) && env.is_contract_fn {
                let Some(cf) = self.contract_fields.iter().find(|f| f.name == *field) else {
                    return Err(err(format!("contract doesn't have field `{field}`")));
                };
                if let Type::Map { key, .. } = &cf.ty {
                    return self.check_contract_storage_map_method(&key, method, args, env);
                }
            }
        };

        let recv_ty = self.infer_expr(env, receiver)?;
        let err_method =
            |msg: &str| -> TypeError { err(format!("built-in method `{method}`: {msg}")) };
        match (&recv_ty, method) {
            (Type::String | Type::Hash160 | Type::Hash256, "size") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Int))
            }
            (Type::String | Type::Hash160 | Type::Hash256, "sub") => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                let t1 = self.infer_expr(env, &args[1])?;
                if t0 != Type::Int || t1 != Type::Int {
                    return Err(err_method("expects (int, int)"));
                }
                Ok(Some(recv_ty))
            }
            (Type::Buffer, "size") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Int))
            }
            (Type::Buffer, "sub") => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                let t1 = self.infer_expr(env, &args[1])?;
                if t0 != Type::Int || t1 != Type::Int {
                    return Err(err_method("expects (int, int)"));
                }
                Ok(Some(Type::Buffer))
            }
            (Type::Int, "sqrt") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Int))
            }
            (Type::Int, "modmul") | (Type::Int, "modpow") => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                let t1 = self.infer_expr(env, &args[1])?;
                if t0 != Type::Int || t1 != Type::Int {
                    return Err(err_method("expects (int, int)"));
                }
                Ok(Some(Type::Int))
            }
            (Type::Int, "within") => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                let t1 = self.infer_expr(env, &args[1])?;
                if t0 != Type::Int || t1 != Type::Int {
                    return Err(err_method("expects (int, int)"));
                }
                // NeoVM `WITHIN` returns bool (x in [a, b)).
                Ok(Some(Type::Bool))
            }
            (Type::Array(_), "size") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Int))
            }
            (Type::Array(elem), "push") => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                if !t0.can_assign_to(elem.as_ref()) {
                    return Err(err_method("type mismatch"));
                }
                Ok(Some(Type::Void))
            }
            (Type::Array(elem), "pop") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some((**elem).clone()))
            }
            (Type::Array(_), "clear") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Void))
            }
            (Type::Map { .. }, "size") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Int))
            }
            (Type::Map { key, .. }, "keys") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Array(key.clone())))
            }
            (Type::Map { value, .. }, "values") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Array(value.clone())))
            }
            (Type::Map { key, .. }, "has") => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                if !t0.can_assign_to(key.as_ref()) {
                    return Err(err_method("type mismatch"));
                }
                Ok(Some(Type::Bool))
            }
            (Type::Map { .. }, "clear") => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                Ok(Some(Type::Void))
            }
            (Type::Map { key, .. }, "remove") => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                let t0 = self.infer_expr(env, &args[0])?;
                if !t0.can_assign_to(key.as_ref()) {
                    return Err(err_method("type mismatch"));
                }
                Ok(Some(Type::Void))
            }
            _ => Ok(None),
        }
    }

    fn check_builtin_call(
        &self,
        env: &mut FnEnv,
        name: &str,
        args: &[Expr],
    ) -> Result<Option<Type>, TypeError> {
        let Some(builtin) = BuiltinMethod::resolve(name) else {
            return Ok(None);
        };
        if args.len() != builtin.source_arg_count() {
            return Err(err(format!(
                "`{name}` expects {} argument(s), got {}",
                builtin.source_arg_count(),
                args.len()
            )));
        }
        for (index, expr) in args.iter().enumerate() {
            let ty = self.infer_expr(env, expr)?;
            if !builtin.binding().arg_type_matches(index, &ty) {
                return Err(err(format!(
                    "`{name}` argument type mismatch: expected `{:?}`, got `{ty:?}`",
                    builtin.binding().source_arg_type(index)
                )));
            }
        }
        Ok(Some(builtin.return_lang_type()))
    }

    fn check_native_contract_call(
        &self,
        contract: &NativeContract,
        method: &str,
        args: &[Expr],
        env: &mut FnEnv,
    ) -> Result<Type, TypeError> {
        let Some(native_method) = contract.resolve_method(method, args.len()) else {
            return Err(err(format!(
                "{}.{method} is not a known native contract method with {} argument(s)",
                contract.name,
                args.len()
            )));
        };
        for (index, arg) in args.iter().enumerate() {
            let ty = self.infer_expr(env, arg)?;
            if !native_method.arg_type_matches(index, &ty) {
                return Err(err(format!(
                    "{}.{method} argument {} type mismatch: expected `{:?}`, got `{ty:?}`",
                    contract.name,
                    index + 1,
                    native_method.args[index],
                )));
            }
        }
        Ok(native_method.return_lang_type())
    }

    fn check_runtime_call(
        &self,
        method: &str,
        args: &[Expr],
        env: &mut FnEnv,
    ) -> Result<Type, TypeError> {
        let Some(binding) = RuntimeMethod::resolve(method) else {
            return Err(err(format!("runtime.{method} is not a known runtime API")));
        };
        if args.len() != binding.source_arg_count() {
            return Err(err(format!(
                "runtime.{method} expects {} argument(s), got {}",
                binding.source_arg_count(),
                args.len()
            )));
        }
        for (index, expr) in args.iter().enumerate() {
            let ty = self.infer_expr(env, expr)?;
            let sit = binding.binding().source_arg_type(index);
            if !sit.satisfies_lang_type(&ty) {
                return Err(err(format!(
                    "runtime.{method} argument type mismatch: expected `{sit:?}`, got `{ty:?}`"
                )));
            }
        }
        Ok(binding.return_lang_type())
    }
}
