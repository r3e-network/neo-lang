//! NeoVM is a stack-based virtual machine.
//! NeoVM has three types of stack: InvocationStack, EvaluationStack, and ResultStack.
//!
//! InvocationStack is used to store all execution contexts, which are isolated from each other in the stack.
//! Each Call, CallA, CallT will push a new execution context to the invocation stack, and each context has
//! its own arguments slots, local variables slots, and all contexts share the same static field slots.
//!
//! EvaluationStack is for storing the data used by the instruction in execution process.
//! Each execution context has its own evaluation stack.
//!
//! ResultStack is used to store execution result after all scripts are executed.
//!
//! From https://developers.neo.org/docs/n3/foundation/neovm

pub mod builtin;
pub mod cost;
pub mod nef;
pub mod opcode;
pub mod syscall;

use opcode::OpCode;
use syscall::Syscall;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StackItemType {
    Any = 0x00,
    Pointer = 0x10,
    Boolean = 0x20,
    Integer = 0x21,
    ByteString = 0x28,
    Buffer = 0x30,
    Array = 0x40,
    Map = 0x48,
    InteropInterface = 0x60,
}

/// An Instruction in NeoVM(NeoVM is a stack-based virtual machine).
/// Each instruction is a single byte opcode followed by optional operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub opcode: OpCode,

    // Serialized operands, and empty means no operands
    pub operands: Vec<u8>,
}

impl Instruction {
    /// Total size of this instruction in the final script (opcode byte + operands).
    #[inline]
    pub fn encoded_len(&self) -> usize {
        1 + self.operands.len()
    }

    pub fn encode_into(&self, out: &mut Vec<u8>) {
        out.push(self.opcode as u8);
        out.extend_from_slice(&self.operands);
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(1 + self.operands.len());
        self.encode_into(&mut v);
        v
    }
}

/// Accumulates NeoVM [`Instruction`]s during codegen. Raw script bytes are produced only
/// by [`Self::encode_into`], [`Self::to_bytes`], or [`Self::into_bytes`].
///
/// Jump offsets are measured from the **start of the branch/call instruction** (the opcode byte), matching NeoVM semantics.
#[derive(Debug, Default, Clone)]
pub struct Builder {
    instructions: Vec<Instruction>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    #[inline]
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    #[inline]
    pub fn instruction_count(&self) -> usize {
        self.instructions.len()
    }

    /// Byte length of the script if serialized now (cursor for the next would-be byte).
    pub fn bytecode_len(&self) -> usize {
        self.instructions.iter().map(Instruction::encoded_len).sum()
    }

    /// Same as [`Self::bytecode_len`].
    #[inline]
    pub fn cursor(&self) -> usize {
        self.bytecode_len()
    }

    /// Byte offset in the serialized script of the instruction at `instruction_index`.
    pub fn bytecode_offset_of(&self, instruction_index: usize) -> usize {
        self.instructions[..instruction_index]
            .iter()
            .map(Instruction::encoded_len)
            .sum()
    }

    pub fn encode_into(&self, out: &mut Vec<u8>) {
        for inst in &self.instructions {
            inst.encode_into(out);
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.bytecode_len());
        self.encode_into(&mut v);
        v
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let cap: usize = self.instructions.iter().map(Instruction::encoded_len).sum();
        let mut v = Vec::with_capacity(cap);
        for inst in self.instructions {
            inst.encode_into(&mut v);
        }
        v
    }

    #[inline]
    pub fn emit(&mut self, op: OpCode) {
        self.push(Instruction {
            opcode: op,
            operands: Vec::new(),
        });
    }

    #[inline]
    pub fn emit_with_operands(&mut self, op: OpCode, operands: &[u8]) {
        self.push(Instruction {
            opcode: op,
            operands: operands.to_vec(),
        });
    }

    pub fn push(&mut self, inst: Instruction) {
        self.instructions.push(inst);
    }

    /// `System.Crypto.*` / `System.Runtime.*` etc.: opcode `SYSCALL` + 4-byte LE syscall id.
    pub fn emit_syscall(&mut self, syscall: Syscall) {
        self.emit_with_operands(OpCode::SYSCALL, &syscall.token().to_le_bytes());
    }

    pub fn push_bool(&mut self, v: bool) {
        self.emit(if v { OpCode::PUSHT } else { OpCode::PUSHF });
    }

    pub fn push_null(&mut self) {
        self.emit(OpCode::PUSHNULL);
    }

    /// Smallest fixed PUSH* for an integer (`PUSHM1` … `PUSH16`, then `PUSHINT8` … `PUSHINT64`).
    /// Wider literals use [`Self::push_int128`] / [`Self::push_int256`].
    pub fn push_int(&mut self, n: i64) {
        match n {
            -1 => self.emit(OpCode::PUSHM1),
            0 => self.emit(OpCode::PUSH0),
            1 => self.emit(OpCode::PUSH1),
            2 => self.emit(OpCode::PUSH2),
            3 => self.emit(OpCode::PUSH3),
            4 => self.emit(OpCode::PUSH4),
            5 => self.emit(OpCode::PUSH5),
            6 => self.emit(OpCode::PUSH6),
            7 => self.emit(OpCode::PUSH7),
            8 => self.emit(OpCode::PUSH8),
            9 => self.emit(OpCode::PUSH9),
            10 => self.emit(OpCode::PUSH10),
            11 => self.emit(OpCode::PUSH11),
            12 => self.emit(OpCode::PUSH12),
            13 => self.emit(OpCode::PUSH13),
            14 => self.emit(OpCode::PUSH14),
            15 => self.emit(OpCode::PUSH15),
            16 => self.emit(OpCode::PUSH16),
            _ if n >= i8::MIN as i64 && n <= i8::MAX as i64 => self.push(Instruction {
                opcode: OpCode::PUSHINT8,
                operands: vec![n as i8 as u8],
            }),
            _ if n >= i16::MIN as i64 && n <= i16::MAX as i64 => self.push(Instruction {
                opcode: OpCode::PUSHINT16,
                operands: (n as i16).to_le_bytes().to_vec(),
            }),
            _ if n >= i32::MIN as i64 && n <= i32::MAX as i64 => self.push(Instruction {
                opcode: OpCode::PUSHINT32,
                operands: (n as i32).to_le_bytes().to_vec(),
            }),
            _ => self.push(Instruction {
                opcode: OpCode::PUSHINT64,
                operands: n.to_le_bytes().to_vec(),
            }),
        }
    }

    /// `PUSHINT128`: 16-byte signed integer, little-endian (two's complement).
    pub fn push_int128(&mut self, n: i128) {
        self.push(Instruction {
            opcode: OpCode::PUSHINT128,
            operands: n.to_le_bytes().to_vec(),
        });
    }

    /// `PUSHINT256`: 32-byte signed integer, little-endian (two's complement), as in NeoVM.
    pub fn push_int256(&mut self, n: &[u8; 32]) {
        self.push(Instruction {
            opcode: OpCode::PUSHINT256,
            operands: n.to_vec(),
        });
    }

    /// Byte string / buffer literal: `PUSHDATA1` / `PUSHDATA2` / `PUSHDATA4` + payload.
    pub fn push_data(&mut self, data: &[u8]) {
        let len = data.len();
        let mut operands = Vec::new();
        let op = if len <= u8::MAX as usize {
            operands.push(len as u8);
            OpCode::PUSHDATA1
        } else if len <= u16::MAX as usize {
            operands.extend_from_slice(&(len as u16).to_le_bytes());
            OpCode::PUSHDATA2
        } else if len <= u32::MAX as usize {
            operands.extend_from_slice(&(len as u32).to_le_bytes());
            OpCode::PUSHDATA4
        } else {
            panic!("data length too long");
        };
        operands.extend_from_slice(data);
        self.push(Instruction {
            opcode: op,
            operands,
        });
    }

    /// Patch the 4-byte LE relative offset on the instruction at `jump_instruction_index`
    /// (`target_byte_offset - bytecode_offset_of(jump_instruction_index)`).
    pub fn patch_jmp_target_at_instruction(
        &mut self,
        jump_instruction_index: usize,
        target_byte_offset: usize,
    ) {
        let jump_pc = self.bytecode_offset_of(jump_instruction_index);
        let inst = self
            .instructions
            .get_mut(jump_instruction_index)
            .expect("jump_instruction_index out of range");
        assert_eq!(
            inst.operands.len(),
            4,
            "instruction at index must have a 4-byte placeholder"
        );
        let relative = target_byte_offset as i64 - jump_pc as i64;
        let relative = i32::try_from(relative).expect("jump offset overflow");
        inst.operands.copy_from_slice(&relative.to_le_bytes());
    }

    /// Emit `JMP_L` with a placeholder operand; returns the instruction index for [`Self::patch_jmp_target_at_instruction`].
    pub fn emit_jmp_l_placeholder(&mut self) -> usize {
        let index = self.instructions.len();
        self.push(Instruction {
            opcode: OpCode::JMP_L,
            operands: vec![0u8; 4],
        });
        index
    }

    /// `CALL_L` with a 4-byte LE relative offset placeholder (patched like [`Self::patch_jmp_target_at_instruction`]).
    pub fn emit_call_l_placeholder(&mut self) -> usize {
        let index = self.instructions.len();
        self.push(Instruction {
            opcode: OpCode::CALL_L,
            operands: vec![0u8; 4],
        });
        index
    }

    /// Patch `CALL_L` at `call_instruction_index` to jump to `target_byte_offset` (same relative encoding as `JMP_L`).
    pub fn patch_call_l_target_at_instruction(
        &mut self,
        call_instruction_index: usize,
        target_byte_offset: usize,
    ) {
        let inst = self
            .instructions
            .get_mut(call_instruction_index)
            .expect("call_instruction_index out of range");
        assert_eq!(
            inst.opcode,
            OpCode::CALL_L,
            "instruction at index must be CALL_L"
        );
        self.patch_jmp_target_at_instruction(call_instruction_index, target_byte_offset);
    }

    /// Same as [`Self::emit_jmp_l_placeholder`] for `JMPIF_L`.
    pub fn emit_jmpif_l_placeholder(&mut self) -> usize {
        let index = self.instructions.len();
        self.push(Instruction {
            opcode: OpCode::JMPIF_L,
            operands: vec![0u8; 4],
        });
        index
    }

    /// Same as [`Self::emit_jmp_l_placeholder`] for `JMPIFNOT_L`.
    pub fn emit_jmpifnot_l_placeholder(&mut self) -> usize {
        let index = self.instructions.len();
        self.push(Instruction {
            opcode: OpCode::JMPIFNOT_L,
            operands: vec![0u8; 4],
        });
        index
    }

    pub fn emit_initslot(&mut self, local_var_count: u8, arg_count: u8) {
        self.emit_with_operands(OpCode::INITSLOT, &[local_var_count, arg_count]);
    }

    /// Patch the first operand (local variable count) of an `INITSLOT` at `instruction_index`.
    pub fn patch_initslot_local_count(&mut self, instruction_index: usize, local_var_count: u8) {
        let inst = self
            .instructions
            .get_mut(instruction_index)
            .expect("instruction_index out of range");
        assert_eq!(inst.opcode, OpCode::INITSLOT, "not an INITSLOT");
        assert_eq!(inst.operands.len(), 2);
        inst.operands[0] = local_var_count;
    }

    pub fn into_instructions(self) -> Vec<Instruction> {
        self.instructions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opcode::OpCode;

    #[test]
    fn instruction_encoded_len_and_encode() {
        let i = Instruction {
            opcode: OpCode::NOP,
            operands: vec![],
        };
        assert_eq!(i.encoded_len(), 1);
        assert_eq!(i.to_bytes(), &[OpCode::NOP as u8]);

        let i2 = Instruction {
            opcode: OpCode::PUSHINT32,
            operands: (-1i32).to_le_bytes().to_vec(),
        };
        assert_eq!(i2.encoded_len(), 5);
        let mut buf = vec![0xAA, 0xBB];
        i.encode_into(&mut buf);
        i2.encode_into(&mut buf);
        assert_eq!(
            buf,
            vec![
                0xAA,
                0xBB,
                OpCode::NOP as u8,
                OpCode::PUSHINT32 as u8,
                0xFF,
                0xFF,
                0xFF,
                0xFF,
            ]
        );
    }

    #[test]
    fn builder_new_is_empty_and_default_matches() {
        let b = Builder::new();
        assert!(b.is_empty());
        assert_eq!(b.bytecode_len(), 0);
        assert_eq!(b.cursor(), 0);
        assert_eq!(b.to_bytes(), Vec::<u8>::new());

        let d = Builder::default();
        assert!(d.is_empty());
        assert_eq!(d.to_bytes(), Vec::<u8>::new());
    }

    #[test]
    fn bytecode_offset_of_and_cursor_track_encoding() {
        let mut b = Builder::new();
        b.emit(OpCode::PUSH1); // 1 byte
        b.push_int(1000); // PUSHINT16 → 3 bytes
        assert_eq!(b.bytecode_offset_of(0), 0);
        assert_eq!(b.bytecode_offset_of(1), 1);
        assert_eq!(b.cursor(), 4);
        assert_eq!(b.bytecode_len(), 4);
    }

    #[test]
    fn encode_into_appends_full_script() {
        let mut b = Builder::new();
        b.emit(OpCode::DROP);
        let mut buf = vec![0xCC];
        b.encode_into(&mut buf);
        assert_eq!(buf, vec![0xCC, OpCode::DROP as u8]);
        b.encode_into(&mut buf);
        assert_eq!(buf, vec![0xCC, OpCode::DROP as u8, OpCode::DROP as u8]);
    }

    #[test]
    fn to_bytes_and_into_bytes_match() {
        let mut b = Builder::new();
        b.push_bool(false);
        b.emit(OpCode::RET);
        let v1 = b.to_bytes();
        let b2 = {
            let mut x = Builder::new();
            x.push_bool(false);
            x.emit(OpCode::RET);
            x
        };
        let v2 = b2.into_bytes();
        assert_eq!(v1, v2);
        assert_eq!(v1, vec![OpCode::PUSHF as u8, OpCode::RET as u8]);
    }

    #[test]
    fn clone_preserves_instructions() {
        let mut b = Builder::new();
        b.push_int(7);
        let c = b.clone();
        assert_eq!(b.to_bytes(), c.to_bytes());
        assert_eq!(b.instruction_count(), c.instruction_count());
    }

    #[test]
    fn push_small_ints_use_short_opcodes() {
        let mut b = Builder::new();
        b.push_int(-1);
        b.push_int(0);
        b.push_int(16);
        assert_eq!(b.instruction_count(), 3);
        assert_eq!(b.instructions()[1].opcode, OpCode::PUSH0);
        assert_eq!(
            b.to_bytes(),
            &[
                OpCode::PUSHM1 as u8,
                OpCode::PUSH0 as u8,
                OpCode::PUSH16 as u8
            ]
        );
    }

    #[test]
    fn push_int_uses_pushint8_bounds() {
        let mut b = Builder::new();
        b.push_int(42);
        b.push_int(i8::MIN as i64);
        b.push_int(i8::MAX as i64);
        assert_eq!(b.instructions()[0].opcode, OpCode::PUSHINT8);
        assert_eq!(b.instructions()[1].opcode, OpCode::PUSHINT8);
        assert_eq!(b.instructions()[2].opcode, OpCode::PUSHINT8);
        let bytes = b.to_bytes();
        assert_eq!(bytes[1], 42u8);
        assert_eq!(&bytes[2..4], &[OpCode::PUSHINT8 as u8, 0x80]);
        assert_eq!(&bytes[4..6], &[OpCode::PUSHINT8 as u8, 0x7F]);
    }

    #[test]
    fn push_int_uses_pushint16() {
        let mut b = Builder::new();
        let v = i8::MAX as i64 + 1;
        b.push_int(v);
        assert_eq!(b.instructions()[0].opcode, OpCode::PUSHINT16);
        let bytes = b.to_bytes();
        assert_eq!(bytes[0], OpCode::PUSHINT16 as u8);
        assert_eq!(&bytes[1..3], &(v as i16).to_le_bytes());
    }

    #[test]
    fn push_int_uses_pushint32() {
        let mut b = Builder::new();
        let v = i16::MAX as i64 + 1;
        b.push_int(v);
        assert_eq!(b.instructions()[0].opcode, OpCode::PUSHINT32);
        let bytes = b.to_bytes();
        assert_eq!(&bytes[1..5], &(v as i32).to_le_bytes());
    }

    #[test]
    fn push_int_uses_pushint64() {
        let mut b = Builder::new();
        let v = i32::MAX as i64 + 1;
        b.push_int(v);
        assert_eq!(b.instructions()[0].opcode, OpCode::PUSHINT64);
        let bytes = b.to_bytes();
        assert_eq!(&bytes[1..9], &v.to_le_bytes());
    }

    #[test]
    fn push_int128_encoding() {
        let mut b = Builder::new();
        b.push_int128(0);
        b.push_int128(-1);
        b.push_int128(i128::MIN);
        b.push_int128(i128::MAX);
        let enc = b.to_bytes();
        assert_eq!(&enc[0..1], &[OpCode::PUSHINT128 as u8]);
        assert_eq!(&enc[1..17], &0_i128.to_le_bytes());
        assert_eq!(&enc[17..18], &[OpCode::PUSHINT128 as u8]);
        assert_eq!(&enc[18..34], &(-1_i128).to_le_bytes());
        assert_eq!(&enc[34..35], &[OpCode::PUSHINT128 as u8]);
        assert_eq!(&enc[35..51], &i128::MIN.to_le_bytes());
        assert_eq!(&enc[51..52], &[OpCode::PUSHINT128 as u8]);
        assert_eq!(&enc[52..68], &i128::MAX.to_le_bytes());
    }

    #[test]
    fn push_int256_encoding() {
        let mut b = Builder::new();
        let zero = [0u8; 32];
        let mut one = [0u8; 32];
        one[0] = 1;
        let neg1 = [0xFF; 32];
        b.push_int256(&zero);
        b.push_int256(&one);
        b.push_int256(&neg1);
        let enc = b.to_bytes();
        assert_eq!(enc.len(), 3 * (1 + 32));
        assert_eq!(enc[0], OpCode::PUSHINT256 as u8);
        assert_eq!(&enc[1..33], &zero[..]);
        assert_eq!(enc[33], OpCode::PUSHINT256 as u8);
        assert_eq!(&enc[34..66], &one[..]);
        assert_eq!(enc[66], OpCode::PUSHINT256 as u8);
        assert_eq!(&enc[67..99], &neg1[..]);
    }

    #[test]
    fn push_bool_and_push_null() {
        let mut b = Builder::new();
        b.push_bool(true);
        b.push_bool(false);
        b.push_null();
        assert_eq!(
            b.to_bytes(),
            vec![
                OpCode::PUSHT as u8,
                OpCode::PUSHF as u8,
                OpCode::PUSHNULL as u8,
            ]
        );
    }

    #[test]
    fn push_custom_instruction_and_emit_with_operands() {
        let mut b = Builder::new();
        b.emit_with_operands(OpCode::ISTYPE, &[0x12]);
        b.push(Instruction {
            opcode: OpCode::CONVERT,
            operands: vec![0x34],
        });
        assert_eq!(
            b.to_bytes(),
            vec![OpCode::ISTYPE as u8, 0x12, OpCode::CONVERT as u8, 0x34,]
        );
    }

    #[test]
    fn push_data_empty_pushdata1() {
        let mut b = Builder::new();
        b.push_data(&[]);
        assert_eq!(b.instructions()[0].opcode, OpCode::PUSHDATA1);
        assert_eq!(b.to_bytes(), vec![OpCode::PUSHDATA1 as u8, 0x00]);
    }

    #[test]
    fn push_data_small_uses_pushdata1() {
        let mut b = Builder::new();
        b.push_data(&[1, 2, 3]);
        assert_eq!(b.to_bytes(), vec![OpCode::PUSHDATA1 as u8, 0x03, 1, 2, 3]);
    }

    #[test]
    fn push_data_256_bytes_uses_pushdata2() {
        let payload: Vec<u8> = (0_u8..=255).collect();
        let mut b = Builder::new();
        b.push_data(&payload);
        assert_eq!(b.instructions()[0].opcode, OpCode::PUSHDATA2);
        let bytes = b.to_bytes();
        assert_eq!(&bytes[0..3], &[OpCode::PUSHDATA2 as u8, 0x00, 0x01]);
        assert_eq!(&bytes[3..], payload.as_slice());
    }

    #[test]
    fn push_data_max_u8_length_still_pushdata1() {
        let payload = vec![0xAB; u8::MAX as usize];
        let mut b = Builder::new();
        b.push_data(&payload);
        let bytes = b.to_bytes();
        assert_eq!(bytes[0], OpCode::PUSHDATA1 as u8);
        assert_eq!(bytes[1], u8::MAX);
        assert_eq!(bytes.len(), 2 + payload.len());
    }

    #[test]
    fn syscall_encoding() {
        let mut b = Builder::new();
        b.emit_syscall(Syscall::RUNTIME_PLATFORM);
        let bytes = b.to_bytes();
        assert_eq!(bytes[0], OpCode::SYSCALL as u8);
        assert_eq!(
            &bytes[1..5],
            &Syscall::RUNTIME_PLATFORM.token().to_le_bytes()
        );
    }

    #[test]
    fn syscall_distinct_tokens_differ() {
        let t1 = Syscall::RUNTIME_GET_TIME.token();
        let t2 = Syscall::CRYPTO_CHECK_SIG.token();
        assert_ne!(t1, t2);

        let mut b = Builder::new();
        b.emit_syscall(Syscall::RUNTIME_GET_TIME);
        b.emit_syscall(Syscall::RUNTIME_LOG);
        let bytes = b.to_bytes();
        assert_eq!(&bytes[1..5], &t1.to_le_bytes());
        assert_eq!(&bytes[6..10], &Syscall::RUNTIME_LOG.token().to_le_bytes());
    }

    #[test]
    fn patch_jmp_l_forward() {
        let mut b = Builder::new();
        let index = b.emit_jmp_l_placeholder();
        b.emit(OpCode::PUSH0);
        let target = b.cursor();
        b.patch_jmp_target_at_instruction(index, target);
        let jump_pc = b.bytecode_offset_of(index);
        let offset = (target as i32) - (jump_pc as i32);
        let bytes = b.to_bytes();
        assert_eq!(&bytes[jump_pc + 1..jump_pc + 5], &offset.to_le_bytes());
    }

    #[test]
    fn patch_jmp_l_backward() {
        let mut b = Builder::new();
        b.emit(OpCode::PUSH1);
        let index = b.emit_jmp_l_placeholder();
        b.emit(OpCode::DROP);
        let target = 0_usize;
        b.patch_jmp_target_at_instruction(index, target);
        let jump_pc = b.bytecode_offset_of(index);
        let relative = (target as i32) - (jump_pc as i32);
        assert!(relative < 0);
        let bytes = b.to_bytes();
        assert_eq!(&bytes[jump_pc + 1..jump_pc + 5], &relative.to_le_bytes());
    }

    #[test]
    fn patch_jmpif_l_and_jmpifnot_l() {
        let mut b_if = Builder::new();
        let i1 = b_if.emit_jmpif_l_placeholder();
        b_if.emit(OpCode::NOP);
        b_if.patch_jmp_target_at_instruction(i1, b_if.cursor());
        assert_eq!(b_if.instructions()[i1].opcode, OpCode::JMPIF_L);

        let mut b_not = Builder::new();
        let i2 = b_not.emit_jmpifnot_l_placeholder();
        b_not.emit(OpCode::NOP);
        b_not.patch_jmp_target_at_instruction(i2, b_not.cursor());
        assert_eq!(b_not.instructions()[i2].opcode, OpCode::JMPIFNOT_L);
    }

    #[test]
    fn two_forward_jmps_independent_patches() {
        let mut b = Builder::new();
        let j0 = b.emit_jmp_l_placeholder();
        let j1 = b.emit_jmp_l_placeholder();
        b.emit(OpCode::PUSH0);
        let mid = b.cursor();
        b.emit(OpCode::PUSH1);
        let end = b.cursor();

        b.patch_jmp_target_at_instruction(j1, end);
        b.patch_jmp_target_at_instruction(j0, mid);

        let bytes = b.to_bytes();
        let pc0 = b.bytecode_offset_of(j0);
        let pc1 = b.bytecode_offset_of(j1);
        let rel0 = (mid as i32) - (pc0 as i32);
        let rel1 = (end as i32) - (pc1 as i32);
        assert_eq!(&bytes[pc0 + 1..pc0 + 5], &rel0.to_le_bytes());
        assert_eq!(&bytes[pc1 + 1..pc1 + 5], &rel1.to_le_bytes());
    }

    #[test]
    #[should_panic(expected = "4-byte placeholder")]
    fn patch_non_placeholder_panics() {
        let mut b = Builder::new();
        b.emit(OpCode::PUSH0);
        b.patch_jmp_target_at_instruction(0, 0);
    }
}
