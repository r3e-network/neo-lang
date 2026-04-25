//! Per-function local and argument slots (`INITSLOT` / `LDLOC` / `LDARG` / …).

use std::collections::HashMap;

use crate::codegen::CodegenError;
use crate::syntax::ast::Param;
use crate::target::opcode::OpCode;
use crate::target::Builder;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Slot {
    Arg(u8),
    Local(u8),
}

pub(crate) struct VarEnv {
    args: HashMap<String, u8>,
    scopes: Vec<HashMap<String, u8>>,
    pub(crate) next_local: u8,
}

impl VarEnv {
    pub(crate) fn new(params: &[Param]) -> Result<Self, CodegenError> {
        if params.len() > u8::MAX as usize {
            return Err(CodegenError::LocalLimitExceeded);
        }
        let mut args = HashMap::new();
        for (index, param) in params.iter().enumerate() {
            args.insert(param.name.clone(), index as u8);
        }
        Ok(Self {
            args,
            scopes: vec![HashMap::new()],
            next_local: 0,
        })
    }

    pub(crate) fn enter_block(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(crate) fn exit_block(&mut self) {
        self.scopes.pop();
        debug_assert!(!self.scopes.is_empty());
    }

    pub(crate) fn declare_local(&mut self, name: &str) -> Result<u8, CodegenError> {
        let slot = self.next_local;
        if slot == u8::MAX {
            return Err(CodegenError::LocalLimitExceeded);
        }
        let top = self.scopes.last_mut().expect("scope stack");
        if top.insert(name.to_string(), slot).is_some() {
            return Err(CodegenError::DuplicateLocal(name.to_string()));
        }
        self.next_local = self
            .next_local
            .checked_add(1)
            .ok_or(CodegenError::LocalLimitExceeded)?;
        Ok(slot)
    }

    pub(crate) fn resolve(&self, name: &str) -> Result<Slot, CodegenError> {
        for scope in self.scopes.iter().rev() {
            if let Some(&slot) = scope.get(name) {
                return Ok(Slot::Local(slot));
            }
        }
        if let Some(&index) = self.args.get(name) {
            return Ok(Slot::Arg(index));
        }
        Err(CodegenError::UndefinedVariable(name.to_string()))
    }

    pub(crate) fn local_count(&self) -> u8 {
        self.next_local
    }

    pub(crate) fn alloc_slot(&mut self) -> Result<u8, CodegenError> {
        let slot = self.next_local;
        if slot == u8::MAX {
            return Err(CodegenError::LocalLimitExceeded);
        }
        self.next_local = self
            .next_local
            .checked_add(1)
            .ok_or(CodegenError::LocalLimitExceeded)?;
        Ok(slot)
    }
}

impl Builder {
    pub(crate) fn emit_ldslot(&mut self, slot: Slot) {
        match slot {
            Slot::Arg(index) => self.emit_ldarg(index),
            Slot::Local(index) => self.emit_ldloc(index),
        }
    }

    pub(crate) fn emit_ldarg(&mut self, index: u8) {
        match index {
            0 => self.emit(OpCode::LDARG0),
            1 => self.emit(OpCode::LDARG1),
            2 => self.emit(OpCode::LDARG2),
            3 => self.emit(OpCode::LDARG3),
            4 => self.emit(OpCode::LDARG4),  
            5 => self.emit(OpCode::LDARG5),
            6 => self.emit(OpCode::LDARG6),
            _ => self.emit_with_operands(OpCode::LDARG, std::slice::from_ref(&index)),
        }
    }

    pub(crate) fn emit_ldloc(&mut self, index: u8) {
        match index {
            0 => self.emit(OpCode::LDLOC0),
            1 => self.emit(OpCode::LDLOC1),
            2 => self.emit(OpCode::LDLOC2),
            3 => self.emit(OpCode::LDLOC3),
            4 => self.emit(OpCode::LDLOC4),
            5 => self.emit(OpCode::LDLOC5),
            6 => self.emit(OpCode::LDLOC6),
            _ => self.emit_with_operands(OpCode::LDLOC, std::slice::from_ref(&index)),
        }
    }

    pub(crate) fn emit_stloc(&mut self, index: u8) {
        match index {
            0 => self.emit(OpCode::STLOC0),
            1 => self.emit(OpCode::STLOC1),
            2 => self.emit(OpCode::STLOC2),
            3 => self.emit(OpCode::STLOC3),
            4 => self.emit(OpCode::STLOC4),
            5 => self.emit(OpCode::STLOC5),
            6 => self.emit(OpCode::STLOC6),
            _ => self.emit_with_operands(OpCode::STLOC, std::slice::from_ref(&index)),
        }
    }

    pub(crate) fn emit_starg(&mut self, index: u8) {
        match index {
            0 => self.emit(OpCode::STARG0),
            1 => self.emit(OpCode::STARG1),
            2 => self.emit(OpCode::STARG2),
            3 => self.emit(OpCode::STARG3),
            4 => self.emit(OpCode::STARG4),
            5 => self.emit(OpCode::STARG5),
            6 => self.emit(OpCode::STARG6),
            _ => self.emit_with_operands(OpCode::STARG, std::slice::from_ref(&index)),
        }
    }
}
