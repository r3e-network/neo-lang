//! Built-in neo-lang functions (`assert`, `abort`, `min`, `max`, ...).
//!
//! Each binding describes source arguments and a fixed emit plan: push args onto the
//! stack in order, then execute one or more NeoVM opcodes.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::syntax::ast::Type;
use crate::target::opcode::OpCode;
use crate::target::StackItemType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinEmitStep {
    SourceArg(usize),
    Op(OpCode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinCse {
    Min,
    Max,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltinBinding {
    pub name: &'static str,
    pub args: &'static [StackItemType],
    pub return_type: Option<StackItemType>,
    pub emit_plan: &'static [BuiltinEmitStep],
    pub has_side_effects: bool,
    pub cse: Option<BuiltinCse>,
}

impl BuiltinBinding {
    pub const fn new(
        name: &'static str,
        args: &'static [StackItemType],
        return_type: Option<StackItemType>,
        emit_plan: &'static [BuiltinEmitStep],
        has_side_effects: bool,
        cse: Option<BuiltinCse>,
    ) -> Self {
        Self {
            name,
            args,
            return_type,
            emit_plan,
            has_side_effects,
            cse,
        }
    }

    pub fn source_arg_count(self) -> usize {
        self.args.len()
    }

    pub fn source_arg_type(self, index: usize) -> StackItemType {
        self.args[index]
    }

    pub fn return_lang_type(self) -> Type {
        match self.return_type {
            None => Type::Void,
            Some(sit) => sit.to_lang_type(),
        }
    }

    pub fn leaves_stack_value(self) -> bool {
        self.return_type.is_some()
    }

    pub fn arg_type_matches(self, index: usize, ty: &Type) -> bool {
        self.args[index].satisfies_lang_type(ty)
    }
}

const ASSERT_EMIT: &[BuiltinEmitStep] = &[
    BuiltinEmitStep::SourceArg(0),
    BuiltinEmitStep::SourceArg(1),
    BuiltinEmitStep::Op(OpCode::ASSERTMSG),
];

const ABORT_EMIT: &[BuiltinEmitStep] = &[
    BuiltinEmitStep::SourceArg(0),
    BuiltinEmitStep::Op(OpCode::ABORTMSG),
];

const MIN_EMIT: &[BuiltinEmitStep] = &[
    BuiltinEmitStep::SourceArg(0),
    BuiltinEmitStep::SourceArg(1),
    BuiltinEmitStep::Op(OpCode::MIN),
];

const MAX_EMIT: &[BuiltinEmitStep] = &[
    BuiltinEmitStep::SourceArg(0),
    BuiltinEmitStep::SourceArg(1),
    BuiltinEmitStep::Op(OpCode::MAX),
];

pub const BUILTIN_BINDINGS: &[BuiltinBinding] = &[
    BuiltinBinding::new(
        "assert",
        &[StackItemType::Boolean, StackItemType::ByteString],
        None,
        ASSERT_EMIT,
        true,
        None,
    ),
    BuiltinBinding::new(
        "abort",
        &[StackItemType::ByteString],
        None,
        ABORT_EMIT,
        true,
        None,
    ),
    BuiltinBinding::new(
        "min",
        &[StackItemType::Integer, StackItemType::Integer],
        Some(StackItemType::Integer),
        MIN_EMIT,
        false,
        Some(BuiltinCse::Min),
    ),
    BuiltinBinding::new(
        "max",
        &[StackItemType::Integer, StackItemType::Integer],
        Some(StackItemType::Integer),
        MAX_EMIT,
        false,
        Some(BuiltinCse::Max),
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltinMethod(pub &'static BuiltinBinding);

impl BuiltinMethod {
    pub fn resolve(name: &str) -> Option<Self> {
        builtin_binding_for_name(name).map(Self)
    }

    pub fn binding(self) -> &'static BuiltinBinding {
        self.0
    }

    pub fn source_arg_count(self) -> usize {
        self.0.source_arg_count()
    }

    pub fn return_lang_type(self) -> Type {
        self.0.return_lang_type()
    }

    pub fn leaves_stack_value(self) -> bool {
        self.0.leaves_stack_value()
    }

    pub fn has_side_effects(self) -> bool {
        self.0.has_side_effects
    }

    pub fn emit_plan(self) -> &'static [BuiltinEmitStep] {
        self.0.emit_plan
    }

    pub fn cse(self) -> Option<BuiltinCse> {
        self.0.cse
    }
}

static BUILTIN_BINDING_BY_NAME: LazyLock<HashMap<&'static str, &'static BuiltinBinding>> =
    LazyLock::new(|| BUILTIN_BINDINGS.iter().map(|b| (b.name, b)).collect());

pub fn builtin_binding_for_name(name: &str) -> Option<&'static BuiltinBinding> {
    BUILTIN_BINDING_BY_NAME.get(name).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_binding_emits_min_opcode() {
        let binding = builtin_binding_for_name("min").expect("min");
        assert_eq!(
            binding.emit_plan.last(),
            Some(&BuiltinEmitStep::Op(OpCode::MIN))
        );
    }
}
