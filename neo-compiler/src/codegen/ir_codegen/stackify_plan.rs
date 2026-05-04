use std::collections::{HashMap, HashSet};

use crate::codegen::expr::parse_int_literal;
use crate::codegen::CodegenError;
use crate::ir::*;
use crate::syntax::ast::Literal;

fn is_cheap_const_literal(lit: &Literal) -> bool {
    match lit {
        Literal::Null | Literal::Bool(_) => true,
        Literal::Int(s) => {
            let Some(n) = parse_int_literal(s) else {
                return false;
            };
            (i32::MIN as i128..=i32::MAX as i128).contains(&n)
        }
        Literal::String(v) | Literal::Buffer(v) => v.len() <= 4,
    }
}

/// Result of the stackify pre-pass: spill set, local slot maps, and final `locals` count for `INITSLOT`.
pub(super) struct StackifyPlan {
    pub uses: HashMap<ValueId, usize>,
    pub def_block: HashMap<ValueId, BlockId>,
    pub def_instr_vec: Vec<Option<Instr>>,
    pub cross_block_use: HashSet<ValueId>,
    pub spill: HashSet<ValueId>,
    pub value_slot: HashMap<ValueId, u8>,
    pub param_slot: HashMap<(BlockId, usize), u8>,
    pub compound_local_pairs: Vec<(u8, u8)>,
    pub next_local: u8,
}

impl FunctionIr {
    pub(super) fn build_stackify_plan(&self) -> Result<StackifyPlan, CodegenError> {
        let mut uses: HashMap<ValueId, usize> = HashMap::new();
        let mut def_block: HashMap<ValueId, BlockId> = HashMap::new();
        let mut def_instr: HashMap<ValueId, Instr> = HashMap::new();
        for (block_id, block) in &self.blocks {
            for (value_id, instr) in &block.instrs {
                def_block.insert(*value_id, *block_id);
                def_instr.insert(*value_id, instr.clone());
            }
        }

        let mut def_instr_vec: Vec<Option<Instr>> = vec![None; self.value_count];
        for (value_id, instr) in def_instr.iter() {
            if value_id.0 < def_instr_vec.len() {
                def_instr_vec[value_id.0] = Some(instr.clone());
            }
        }

        let mut cross_block_use: HashSet<ValueId> = HashSet::new();
        for (block_id, block) in &self.blocks {
            for (_, instr) in &block.instrs {
                instr.collect_value_uses(&mut uses);
                instr.collect_cross_block_uses(*block_id, &def_block, &mut cross_block_use);
            }
            block.term.collect_value_uses(&mut uses);
            block
                .term
                .collect_cross_block_uses(*block_id, &def_block, &mut cross_block_use);
        }

        let mut spill: HashSet<ValueId> = HashSet::new();
        for block in self.blocks.values() {
            for (value_id, instr) in &block.instrs {
                let use_count = uses.get(value_id).copied().unwrap_or(0);
                if use_count > 1 {
                    if matches!(instr, Instr::Const(lit) if is_cheap_const_literal(lit)) {
                        continue;
                    }
                    spill.insert(*value_id);
                    continue;
                }
                if instr.has_side_effects() && use_count > 0 {
                    spill.insert(*value_id);
                }
            }
        }
        spill.extend(cross_block_use.iter().copied());

        for id in cross_block_use.iter().copied() {
            let Some(def) = def_instr_vec.get(id.0).and_then(|x| x.as_ref()) else {
                continue;
            };
            if matches!(def, Instr::Const(lit) if is_cheap_const_literal(lit)) {
                spill.remove(&id);
            }
        }

        for block in self.blocks.values() {
            for (_, instr) in &block.instrs {
                if let Instr::Binary { left, right, .. } = instr {
                    if left == right {
                        if let ValueRef::Value(id) = left {
                            if uses.get(&id).copied() == Some(2) && !cross_block_use.contains(&id) {
                                spill.remove(&id);
                            }
                        }
                    }
                }
            }
        }

        spill.retain(|id| def_instr.contains_key(id));

        let mut next_local: u8 = 0;
        let mut value_slot: HashMap<ValueId, u8> = HashMap::new();
        for i in 0..self.value_count {
            let id = ValueId(i);
            if spill.contains(&id) {
                value_slot.insert(id, next_local);
                next_local = next_local
                    .checked_add(1)
                    .ok_or(CodegenError::LocalLimitExceeded)?;
                if next_local == u8::MAX {
                    return Err(CodegenError::LocalLimitExceeded);
                }
            }
        }

        let mut param_slot: HashMap<(BlockId, usize), u8> = HashMap::new();
        for (block_id, block) in &self.blocks {
            if *block_id == self.entry {
                continue;
            }
            for (index, _p) in block.params.iter().enumerate() {
                param_slot.insert((*block_id, index), next_local);
                next_local = next_local
                    .checked_add(1)
                    .ok_or(CodegenError::LocalLimitExceeded)?;
                if next_local == u8::MAX {
                    return Err(CodegenError::LocalLimitExceeded);
                }
            }
        }

        let mut compound_local_pairs: Vec<(u8, u8)> = Vec::new();
        for block in self.blocks.values() {
            for (_, instr) in &block.instrs {
                if matches!(instr, Instr::ContractMapStorageCompound { .. }) {
                    let key_slot = next_local;
                    next_local = next_local
                        .checked_add(1)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    if next_local == u8::MAX {
                        return Err(CodegenError::LocalLimitExceeded);
                    }
                    let value_slot_pair = next_local;
                    next_local = next_local
                        .checked_add(1)
                        .ok_or(CodegenError::LocalLimitExceeded)?;
                    if next_local == u8::MAX {
                        return Err(CodegenError::LocalLimitExceeded);
                    }
                    compound_local_pairs.push((key_slot, value_slot_pair));
                }
            }
        }

        Ok(StackifyPlan {
            uses,
            def_block,
            def_instr_vec,
            cross_block_use,
            spill,
            value_slot,
            param_slot,
            compound_local_pairs,
            next_local,
        })
    }
}
