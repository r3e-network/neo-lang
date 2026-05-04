//! Codegen from IR (CFG + block-parameter SSA) to NeoVM instructions.
//!
//! This module is split into multiple files to keep each concern small:
//! - `compile`: thin `FunctionIr::compile_ir` entry (patch jumps, `INITSLOT`, return `CompliledFunction`)
//! - `stackify_plan`: spill / slot allocation pre-pass
//! - `block_emit`: walk basic blocks and emit stackified instructions + terminators
//! - `branch_emit`: `Branch` terminator lowering (including early-return fold)
//! - `builder`: instruction emission helpers on `Builder`
//! - `analysis`: value-use accounting used by the stackifier
//! - `context`: shared immutable/mutable context structs for the above
//! - `compile_tests` (cfg test): `compile_function` / `compile_ir` branch lowering smoke tests

mod analysis;
mod block_emit;
mod branch_emit;
mod builder;
mod compile;
mod stackify_plan;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use crate::ir::{BlockId, Instr, ValueId};

/// Immutable IR + slot maps shared while emitting stackified code for one function.
pub(super) struct IrStackifyContext<'a> {
    pub(super) defs: &'a HashMap<ValueId, Instr>,
    pub(super) all_defs: &'a [Option<Instr>],
    pub(super) uses: &'a HashMap<ValueId, usize>,
    pub(super) spill: &'a HashSet<ValueId>,
    pub(super) value_slot: &'a HashMap<ValueId, u8>,
    pub(super) param_slot: &'a HashMap<(BlockId, usize), u8>,
    pub(super) entry_bb: BlockId,
    pub(super) arg_count: u8,
}

/// Mutable scratch for side-effecting IR emission (compound map ops + `CALL_L` patches).
pub(super) struct IrSideEffectMux<'a> {
    pub(super) compound_pairs: &'a [(u8, u8)],
    pub(super) compound_index: &'a mut usize,
    pub(super) call_patches: &'a mut Vec<(usize, String)>,
}
