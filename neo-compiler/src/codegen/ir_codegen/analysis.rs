use std::collections::{HashMap, HashSet};

use crate::ir::*;

impl Instr {
    pub(super) fn collect_value_uses(&self, out: &mut HashMap<ValueId, usize>) {
        fn bump(value: ValueRef, out: &mut HashMap<ValueId, usize>) {
            if let ValueRef::Value(id) = value {
                *out.entry(id).or_insert(0) += 1;
            }
        }
        match self {
            Instr::Const(_) => {}
            Instr::StructFieldGet { base, .. } => bump(*base, out),
            Instr::IndexGet { base, index } => {
                bump(*base, out);
                bump(*index, out);
            }
            Instr::Size { value } => bump(*value, out),
            Instr::Keys { map } => bump(*map, out),
            Instr::Values { map } => bump(*map, out),
            Instr::HasKey { map, key } => {
                bump(*map, out);
                bump(*key, out);
            }
            Instr::SubStr {
                value,
                start,
                length,
            } => {
                bump(*value, out);
                bump(*start, out);
                bump(*length, out);
            }
            Instr::Sqrt { value } => bump(*value, out),
            Instr::ModMul {
                value,
                other,
                modulus,
            } => {
                bump(*value, out);
                bump(*other, out);
                bump(*modulus, out);
            }
            Instr::ModPow {
                value,
                exponent,
                modulus,
            } => {
                bump(*value, out);
                bump(*exponent, out);
                bump(*modulus, out);
            }
            Instr::Within {
                value,
                min_inclusive,
                max_exclusive,
            } => {
                bump(*value, out);
                bump(*min_inclusive, out);
                bump(*max_exclusive, out);
            }
            Instr::IndexSet { base, index, value } => {
                bump(*base, out);
                bump(*index, out);
                bump(*value, out);
            }
            Instr::ArrayAppend { array, value } => {
                bump(*array, out);
                bump(*value, out);
            }
            Instr::ArrayPop { array } => bump(*array, out),
            Instr::ClearItems { collection } => bump(*collection, out),
            Instr::Remove { map, key } => {
                bump(*map, out);
                bump(*key, out);
            }
            Instr::StructFieldSet { base, value, .. } => {
                bump(*base, out);
                bump(*value, out);
            }
            Instr::Unary { value, .. } | Instr::Copy(value) => bump(*value, out),
            Instr::IsNull { value, .. } => bump(*value, out),
            Instr::Binary { left, right, .. } => {
                bump(*left, out);
                bump(*right, out);
            }
            Instr::Cast { value, .. } => bump(*value, out),
            Instr::BuiltinCall { args, .. } => {
                for arg in args {
                    bump(*arg, out);
                }
            }
            Instr::ContractStorageGet { .. } => {}
            Instr::ContractStoragePut { value, .. } => bump(*value, out),
            Instr::ContractMapStorageGet { key, .. } => bump(*key, out),
            Instr::ContractMapStorageHas { key, .. } => bump(*key, out),
            Instr::ContractMapStorageRemove { key, .. } => bump(*key, out),
            Instr::ContractMapStoragePut { key, value, .. } => {
                bump(*key, out);
                bump(*value, out);
            }
            Instr::ContractMapStorageCompound { key, value, .. } => {
                bump(*key, out);
                bump(*value, out);
            }
            Instr::Emit { args, .. } => {
                for arg in args {
                    bump(*arg, out);
                }
            }
            Instr::PackageCall { args, .. } => {
                for arg in args {
                    bump(*arg, out);
                }
            }
            Instr::ContractMethodCall { args, .. } => {
                for arg in args {
                    bump(*arg, out);
                }
            }
            Instr::StructPack { field_values, .. } => {
                for value in field_values {
                    bump(*value, out);
                }
            }
            Instr::StructCall { recv, args, .. } => {
                bump(*recv, out);
                for arg in args {
                    bump(*arg, out);
                }
            }
            Instr::RuntimeCall { args, .. } => {
                for arg in args {
                    bump(*arg, out);
                }
            }
            Instr::NativeCall { args, .. } => {
                for arg in args {
                    bump(*arg, out);
                }
            }
            Instr::ArrayPack { elements } => {
                for value in elements {
                    bump(*value, out);
                }
            }
            Instr::MapPack { pairs } => {
                for (key, value) in pairs {
                    bump(*key, out);
                    bump(*value, out);
                }
            }
            Instr::EvalAst(_) => {}
        }
    }

    pub(super) fn collect_cross_block_uses(
        &self,
        use_bb: BlockId,
        def_block: &HashMap<ValueId, BlockId>,
        out: &mut HashSet<ValueId>,
    ) {
        fn bump(
            use_bb: BlockId,
            value: ValueRef,
            def_block: &HashMap<ValueId, BlockId>,
            out: &mut HashSet<ValueId>,
        ) {
            let ValueRef::Value(id) = value else { return };
            if def_block.get(&id).copied().is_some_and(|db| db != use_bb) {
                out.insert(id);
            }
        }
        match self {
            Instr::Const(_) => {}
            Instr::StructFieldGet { base, .. } => bump(use_bb, *base, def_block, out),
            Instr::IndexGet { base, index } => {
                bump(use_bb, *base, def_block, out);
                bump(use_bb, *index, def_block, out);
            }
            Instr::Size { value } => bump(use_bb, *value, def_block, out),
            Instr::Keys { map } => bump(use_bb, *map, def_block, out),
            Instr::Values { map } => bump(use_bb, *map, def_block, out),
            Instr::HasKey { map, key } => {
                bump(use_bb, *map, def_block, out);
                bump(use_bb, *key, def_block, out);
            }
            Instr::SubStr {
                value,
                start,
                length,
            } => {
                bump(use_bb, *value, def_block, out);
                bump(use_bb, *start, def_block, out);
                bump(use_bb, *length, def_block, out);
            }
            Instr::Sqrt { value } => bump(use_bb, *value, def_block, out),
            Instr::ModMul {
                value,
                other,
                modulus,
            } => {
                bump(use_bb, *value, def_block, out);
                bump(use_bb, *other, def_block, out);
                bump(use_bb, *modulus, def_block, out);
            }
            Instr::ModPow {
                value,
                exponent,
                modulus,
            } => {
                bump(use_bb, *value, def_block, out);
                bump(use_bb, *exponent, def_block, out);
                bump(use_bb, *modulus, def_block, out);
            }
            Instr::Within {
                value,
                min_inclusive,
                max_exclusive,
            } => {
                bump(use_bb, *value, def_block, out);
                bump(use_bb, *min_inclusive, def_block, out);
                bump(use_bb, *max_exclusive, def_block, out);
            }
            Instr::IndexSet { base, index, value } => {
                bump(use_bb, *base, def_block, out);
                bump(use_bb, *index, def_block, out);
                bump(use_bb, *value, def_block, out);
            }
            Instr::ArrayAppend { array, value } => {
                bump(use_bb, *array, def_block, out);
                bump(use_bb, *value, def_block, out);
            }
            Instr::ArrayPop { array } => bump(use_bb, *array, def_block, out),
            Instr::ClearItems { collection } => bump(use_bb, *collection, def_block, out),
            Instr::Remove { map, key } => {
                bump(use_bb, *map, def_block, out);
                bump(use_bb, *key, def_block, out);
            }
            Instr::StructFieldSet { base, value, .. } => {
                bump(use_bb, *base, def_block, out);
                bump(use_bb, *value, def_block, out);
            }
            Instr::Unary { value, .. } | Instr::Copy(value) => bump(use_bb, *value, def_block, out),
            Instr::IsNull { value, .. } => bump(use_bb, *value, def_block, out),
            Instr::Binary { left, right, .. } => {
                bump(use_bb, *left, def_block, out);
                bump(use_bb, *right, def_block, out);
            }
            Instr::Cast { value, .. } => bump(use_bb, *value, def_block, out),
            Instr::BuiltinCall { args, .. } => {
                for arg in args {
                    bump(use_bb, *arg, def_block, out);
                }
            }
            Instr::ContractStorageGet { .. } => {}
            Instr::ContractStoragePut { value, .. } => bump(use_bb, *value, def_block, out),
            Instr::ContractMapStorageGet { key, .. } => bump(use_bb, *key, def_block, out),
            Instr::ContractMapStorageHas { key, .. } => bump(use_bb, *key, def_block, out),
            Instr::ContractMapStorageRemove { key, .. } => bump(use_bb, *key, def_block, out),
            Instr::ContractMapStoragePut { key, value, .. } => {
                bump(use_bb, *key, def_block, out);
                bump(use_bb, *value, def_block, out);
            }
            Instr::ContractMapStorageCompound { key, value, .. } => {
                bump(use_bb, *key, def_block, out);
                bump(use_bb, *value, def_block, out);
            }
            Instr::Emit { args, .. } => {
                for arg in args {
                    bump(use_bb, *arg, def_block, out);
                }
            }
            Instr::PackageCall { args, .. } => {
                for arg in args {
                    bump(use_bb, *arg, def_block, out);
                }
            }
            Instr::ContractMethodCall { args, .. } => {
                for arg in args {
                    bump(use_bb, *arg, def_block, out);
                }
            }
            Instr::StructPack { field_values, .. } => {
                for value in field_values {
                    bump(use_bb, *value, def_block, out);
                }
            }
            Instr::StructCall { recv, args, .. } => {
                bump(use_bb, *recv, def_block, out);
                for arg in args {
                    bump(use_bb, *arg, def_block, out);
                }
            }
            Instr::RuntimeCall { args, .. } => {
                for arg in args {
                    bump(use_bb, *arg, def_block, out);
                }
            }
            Instr::NativeCall { args, .. } => {
                for arg in args {
                    bump(use_bb, *arg, def_block, out);
                }
            }
            Instr::ArrayPack { elements } => {
                for value in elements {
                    bump(use_bb, *value, def_block, out);
                }
            }
            Instr::MapPack { pairs } => {
                for (key, value) in pairs {
                    bump(use_bb, *key, def_block, out);
                    bump(use_bb, *value, def_block, out);
                }
            }
            Instr::EvalAst(_) => {}
        }
    }
}

impl Terminator {
    pub(super) fn collect_value_uses(&self, out: &mut HashMap<ValueId, usize>) {
        fn bump(value: ValueRef, out: &mut HashMap<ValueId, usize>) {
            if let ValueRef::Value(id) = value {
                *out.entry(id).or_insert(0) += 1;
            }
        }
        match self {
            Terminator::Unset => {}
            Terminator::Return(value) => {
                if let Some(value) = value {
                    bump(*value, out);
                }
            }
            Terminator::Jump { args, .. } => {
                for arg in args {
                    bump(*arg, out);
                }
            }
            Terminator::Branch {
                cond,
                then_args,
                else_args,
                ..
            } => {
                bump(*cond, out);
                for arg in then_args.iter().chain(else_args.iter()) {
                    bump(*arg, out);
                }
            }
        }
    }

    pub(super) fn collect_cross_block_uses(
        &self,
        use_bb: BlockId,
        def_block: &HashMap<ValueId, BlockId>,
        out: &mut HashSet<ValueId>,
    ) {
        fn bump(
            use_bb: BlockId,
            value: ValueRef,
            def_block: &HashMap<ValueId, BlockId>,
            out: &mut HashSet<ValueId>,
        ) {
            let ValueRef::Value(id) = value else { return };
            if def_block.get(&id).copied().is_some_and(|db| db != use_bb) {
                out.insert(id);
            }
        }
        match self {
            Terminator::Unset => {}
            Terminator::Return(value) => {
                if let Some(value) = value {
                    bump(use_bb, *value, def_block, out);
                }
            }
            Terminator::Jump { args, .. } => {
                for arg in args {
                    bump(use_bb, *arg, def_block, out);
                }
            }
            Terminator::Branch {
                cond,
                then_args,
                else_args,
                ..
            } => {
                bump(use_bb, *cond, def_block, out);
                for arg in then_args.iter().chain(else_args.iter()) {
                    bump(use_bb, *arg, def_block, out);
                }
            }
        }
    }
}
