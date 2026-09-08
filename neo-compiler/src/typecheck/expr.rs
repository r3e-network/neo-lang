use crate::natives::lang_type_assignable_to;
use crate::syntax::ast::{AssignOp, BinaryOp, Expr, Literal, Type, UnaryOp};

use super::context::{FnEnv, TypeCheckContext};
use super::error::{err_at, TypeError};
use super::map_key::check_map_key_rules_in_type;

impl<'a> TypeCheckContext<'a> {
    pub(super) fn infer_expr(&self, env: &mut FnEnv<'a>, expr: &Expr) -> Result<Type, TypeError> {
        let expr_span = env.expr_span(expr);
        let previous_span = env.current_span;
        env.current_span = expr_span;
        let result = match expr {
            Expr::Literal(lit) => match lit {
                Literal::Null => Ok(Type::Any),
                Literal::Bool(_) => Ok(Type::Bool),
                Literal::Int(_) => Ok(Type::Int),
                Literal::String(_) => Ok(Type::String),
                Literal::Buffer(_) => Ok(Type::Buffer),
            },
            Expr::Ident(name) => env.resolve(name).ok_or_else(|| {
                err_at(
                    env.current_span,
                    format!("unknown variable or parameter `{name}`"),
                )
            }),
            Expr::Self_ => Err(err_at(
                env.current_span,
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
                    UnaryOp::Not => self.infer_expr_unary_not(env.current_span, ty),
                    UnaryOp::Positive | UnaryOp::Negative | UnaryOp::BitNot => {
                        self.infer_expr_unary_int(env.current_span, ty)
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
                    BinaryOp::And | BinaryOp::Or => {
                        self.infer_expr_binary_logical(env.current_span, lt, rt)
                    }
                    BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge => self.infer_expr_binary_compare(env.current_span, lt, rt),
                    BinaryOp::Add => self.infer_expr_binary_add(env.current_span, lt, rt),
                    BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod
                    | BinaryOp::Sub
                    | BinaryOp::Shl
                    | BinaryOp::Shr
                    | BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor => self.infer_expr_binary_arith(env.current_span, lt, rt),
                }
            }
            Expr::Member { base, field } => match base.as_ref() {
                Expr::Ident(var) => self.infer_expr_member_ident(env, var, field),
                Expr::Self_ => self.infer_expr_member_self(env, field),
                _ => Err(err_at(
                    env.current_span,
                    "only `variable.field` or `self.field` member access is allowed",
                )),
            },
            Expr::Index { base, index } => self.infer_expr_index(env, base, index),
            Expr::StructLit { name, fields } => self.infer_expr_struct_lit(env, name, fields),
            Expr::MapLit { ty, pairs } => self.infer_expr_map_lit(env, ty, pairs),
            Expr::ArrayLit { ty, elements } => self.infer_expr_array_lit(env, ty, elements),
            Expr::Assign { target, op, value } => self.infer_expr_assign(env, target, *op, value),
            Expr::Call { callee, args } => self.check_call(env, callee, args),
        };
        env.current_span = previous_span;
        result
    }

    fn infer_expr_unary_not(
        &self,
        span: crate::diagnostic::Span,
        ty: Type,
    ) -> Result<Type, TypeError> {
        if ty != Type::Bool {
            return Err(err_at(span, format!("`!` expects bool, got `{ty}`")));
        }
        Ok(Type::Bool)
    }

    fn infer_expr_unary_int(
        &self,
        span: crate::diagnostic::Span,
        ty: Type,
    ) -> Result<Type, TypeError> {
        if ty != Type::Int {
            return Err(err_at(span, format!("unary op expects int, got `{ty}`")));
        }
        Ok(Type::Int)
    }

    fn infer_expr_binary_logical(
        &self,
        span: crate::diagnostic::Span,
        lt: Type,
        rt: Type,
    ) -> Result<Type, TypeError> {
        if lt != Type::Bool || rt != Type::Bool {
            return Err(err_at(
                span,
                format!("logical op expects bool operands, got `{lt}` and `{rt}`"),
            ));
        }
        Ok(Type::Bool)
    }

    fn infer_expr_binary_compare(
        &self,
        span: crate::diagnostic::Span,
        lt: Type,
        rt: Type,
    ) -> Result<Type, TypeError> {
        let ok = lt == rt && lt.is_primitive();
        if !ok {
            return Err(err_at(
                span,
                format!("comparison requires matching primitive types, got `{lt}` and `{rt}`"),
            ));
        }
        Ok(Type::Bool)
    }

    fn infer_expr_binary_add(
        &self,
        span: crate::diagnostic::Span,
        lt: Type,
        rt: Type,
    ) -> Result<Type, TypeError> {
        if lt == Type::Int && rt == Type::Int {
            Ok(Type::Int)
        } else if lt == Type::String && rt == Type::String {
            Ok(Type::String)
        } else {
            Err(err_at(
                span,
                format!("`+` expects int+int or string+string, got `{lt}` and `{rt}`"),
            ))
        }
    }

    fn infer_expr_binary_arith(
        &self,
        span: crate::diagnostic::Span,
        lt: Type,
        rt: Type,
    ) -> Result<Type, TypeError> {
        if lt != Type::Int || rt != Type::Int {
            return Err(err_at(
                span,
                format!("arithmetic op expects int operands, got `{lt}` and `{rt}`"),
            ));
        }
        Ok(Type::Int)
    }

    fn infer_expr_member_ident(
        &self,
        env: &FnEnv<'a>,
        var: &str,
        field: &str,
    ) -> Result<Type, TypeError> {
        let struct_name = env.value_struct.get(var).ok_or_else(|| {
            err_at(
                env.current_span,
                "member access needs a variable with struct type",
            )
        })?;
        let struct_decl = self.structs.get(struct_name).ok_or_else(|| {
            err_at(
                env.current_span,
                format!("unknown struct type `{struct_name}`"),
            )
        })?;
        let struct_field = struct_decl
            .fields
            .iter()
            .find(|f| f.name == field)
            .ok_or_else(|| {
                err_at(
                    env.current_span,
                    format!("struct `{struct_name}` has no field `{field}`"),
                )
            })?;
        Ok(struct_field.ty.clone())
    }

    fn infer_expr_member_self(&self, env: &FnEnv<'a>, field: &str) -> Result<Type, TypeError> {
        if env.is_contract_fn {
            if !self.contract_fields.is_empty() {
                if let Some(cf) = self.contract_fields.iter().find(|f| f.name == field) {
                    if cf.ty.is_map() {
                        return Err(err_at(env.current_span, format!(
                            "use `self.{field}[key]` for contract map fields (whole-field load is not supported)"
                        )));
                    }
                    return Ok(cf.ty.clone());
                }
            }
        }
        let struct_name = env.value_struct.get("self").ok_or_else(|| {
            err_at(
                env.current_span,
                "`self.member` needs a contract field or struct `self` parameter",
            )
        })?;
        let struct_decl = self.structs.get(struct_name).ok_or_else(|| {
            err_at(
                env.current_span,
                format!("unknown struct type `{struct_name}`"),
            )
        })?;
        let struct_field = struct_decl
            .fields
            .iter()
            .find(|f| f.name == field)
            .ok_or_else(|| {
                err_at(
                    env.current_span,
                    format!("struct `{struct_name}` has no field `{field}`"),
                )
            })?;
        Ok(struct_field.ty.clone())
    }

    fn infer_expr_index_array_elem(
        &self,
        span: crate::diagnostic::Span,
        index_ty: Type,
        elem: Type,
    ) -> Result<Type, TypeError> {
        if index_ty != Type::Int {
            return Err(err_at(
                span,
                format!("array index must be int, got `{index_ty}`"),
            ));
        }
        Ok(elem)
    }

    fn infer_expr_index_map_value(
        &self,
        span: crate::diagnostic::Span,
        index_ty: Type,
        key: Type,
        value: Type,
    ) -> Result<Type, TypeError> {
        if !index_ty.can_assign_to(&key) {
            return Err(err_at(
                span,
                format!("map index type mismatch: expected `{key}`, got `{index_ty}`"),
            ));
        }
        Ok(value)
    }

    fn infer_expr_index(
        &self,
        env: &mut FnEnv<'a>,
        base: &Expr,
        index: &Expr,
    ) -> Result<Type, TypeError> {
        if env.is_contract_fn {
            if let Some((key_ty, val_ty)) =
                self.try_contract_self_map_types(base, env.current_span)?
            {
                let index_ty = self.infer_expr(env, index)?;
                if !index_ty.can_assign_to(&key_ty) {
                    return Err(err_at(
                        env.current_span,
                        format!("map index type mismatch: expected `{key_ty}`, got `{index_ty}`"),
                    ));
                }
                return Ok(val_ty);
            }
        }
        let base_ty = self.infer_expr(env, base)?;
        let index_ty = self.infer_expr(env, index)?;
        match base_ty {
            Type::Array(elem) => {
                self.infer_expr_index_array_elem(env.current_span, index_ty, *elem)
            }
            Type::Map { key, value } => {
                self.infer_expr_index_map_value(env.current_span, index_ty, *key, *value)
            }
            _ => Err(err_at(
                env.current_span,
                format!("indexing requires array or map, got `{base_ty}`"),
            )),
        }
    }

    fn infer_expr_struct_lit(
        &self,
        env: &mut FnEnv<'a>,
        name: &str,
        fields: &[(String, Expr)],
    ) -> Result<Type, TypeError> {
        let struct_decl = self.structs.get(name).ok_or_else(|| {
            err_at(
                env.current_span,
                format!("unknown struct `{name}` in struct literal"),
            )
        })?;
        for struct_field in &struct_decl.fields {
            let init = fields
                .iter()
                .find(|(n, _)| n == &struct_field.name)
                .map(|(_, expr)| expr)
                .or(struct_field.init.as_ref());
            if let Some(expr) = init {
                let ty = self.infer_expr(env, expr)?;
                if !lang_type_assignable_to(&ty, &struct_field.ty) {
                    return Err(err_at(
                        env.current_span,
                        format!(
                            "field `{}` type mismatch: expected `{}`, got `{ty}`",
                            struct_field.name, struct_field.ty
                        ),
                    ));
                }
            }
        }
        for (field_name, _) in fields {
            if !struct_decl.fields.iter().any(|f| f.name == *field_name) {
                return Err(err_at(
                    env.current_span,
                    format!("struct `{name}` has no field `{field_name}`"),
                ));
            }
        }
        Ok(Type::Named(name.to_string()))
    }

    fn infer_expr_map_lit(
        &self,
        env: &mut FnEnv<'a>,
        ty: &Type,
        pairs: &[(Expr, Expr)],
    ) -> Result<Type, TypeError> {
        check_map_key_rules_in_type(ty, env.current_span)?;
        let Type::Map { key, value } = ty else {
            return Err(err_at(
                env.current_span,
                "internal: MapLit without map type",
            ));
        };
        let key_ty = *key.clone();
        let value_ty = *value.clone();
        for (key_expr, value_expr) in pairs {
            let kt = self.infer_expr(env, key_expr)?;
            let vt = self.infer_expr(env, value_expr)?;
            if !kt.can_assign_to(&key_ty) {
                return Err(err_at(
                    env.current_span,
                    format!("map literal key type mismatch: expected `{key_ty}`, got `{kt}`"),
                ));
            }
            if !vt.can_assign_to(&value_ty) {
                return Err(err_at(
                    env.current_span,
                    format!("map literal value type mismatch: expected `{value_ty}`, got `{vt}`"),
                ));
            }
        }
        Ok(ty.clone())
    }

    fn infer_expr_array_lit(
        &self,
        env: &mut FnEnv<'a>,
        ty: &Type,
        elements: &[Expr],
    ) -> Result<Type, TypeError> {
        let Type::Array(elem) = ty else {
            return Err(err_at(
                env.current_span,
                "internal: ArrayLit without array type",
            ));
        };
        let elem_ty = *elem.clone();
        for expr in elements {
            let ty = self.infer_expr(env, expr)?;
            if !ty.can_assign_to(&elem_ty) {
                return Err(err_at(
                    env.current_span,
                    format!("array element type mismatch: expected `{elem_ty}`, got `{ty}`"),
                ));
            }
        }
        Ok(ty.clone())
    }

    fn infer_expr_assign(
        &self,
        env: &mut FnEnv<'a>,
        target: &Expr,
        op: AssignOp,
        value: &Expr,
    ) -> Result<Type, TypeError> {
        let value_ty = self.infer_expr(env, value)?;
        let target_ty = self.infer_lvalue_type(env, target)?;
        if matches!(op, AssignOp::Assign) {
            if !value_ty.can_assign_to(&target_ty) {
                return Err(err_at(
                    env.current_span,
                    format!("assignment type mismatch: target `{target_ty}`, value `{value_ty}`"),
                ));
            }
        } else if target_ty != Type::Int || value_ty != Type::Int {
            return Err(err_at(env.current_span, format!(
                "compound assignment expects int target and int value, got `{target_ty}` and `{value_ty}`"
            )));
        }
        Ok(value_ty)
    }

    pub(super) fn try_contract_self_map_types(
        &self,
        base: &Expr,
        span: crate::diagnostic::Span,
    ) -> Result<Option<(Type, Type)>, TypeError> {
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
            _ => Err(err_at(
                span,
                format!(
                    "only `map` contract fields support `[`index`]`; field `{fname}` is not a map"
                ),
            )),
        }
    }

    pub(super) fn infer_lvalue_type(
        &self,
        env: &mut FnEnv<'a>,
        target: &Expr,
    ) -> Result<Type, TypeError> {
        match target {
            Expr::Ident(name) => env.resolve(name).ok_or_else(|| {
                err_at(
                    env.current_span,
                    format!("unknown assignment target `{name}`"),
                )
            }),
            Expr::Member { base, field } => match base.as_ref() {
                Expr::Ident(var) => {
                    let struct_name = env.value_struct.get(var).cloned().ok_or_else(|| {
                        err_at(
                            env.current_span,
                            "member assignment needs a variable with struct type",
                        )
                    })?;
                    let struct_decl = self.structs.get(&struct_name).ok_or_else(|| {
                        err_at(
                            env.current_span,
                            format!("unknown struct type `{struct_name}`"),
                        )
                    })?;
                    let struct_field = struct_decl
                        .fields
                        .iter()
                        .find(|f| f.name == *field)
                        .ok_or_else(|| {
                            err_at(
                                env.current_span,
                                format!("struct `{struct_name}` has no field `{field}`"),
                            )
                        })?;
                    Ok(struct_field.ty.clone())
                }
                Expr::Self_ => {
                    if env.is_contract_fn && !self.contract_fields.is_empty() {
                        if let Some(contract_field) =
                            self.contract_fields.iter().find(|f| f.name == *field)
                        {
                            if contract_field.ty.is_map() {
                                return Err(err_at(
                                    env.current_span,
                                    "cannot assign to a contract map field without `[key]`",
                                ));
                            }
                            return Ok(contract_field.ty.clone());
                        }
                    }
                    let struct_name = env.value_struct.get("self").cloned().ok_or_else(|| {
                    err_at(
                        env.current_span,
                        "`self.member` assignment needs a contract field or struct `self` parameter",
                    )
                })?;
                    let struct_decl = self.structs.get(&struct_name).ok_or_else(|| {
                        err_at(
                            env.current_span,
                            format!("unknown struct type `{struct_name}`"),
                        )
                    })?;
                    let struct_field = struct_decl
                        .fields
                        .iter()
                        .find(|f| f.name == *field)
                        .ok_or_else(|| {
                            err_at(
                                env.current_span,
                                format!("struct `{struct_name}` has no field `{field}`"),
                            )
                        })?;
                    Ok(struct_field.ty.clone())
                }
                _ => Err(err_at(
                    env.current_span,
                    "only `variable.field` or `self.field` member assignment is allowed",
                )),
            },
            Expr::Index { base, index } => {
                if env.is_contract_fn {
                    if let Some((key_ty, val_ty)) =
                        self.try_contract_self_map_types(base.as_ref(), env.current_span)?
                    {
                        let index_ty = self.infer_expr(env, index)?;
                        if !index_ty.can_assign_to(&key_ty) {
                            return Err(err_at(
                                env.current_span,
                                format!("map index type mismatch: expected `{key_ty}`, got `{index_ty}`"),
                            ));
                        }
                        return Ok(val_ty);
                    }
                }
                let base_ty = self.infer_expr(env, base)?;
                let index_ty = self.infer_expr(env, index)?;
                match base_ty {
                    Type::Array(elem) => {
                        if index_ty != Type::Int {
                            return Err(err_at(env.current_span, "array index must be int"));
                        }
                        Ok(*elem)
                    }
                    Type::Map { key, value } => {
                        if !index_ty.can_assign_to(&key) {
                            return Err(err_at(env.current_span, "map index type mismatch"));
                        }
                        Ok(*value)
                    }
                    _ => Err(err_at(env.current_span, "invalid index assignment target")),
                }
            }
            _ => Err(err_at(env.current_span, "invalid assignment target")),
        }
    }
}
