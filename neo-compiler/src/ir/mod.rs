//! Neo-lang intermediate representation (IR).
//!
//! We use a small CFG-based IR with **block parameters** (Pruned SSA / SSA form B).
//! Minimal CFG IR for SSA (block-parameter form).

pub mod lower;
pub mod opt;

use std::collections::BTreeMap;

use crate::syntax::ast::*;
use crate::target::builtin::BuiltinMethod;
use crate::target::natives::NativeContract;
use crate::target::syscall::RuntimeMethod;

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
    /// `expr == null` or `expr != null` via ISNULL (and NOT for `!=`).
    IsNull {
        value: ValueRef,
        eq_null: bool,
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
    /// `self.map.has(key)` for a contract storage map (composite key exists in local storage).
    ContractMapStorageHas {
        field: String,
        key_ty: Type,
        key: ValueRef,
    },
    /// `self.map.remove(key)` for a contract storage map.
    ContractMapStorageRemove {
        field: String,
        key_ty: Type,
        key: ValueRef,
    },
    /// Built-in call (`assert`, `abort`, `min`, `max`, ...) lowered from [`BuiltinMethod`] metadata.
    BuiltinCall {
        builtin: BuiltinMethod,
        args: Vec<ValueRef>,
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
    /// `self.method(args...)` within the same contract → `CALL_L` to `Contract::method` (no `self` arg).
    ContractMethodCall {
        contract_name: String,
        method: String,
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
    /// `runtime.<method>(args...)` lowered from syscall metadata ([`RuntimeMethod`]).
    RuntimeCall {
        method: RuntimeMethod,
        args: Vec<ValueRef>,
    },
    /// `NativeContract.method(args...)` via NEF method token + [`OpCode::CALLT`].
    NativeCall {
        contract: &'static NativeContract,
        method: String,
        args: Vec<ValueRef>,
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
        match self {
            Instr::BuiltinCall { builtin, .. } => builtin.has_side_effects(),
            Instr::IndexSet { .. }
            | Instr::StructFieldSet { .. }
            | Instr::ArrayAppend { .. }
            | Instr::ArrayPop { .. }
            | Instr::ClearItems { .. }
            | Instr::Remove { .. }
            | Instr::ContractStoragePut { .. }
            | Instr::ContractMapStoragePut { .. }
            | Instr::ContractMapStorageCompound { .. }
            | Instr::ContractMapStorageRemove { .. }
            | Instr::Emit { .. }
            | Instr::PackageCall { .. }
            | Instr::ContractMethodCall { .. }
            | Instr::StructCall { .. }
            | Instr::RuntimeCall { .. }
            | Instr::NativeCall { .. }
            | Instr::EvalAst(_) => true,
            _ => false,
        }
    }

    /// Whether this instruction must be preserved even if its output value is unused.
    ///
    /// This is intentionally **more conservative** than `has_side_effects()` because some
    /// instructions may be observable (e.g. runtime/storage behavior) or required for
    /// correctness even when their result is not consumed.
    pub(crate) fn must_keep_even_if_unused(&self) -> bool {
        match self {
            Instr::BuiltinCall { builtin, .. } => builtin.has_side_effects(),
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
            | Instr::ContractMapStorageRemove { .. }
            | Instr::Emit { .. }
            | Instr::PackageCall { .. }
            | Instr::ContractMethodCall { .. }
            | Instr::StructPack { .. }
            | Instr::StructCall { .. }
            | Instr::RuntimeCall { .. }
            | Instr::NativeCall { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    /// Placeholder for a new [`BasicBlock`] before its terminator is set. Open exits are sealed in
    /// [`lower_function_to_ir`](crate::ir::lower::lower_function_to_ir) when needed.
    Unset,
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
