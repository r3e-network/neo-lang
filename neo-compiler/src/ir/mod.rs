//! Neo-lang intermediate representation (IR).
//!
//! We use a small CFG-based IR with **block parameters** (Pruned SSA / SSA form B).
//! Minimal CFG IR for SSA (block-parameter form).

pub mod lower;
pub mod opt;

use std::collections::BTreeMap;

use crate::syntax::ast::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParamId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimTy {
    Bool,
    Int,
    String,
    Buffer,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueRef {
    Value(ValueId),
    Param(ParamId),
}

/// One instruction that defines a new SSA value.
#[derive(Debug, Clone, PartialEq)]
pub enum Instr {
    Const(Literal),
    /// `base.field` for neo-lang structs (represented as NeoVM arrays).
    StructFieldGet {
        base: ValueRef,
        index: usize,
    },
    /// `base[index]` for arrays and maps (NeoVM `PICKITEM`).
    IndexGet {
        base: ValueRef,
        index: ValueRef,
    },
    /// `SIZE(value)` for array/map/buffer/string (NeoVM `SIZE`).
    Size {
        value: ValueRef,
    },
    /// `KEYS(map)` (NeoVM `KEYS`) — returns an array of keys.
    Keys {
        map: ValueRef,
    },
    /// `VALUES(map)` (NeoVM `VALUES`) — returns an array of values.
    Values {
        map: ValueRef,
    },
    /// `HASKEY(map, key)` (NeoVM `HASKEY`) — returns bool.
    HasKey {
        map: ValueRef,
        key: ValueRef,
    },
    /// `APPEND(array, value)` (NeoVM `APPEND`); expression value is `array`.
    ArrayAppend {
        array: ValueRef,
        value: ValueRef,
    },
    /// `POPITEM(array)` (NeoVM `POPITEM`).
    ArrayPop {
        array: ValueRef,
    },
    /// `CLEARITEMS(collection)` (NeoVM `CLEARITEMS`); expression value is `collection`.
    ClearItems {
        collection: ValueRef,
    },
    /// `REMOVE(map, key)` (NeoVM `REMOVE`).
    Remove {
        map: ValueRef,
        key: ValueRef,
    },
    /// `SUBSTR(value, start, length)` (NeoVM `SUBSTR`).
    SubStr {
        value: ValueRef,
        start: ValueRef,
        length: ValueRef,
    },
    /// `SQRT(value)` (NeoVM `SQRT`).
    Sqrt {
        value: ValueRef,
    },
    /// `MODMUL(value, other, modulus)` (NeoVM `MODMUL`).
    ModMul {
        value: ValueRef,
        other: ValueRef,
        modulus: ValueRef,
    },
    /// `MODPOW(value, exponent, modulus)` (NeoVM `MODPOW`).
    ModPow {
        value: ValueRef,
        exponent: ValueRef,
        modulus: ValueRef,
    },
    /// `WITHIN(value, min_inclusive, max_exclusive)` (NeoVM `WITHIN`).
    Within {
        value: ValueRef,
        min_inclusive: ValueRef,
        max_exclusive: ValueRef,
    },
    /// `base[index] = value` (NeoVM `SETITEM`); result is `value`.
    IndexSet {
        base: ValueRef,
        index: ValueRef,
        value: ValueRef,
    },
    /// `base[field_index] = value` for struct fields (`self.f`, `p.f`).
    StructFieldSet {
        base: ValueRef,
        index: usize,
        value: ValueRef,
    },
    Unary {
        op: UnaryOp,
        value: ValueRef,
    },
    Binary {
        op: BinaryOp,
        left: ValueRef,
        right: ValueRef,
    },
    /// `expr as ty` → NeoVM `CONVERT` (same stack value, new logical type).
    Cast {
        value: ValueRef,
        ty: Type,
    },
    /// Copy is a first-class node to simplify copy propagation.
    Copy(ValueRef),
    /// `self.field` scalar read from contract storage (`System.Storage.Local.Get`).
    ContractStorageGet {
        field: String,
        value_ty: Type,
    },
    /// `self.field = value` scalar write; expression value is `value`.
    ContractStoragePut {
        field: String,
        value_ty: Type,
        value: ValueRef,
    },
    /// `self.map[key]` when `map` is a contract storage field.
    ContractMapStorageGet {
        field: String,
        key_ty: Type,
        val_ty: Type,
        key: ValueRef,
    },
    /// `self.map[key] = value` for contract storage map.
    ContractMapStoragePut {
        field: String,
        key_ty: Type,
        val_ty: Type,
        key: ValueRef,
        value: ValueRef,
    },
    /// `self.map[key] += value` (and other `AssignOp` for int values) on contract storage map.
    ContractMapStorageCompound {
        field: String,
        key_ty: Type,
        val_ty: Type,
        key: ValueRef,
        value: ValueRef,
        op: AssignOp,
    },
    /// `assert(cond, message)` (`ASSERTMSG`).
    Assert {
        cond: ValueRef,
        message: ValueRef,
    },
    /// `abort(message)` (`ABORTMSG`).
    Abort {
        message: ValueRef,
    },
    /// `min(a, b)` (`MIN`).
    Min {
        left: ValueRef,
        right: ValueRef,
    },
    /// `max(a, b)` (`MAX`).
    Max {
        left: ValueRef,
        right: ValueRef,
    },
    /// `emit name(args...)`.
    Emit {
        name: String,
        args: Vec<ValueRef>,
    },
    /// Top-level package function `name(args...)` → `CALL_L`.
    PackageCall {
        name: String,
        args: Vec<ValueRef>,
    },
    /// Struct literal `S { ... }` as NeoVM `PACK`.
    StructPack {
        struct_name: String,
        field_values: Vec<ValueRef>,
    },
    /// `recv.method(args...)` on a struct-typed variable → `CALL_L` to `Struct::method`.
    StructCall {
        struct_name: String,
        method: String,
        recv: ValueRef,
        args: Vec<ValueRef>,
    },
    /// `runtime.log(message)`.
    RuntimeLog {
        message: ValueRef,
    },
    /// `runtime.notify(event_name, state_array)` (Syscall `System.Runtime.Notify`).
    RuntimeNotify {
        event_name: ValueRef,
        state: ValueRef,
    },
    /// `runtime.contractCall(contract, method, params)` with injected read-only flags.
    ContractCallReadOnly {
        contract: ValueRef,
        method: ValueRef,
        params: ValueRef,
    },
    /// Array literal `[...]` as NeoVM `PACK`.
    ArrayPack {
        elements: Vec<ValueRef>,
    },
    /// Map literal `map[K,V]{...}` as NeoVM `PACKMAP`.
    MapPack {
        pairs: Vec<(ValueRef, ValueRef)>,
    },
    /// Opaque AST expression fallback (side-effects / not-yet-lowered constructs).
    EvalAst(Expr),
}

impl Instr {
    pub(crate) fn has_side_effects(&self) -> bool {
        matches!(
            self,
            Instr::IndexSet { .. }
                | Instr::StructFieldSet { .. }
                | Instr::ArrayAppend { .. }
                | Instr::ArrayPop { .. }
                | Instr::ClearItems { .. }
                | Instr::Remove { .. }
                | Instr::ContractStoragePut { .. }
                | Instr::ContractMapStoragePut { .. }
                | Instr::ContractMapStorageCompound { .. }
                | Instr::Assert { .. }
                | Instr::Abort { .. }
                | Instr::Emit { .. }
                | Instr::PackageCall { .. }
                | Instr::StructCall { .. }
                | Instr::RuntimeLog { .. }
                | Instr::RuntimeNotify { .. }
                | Instr::ContractCallReadOnly { .. }
                | Instr::EvalAst(_)
        )
    }

    /// Whether this instruction must be preserved even if its output value is unused.
    ///
    /// This is intentionally **more conservative** than `has_side_effects()` because some
    /// instructions may be observable (e.g. runtime/storage behavior) or required for
    /// correctness even when their result is not consumed.
    pub(crate) fn must_keep_even_if_unused(&self) -> bool {
        matches!(
            self,
            Instr::EvalAst(_)
                | Instr::IndexSet { .. }
                | Instr::StructFieldSet { .. }
                | Instr::ArrayAppend { .. }
                | Instr::ArrayPop { .. }
                | Instr::ClearItems { .. }
                | Instr::Remove { .. }
                | Instr::ContractStoragePut { .. }
                | Instr::ContractMapStoragePut { .. }
                | Instr::ContractMapStorageCompound { .. }
                | Instr::Assert { .. }
                | Instr::Abort { .. }
                | Instr::Emit { .. }
                | Instr::PackageCall { .. }
                | Instr::StructPack { .. }
                | Instr::StructCall { .. }
                | Instr::RuntimeLog { .. }
                | Instr::RuntimeNotify { .. }
                | Instr::ContractCallReadOnly { .. }
                // | Instr::ArrayPack { .. }
                // | Instr::MapPack { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    Return(Option<ValueRef>),
    Jump {
        target: BlockId,
        args: Vec<ValueRef>,
    },
    Branch {
        cond: ValueRef,
        then_bb: BlockId,
        then_args: Vec<ValueRef>,
        else_bb: BlockId,
        else_args: Vec<ValueRef>,
    },
}

#[derive(Debug, Clone)]
pub struct BlockParam {
    pub name: String,
    pub ty: PrimTy,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub params: Vec<BlockParam>,
    pub instrs: Vec<(ValueId, Instr)>,
    pub term: Terminator,
}

#[derive(Debug, Clone)]
pub struct FunctionIr {
    pub entry: BlockId,
    pub blocks: BTreeMap<BlockId, BasicBlock>,
    pub value_count: usize,
}
