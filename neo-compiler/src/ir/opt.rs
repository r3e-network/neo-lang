//! SSA-friendly IR optimizations (first phase).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ir::*;
use crate::syntax::ast::{BinaryOp, Literal, Type, UnaryOp};

impl FunctionIr {
    /// Run simple, safe optimizations: const folding/prop, copy prop, DCE.
    pub fn optimize(&mut self) {
        self.const_fold();
        self.copy_prop();
        self.comm_subexpr_elimination();
        self.dead_code_elimination();
    }

    fn comm_subexpr_elimination(&mut self) {
        // Local value numbering per basic block for pure ops only.
        // We do not CSE across blocks yet (needs dominance or global VN).
        for bb in self.blocks.values_mut() {
            let mut subst: HashMap<ValueId, ValueRef> = HashMap::new();
            let mut table: HashMap<PureKey, ValueRef> = HashMap::new();

            fn norm(v: ValueRef, subst: &HashMap<ValueId, ValueRef>) -> ValueRef {
                match v {
                    ValueRef::Value(id) => subst.get(&id).copied().unwrap_or(v),
                    ValueRef::Param(_) => v,
                }
            }

            for (out, instr) in bb.instrs.iter_mut() {
                // Only pure computations are eligible.
                let key = match instr {
                    Instr::Const(literal) => Some(PureKey::Const(literal.clone())),
                    Instr::StructFieldGet { base, index } => {
                        *base = norm(*base, &subst);
                        Some(PureKey::StructFieldGet {
                            base: *base,
                            index: *index,
                        })
                    }
                    Instr::Copy(value) => {
                        *value = norm(*value, &subst);
                        Some(PureKey::Copy(*value))
                    }
                    Instr::Unary { op, value } => {
                        *value = norm(*value, &subst);
                        Some(PureKey::Unary {
                            op: *op,
                            value: *value,
                        })
                    }
                    Instr::Binary { op, left, right } => {
                        *left = norm(*left, &subst);
                        *right = norm(*right, &subst);
                        Some(PureKey::Binary {
                            op: *op,
                            left: *left,
                            right: *right,
                        })
                    }
                    Instr::IndexGet { base, index } => {
                        *base = norm(*base, &subst);
                        *index = norm(*index, &subst);
                        Some(PureKey::IndexGet {
                            base: *base,
                            index: *index,
                        })
                    }
                    Instr::Cast { value, ty } => {
                        *value = norm(*value, &subst);
                        Some(PureKey::Cast {
                            value: *value,
                            ty: ty.clone(),
                        })
                    }
                    Instr::Min { left, right } => {
                        *left = norm(*left, &subst);
                        *right = norm(*right, &subst);
                        Some(PureKey::Min {
                            left: *left,
                            right: *right,
                        })
                    }
                    Instr::Max { left, right } => {
                        *left = norm(*left, &subst);
                        *right = norm(*right, &subst);
                        Some(PureKey::Max {
                            left: *left,
                            right: *right,
                        })
                    }
                    Instr::IndexSet { .. }
                    | Instr::StructFieldSet { .. }
                    | Instr::ContractStorageGet { .. }
                    | Instr::ContractStoragePut { .. }
                    | Instr::ContractMapStorageGet { .. }
                    | Instr::ContractMapStoragePut { .. }
                    | Instr::ContractMapStorageCompound { .. }
                    | Instr::Assert { .. }
                    | Instr::Abort { .. }
                    | Instr::Emit { .. }
                    | Instr::PackageCall { .. }
                    | Instr::StructPack { .. }
                    | Instr::StructInstanceCall { .. }
                    | Instr::RuntimeLog { .. }
                    | Instr::ArrayPack { .. }
                    | Instr::MapPack { .. }
                    | Instr::EvalAst(_) => None,
                };

                if let Some(k) = key {
                    if let Some(existing) = table.get(&k).copied() {
                        // Replace this def with a copy of the existing value.
                        subst.insert(*out, existing);
                        *instr = Instr::Copy(existing);
                    } else {
                        table.insert(k, ValueRef::Value(*out));
                    }
                }
            }

            if !subst.is_empty() {
                // Rewrite remaining uses in this block (instrs + terminator).
                for (_out, instr) in bb.instrs.iter_mut() {
                    rewrite_value_refs_in_instr(instr, &subst);
                }
                rewrite_value_refs_in_term(&mut bb.term, &subst);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PureKey {
    Const(Literal),
    StructFieldGet {
        base: ValueRef,
        index: usize,
    },
    IndexGet {
        base: ValueRef,
        index: ValueRef,
    },
    Copy(ValueRef),
    Unary {
        op: UnaryOp,
        value: ValueRef,
    },
    Binary {
        op: BinaryOp,
        left: ValueRef,
        right: ValueRef,
    },
    Cast {
        value: ValueRef,
        ty: Type,
    },
    Min {
        left: ValueRef,
        right: ValueRef,
    },
    Max {
        left: ValueRef,
        right: ValueRef,
    },
}

impl FunctionIr {
    fn const_fold(&mut self) {
        // Local instruction folding; not global propagation yet.
        for bb in self.blocks.values_mut() {
            // Snapshot constants in this block (out -> lit) so we can fold without aliasing borrows.
            let mut consts: HashMap<ValueId, Literal> = HashMap::new();
            for (out, instr) in bb.instrs.iter() {
                if let Instr::Const(literal) = instr {
                    consts.insert(*out, literal.clone());
                }
            }
            for (_out, instr) in bb.instrs.iter_mut() {
                match instr {
                    Instr::Unary { op, value } => {
                        if let Some(literal) = as_const_ref(&consts, *value) {
                            if let Some(n) = fold_unary(*op, &literal) {
                                *instr = Instr::Const(n);
                            }
                        }
                    }
                    Instr::Binary { op, left, right } => {
                        if let (Some(left_literal), Some(right_literal)) =
                            (as_const_ref(&consts, *left), as_const_ref(&consts, *right))
                        {
                            if let Some(n) = fold_binary(*op, &left_literal, &right_literal) {
                                *instr = Instr::Const(n);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn copy_prop(&mut self) {
        // Very small: rewrite `Copy(x)` -> just use x, and rewrite uses of values defined by Copy/Const.
        //
        // Substitutions must be **per basic block**: a `Copy(Param(i))` in the entry block defines a
        // `ValueId` that means "argument i" only while lowering uses in that same block. If we applied
        // the same map globally, uses of that `ValueId` in successor blocks would incorrectly become
        // bare `Param(i)` (those blocks' Param indices refer to *their* phi params, not callee args).
        for bb in self.blocks.values_mut() {
            let mut subst: HashMap<ValueId, ValueRef> = HashMap::new();
            for (out, instr) in &bb.instrs {
                if let Instr::Copy(v) = instr {
                    subst.insert(*out, *v);
                }
            }
            if subst.is_empty() {
                continue;
            }
            for (_out, instr) in bb.instrs.iter_mut() {
                rewrite_value_refs_in_instr(instr, &subst);
            }
            rewrite_value_refs_in_term(&mut bb.term, &subst);
        }
    }

    fn dead_code_elimination(&mut self) {
        // Mark used values by walking from terminators; keep EvalAst conservatively.
        let mut used: HashSet<ValueId> = HashSet::new();
        let mut work: VecDeque<ValueId> = VecDeque::new();

        // Build def map once: ValueId -> Instr
        let mut defs: HashMap<ValueId, Instr> = HashMap::new();
        for bb in self.blocks.values() {
            for (out, instr) in &bb.instrs {
                defs.insert(*out, instr.clone());
            }
        }

        for bb in self.blocks.values() {
            collect_uses_in_term(&bb.term, &mut work);
        }
        // Side-effecting instructions must be treated as roots: even if their result is unused,
        // their operands are still required for correctness.
        for bb in self.blocks.values() {
            for (_out, instr) in &bb.instrs {
                if matches!(
                    instr,
                    Instr::EvalAst(_)
                        | Instr::IndexSet { .. }
                        | Instr::StructFieldSet { .. }
                        | Instr::ContractStorageGet { .. }
                        | Instr::ContractStoragePut { .. }
                        | Instr::ContractMapStorageGet { .. }
                        | Instr::ContractMapStoragePut { .. }
                        | Instr::ContractMapStorageCompound { .. }
                        | Instr::Assert { .. }
                        | Instr::Abort { .. }
                        | Instr::Emit { .. }
                        | Instr::PackageCall { .. }
                        | Instr::StructPack { .. }
                        | Instr::StructInstanceCall { .. }
                        | Instr::RuntimeLog { .. }
                        | Instr::ArrayPack { .. }
                        | Instr::MapPack { .. }
                ) {
                    collect_uses_in_instr(instr, &mut work);
                }
            }
        }
        while let Some(v) = work.pop_front() {
            if !used.insert(v) {
                continue;
            }
            if let Some(instr) = defs.get(&v) {
                collect_uses_in_instr(instr, &mut work);
            }
        }

        for bb in self.blocks.values_mut() {
            bb.instrs.retain(|(out, instr)| {
                if matches!(
                    instr,
                    Instr::EvalAst(_)
                        | Instr::IndexSet { .. }
                        | Instr::StructFieldSet { .. }
                        | Instr::ContractStorageGet { .. }
                        | Instr::ContractStoragePut { .. }
                        | Instr::ContractMapStorageGet { .. }
                        | Instr::ContractMapStoragePut { .. }
                        | Instr::ContractMapStorageCompound { .. }
                        | Instr::Assert { .. }
                        | Instr::Abort { .. }
                        | Instr::Emit { .. }
                        | Instr::PackageCall { .. }
                        | Instr::StructPack { .. }
                        | Instr::StructInstanceCall { .. }
                        | Instr::RuntimeLog { .. }
                        | Instr::ArrayPack { .. }
                        | Instr::MapPack { .. }
                ) {
                    return true; // side effects / conservative
                }
                used.contains(out)
            });
        }
    }
}

fn collect_uses_in_term(terminator: &Terminator, out: &mut VecDeque<ValueId>) {
    match terminator {
        Terminator::Return(v) => {
            if let Some(ValueRef::Value(x)) = v {
                out.push_back(*x);
            }
        }
        Terminator::Jump { args, .. } => {
            for arg in args {
                if let ValueRef::Value(x) = arg {
                    out.push_back(*x);
                }
            }
        }
        Terminator::Branch {
            cond,
            then_args,
            else_args,
            ..
        } => {
            if let ValueRef::Value(x) = cond {
                out.push_back(*x);
            }
            for arg in then_args.iter().chain(else_args.iter()) {
                if let ValueRef::Value(x) = arg {
                    out.push_back(*x);
                }
            }
        }
    }
}

fn collect_uses_in_instr(instr: &Instr, out: &mut VecDeque<ValueId>) {
    match instr {
        Instr::Unary { value, .. } | Instr::Copy(value) => {
            if let ValueRef::Value(x) = value {
                out.push_back(*x);
            }
        }
        Instr::StructFieldGet { base, .. } => {
            if let ValueRef::Value(x) = base {
                out.push_back(*x);
            }
        }
        Instr::IndexGet { base, index } => {
            if let ValueRef::Value(x) = base {
                out.push_back(*x);
            }
            if let ValueRef::Value(x) = index {
                out.push_back(*x);
            }
        }
        Instr::IndexSet { base, index, value } => {
            if let ValueRef::Value(x) = base {
                out.push_back(*x);
            }
            if let ValueRef::Value(x) = index {
                out.push_back(*x);
            }
            if let ValueRef::Value(x) = value {
                out.push_back(*x);
            }
        }
        Instr::StructFieldSet { base, value, .. } => {
            if let ValueRef::Value(x) = base {
                out.push_back(*x);
            }
            if let ValueRef::Value(x) = value {
                out.push_back(*x);
            }
        }
        Instr::Binary { left, right, .. } => {
            if let ValueRef::Value(x) = left {
                out.push_back(*x);
            }
            if let ValueRef::Value(x) = right {
                out.push_back(*x);
            }
        }
        Instr::ContractStoragePut { value, .. } => {
            if let ValueRef::Value(x) = value {
                out.push_back(*x);
            }
        }
        Instr::ContractMapStorageGet { key, .. } => {
            if let ValueRef::Value(x) = key {
                out.push_back(*x);
            }
        }
        Instr::ContractMapStoragePut { key, value, .. } => {
            if let ValueRef::Value(x) = key {
                out.push_back(*x);
            }
            if let ValueRef::Value(x) = value {
                out.push_back(*x);
            }
        }
        Instr::ContractMapStorageCompound { key, value, .. } => {
            if let ValueRef::Value(x) = key {
                out.push_back(*x);
            }
            if let ValueRef::Value(x) = value {
                out.push_back(*x);
            }
        }
        Instr::Assert { cond, message } => {
            if let ValueRef::Value(x) = cond {
                out.push_back(*x);
            }
            if let ValueRef::Value(x) = message {
                out.push_back(*x);
            }
        }
        Instr::Abort { message } => {
            if let ValueRef::Value(x) = message {
                out.push_back(*x);
            }
        }
        Instr::Min { left, right } | Instr::Max { left, right } => {
            if let ValueRef::Value(x) = left {
                out.push_back(*x);
            }
            if let ValueRef::Value(x) = right {
                out.push_back(*x);
            }
        }
        Instr::Cast { value, .. } => {
            if let ValueRef::Value(x) = value {
                out.push_back(*x);
            }
        }
        Instr::Emit { args, .. } => {
            for arg in args {
                if let ValueRef::Value(x) = arg {
                    out.push_back(*x);
                }
            }
        }
        Instr::PackageCall { args, .. } => {
            for arg in args {
                if let ValueRef::Value(x) = arg {
                    out.push_back(*x);
                }
            }
        }
        Instr::StructPack { field_values, .. } => {
            for value in field_values {
                if let ValueRef::Value(x) = value {
                    out.push_back(*x);
                }
            }
        }
        Instr::StructInstanceCall { recv, args, .. } => {
            if let ValueRef::Value(x) = recv {
                out.push_back(*x);
            }
            for arg in args {
                if let ValueRef::Value(x) = arg {
                    out.push_back(*x);
                }
            }
        }
        Instr::RuntimeLog { message } => {
            if let ValueRef::Value(x) = message {
                out.push_back(*x);
            }
        }
        Instr::ArrayPack { elements } => {
            for element in elements {
                if let ValueRef::Value(x) = element {
                    out.push_back(*x);
                }
            }
        }
        Instr::MapPack { pairs } => {
            for (key, value) in pairs {
                if let ValueRef::Value(x) = key {
                    out.push_back(*x);
                }
                if let ValueRef::Value(x) = value {
                    out.push_back(*x);
                }
            }
        }
        Instr::ContractStorageGet { .. } => {}
        Instr::EvalAst(_) | Instr::Const(_) => {}
    }
}

fn rewrite_value_refs_in_instr(instr: &mut Instr, subst: &HashMap<ValueId, ValueRef>) {
    match instr {
        Instr::Unary { value, .. } | Instr::Copy(value) => *value = rewrite(*value, subst),
        Instr::StructFieldGet { base, .. } => *base = rewrite(*base, subst),
        Instr::IndexGet { base, index } => {
            *base = rewrite(*base, subst);
            *index = rewrite(*index, subst);
        }
        Instr::IndexSet { base, index, value } => {
            *base = rewrite(*base, subst);
            *index = rewrite(*index, subst);
            *value = rewrite(*value, subst);
        }
        Instr::StructFieldSet { base, value, .. } => {
            *base = rewrite(*base, subst);
            *value = rewrite(*value, subst);
        }
        Instr::Binary { left, right, .. } => {
            *left = rewrite(*left, subst);
            *right = rewrite(*right, subst);
        }
        Instr::ContractStoragePut { value, .. } => {
            *value = rewrite(*value, subst);
        }
        Instr::ContractMapStorageGet { key, .. } => {
            *key = rewrite(*key, subst);
        }
        Instr::ContractMapStoragePut { key, value, .. } => {
            *key = rewrite(*key, subst);
            *value = rewrite(*value, subst);
        }
        Instr::ContractMapStorageCompound { key, value, .. } => {
            *key = rewrite(*key, subst);
            *value = rewrite(*value, subst);
        }
        Instr::Assert { cond, message } => {
            *cond = rewrite(*cond, subst);
            *message = rewrite(*message, subst);
        }
        Instr::Abort { message } => {
            *message = rewrite(*message, subst);
        }
        Instr::Min { left, right } | Instr::Max { left, right } => {
            *left = rewrite(*left, subst);
            *right = rewrite(*right, subst);
        }
        Instr::Cast { value, .. } => {
            *value = rewrite(*value, subst);
        }
        Instr::Emit { args, .. } => {
            for arg in args {
                *arg = rewrite(*arg, subst);
            }
        }
        Instr::PackageCall { args, .. } => {
            for arg in args {
                *arg = rewrite(*arg, subst);
            }
        }
        Instr::StructPack { field_values, .. } => {
            for value in field_values {
                *value = rewrite(*value, subst);
            }
        }
        Instr::StructInstanceCall { recv, args, .. } => {
            *recv = rewrite(*recv, subst);
            for arg in args {
                *arg = rewrite(*arg, subst);
            }
        }
        Instr::RuntimeLog { message } => {
            *message = rewrite(*message, subst);
        }
        Instr::ArrayPack { elements } => {
            for element in elements {
                *element = rewrite(*element, subst);
            }
        }
        Instr::MapPack { pairs } => {
            for (key, value) in pairs {
                *key = rewrite(*key, subst);
                *value = rewrite(*value, subst);
            }
        }
        Instr::ContractStorageGet { .. } | Instr::EvalAst(_) | Instr::Const(_) => {}
    }
}

fn rewrite_value_refs_in_term(terminator: &mut Terminator, subst: &HashMap<ValueId, ValueRef>) {
    match terminator {
        Terminator::Return(value) => {
            if let Some(x) = value {
                *x = rewrite(*x, subst);
            }
        }
        Terminator::Jump { args, .. } => {
            for arg in args {
                *arg = rewrite(*arg, subst);
            }
        }
        Terminator::Branch {
            cond,
            then_args,
            else_args,
            ..
        } => {
            *cond = rewrite(*cond, subst);
            for arg in then_args.iter_mut().chain(else_args.iter_mut()) {
                *arg = rewrite(*arg, subst);
            }
        }
    }
}

fn rewrite(value: ValueRef, subst: &HashMap<ValueId, ValueRef>) -> ValueRef {
    match value {
        ValueRef::Value(id) => subst.get(&id).copied().unwrap_or(value),
        ValueRef::Param(_) => value,
    }
}

fn fold_unary(op: UnaryOp, value: &Literal) -> Option<Literal> {
    match (op, value) {
        (UnaryOp::Not, Literal::Bool(b)) => Some(Literal::Bool(!b)),
        (UnaryOp::Negative, Literal::Int(s)) => Some(Literal::Int(format!("-{s}"))),
        (UnaryOp::Positive, Literal::Int(s)) => Some(Literal::Int(s.clone())),
        _ => None,
    }
}

fn fold_binary(op: BinaryOp, left: &Literal, right: &Literal) -> Option<Literal> {
    match (op, left, right) {
        (BinaryOp::Add, Literal::Int(x), Literal::Int(y)) => {
            let xi: i64 = x.parse().ok()?;
            let yi: i64 = y.parse().ok()?;
            Some(Literal::Int((xi + yi).to_string()))
        }
        (BinaryOp::Sub, Literal::Int(x), Literal::Int(y)) => {
            let xi: i64 = x.parse().ok()?;
            let yi: i64 = y.parse().ok()?;
            Some(Literal::Int((xi - yi).to_string()))
        }
        (BinaryOp::Mul, Literal::Int(x), Literal::Int(y)) => {
            let xi: i64 = x.parse().ok()?;
            let yi: i64 = y.parse().ok()?;
            Some(Literal::Int((xi * yi).to_string()))
        }
        (BinaryOp::Div, Literal::Int(x), Literal::Int(y)) => {
            let xi: i64 = x.parse().ok()?;
            let yi: i64 = y.parse().ok()?;
            Some(Literal::Int((xi / yi).to_string()))
        }
        (BinaryOp::Eq, Literal::Int(x), Literal::Int(y)) => Some(Literal::Bool(x == y)),
        (BinaryOp::Eq, Literal::Bool(x), Literal::Bool(y)) => Some(Literal::Bool(x == y)),
        (BinaryOp::Ne, Literal::Int(x), Literal::Int(y)) => Some(Literal::Bool(x != y)),
        (BinaryOp::Ne, Literal::Bool(x), Literal::Bool(y)) => Some(Literal::Bool(x != y)),
        _ => None,
    }
}

fn as_const_ref(consts: &HashMap<ValueId, Literal>, value: ValueRef) -> Option<Literal> {
    let ValueRef::Value(id) = value else {
        return None;
    };
    consts.get(&id).cloned()
}
