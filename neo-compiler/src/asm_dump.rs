//! NeoVM instruction listing for CLI `asm` command.

use std::io;

use crate::codegen::CompiledSourceFile;
use crate::target::opcode::OpCode;
use crate::target::syscall::token_to_syscall;
use crate::target::{Instruction, StackItemType};

pub(crate) struct AsmDump<'a, W: io::Write> {
    w: &'a mut W,
}

impl<'a, W: io::Write> AsmDump<'a, W> {
    pub(crate) fn new(w: &'a mut W) -> Self {
        Self { w }
    }

    pub(crate) fn dump_compiled_source(&mut self, compiled: &CompiledSourceFile) -> io::Result<()> {
        let mut offset: usize = 0;
        for f in &compiled.package_functions {
            writeln!(self.w, "; {} — package", f.name)?;
            offset = self.dump_instructions(offset, &f.instructions)?;
            writeln!(self.w)?;
        }
        for f in &compiled.struct_methods {
            writeln!(self.w, "; {} — struct method", f.name)?;
            offset = self.dump_instructions(offset, &f.instructions)?;
            writeln!(self.w)?;
        }
        for f in &compiled.contract_methods {
            let c = f.contract.as_deref().unwrap_or("?");
            writeln!(self.w, "; {}::{}", c, f.name)?;
            offset = self.dump_instructions(offset, &f.instructions)?;
            writeln!(self.w)?;
        }
        Ok(())
    }

    pub(crate) fn dump_instructions(
        &mut self,
        mut offset: usize,
        instructions: &[Instruction],
    ) -> io::Result<usize> {
        for inst in instructions {
            let detail = format_operands(offset, inst);
            let op = format!("{:?}", inst.opcode);
            if detail.is_empty() {
                writeln!(self.w, "{:04x}  {}", offset, op)?;
            } else {
                writeln!(self.w, "{:04x}  {:<14} {}", offset, op, detail)?;
            }
            offset += inst.encoded_len();
        }
        Ok(offset)
    }
}

pub(crate) fn format_operands(offset: usize, inst: &Instruction) -> String {
    let op = inst.opcode;
    let o = inst.operands.as_slice();
    match op {
        OpCode::INITSLOT if o.len() == 2 => format!("locals={} args={}", o[0], o[1]),
        OpCode::INITSSLOT if o.len() == 1 => format!("count={}", o[0]),
        OpCode::LDARG
        | OpCode::STARG
        | OpCode::LDLOC
        | OpCode::STLOC
        | OpCode::LDSFLD
        | OpCode::STSFLD
            if o.len() == 1 =>
        {
            format!("#{}", o[0])
        }
        OpCode::ISTYPE | OpCode::CONVERT | OpCode::NEWARRAY_T if o.len() == 1 => {
            format!("type={}", format_stack_item_type(o[0]))
        }
        OpCode::CALLT if o.len() == 2 => u16::from_le_bytes([o[0], o[1]]).to_string(),
        OpCode::PUSHINT8 if o.len() == 1 => format!("{}", o[0] as i8),
        OpCode::PUSHINT16 if o.len() == 2 => format!("{}", i16::from_le_bytes([o[0], o[1]])),
        OpCode::PUSHINT32 if o.len() == 4 => {
            i32::from_le_bytes([o[0], o[1], o[2], o[3]]).to_string()
        }
        OpCode::PUSHINT64 if o.len() == 8 => {
            i64::from_le_bytes([o[0], o[1], o[2], o[3], o[4], o[5], o[6], o[7]]).to_string()
        }
        OpCode::PUSHINT128 if !o.is_empty() => format!("le128 [{}]", hex::encode(o)),
        OpCode::PUSHINT256 if !o.is_empty() => format!("le256 [{}]", hex::encode(o)),
        OpCode::SYSCALL if o.len() == 4 => {
            let t = u32::from_le_bytes([o[0], o[1], o[2], o[3]]);
            if let Some(syscall) = token_to_syscall(t) {
                format!("token={:#010x} {}", t, syscall.name)
            } else {
                format!("token={:#010x}", t)
            }
        }
        OpCode::PUSHDATA1 if o.len() >= 1 => {
            let n = o[0] as usize;
            let payload = o.get(1..).unwrap_or(&[]);
            if payload.len() == n {
                pushdata_summary(payload)
            } else {
                pushdata_summary(o)
            }
        }
        OpCode::PUSHDATA2 if o.len() >= 2 => {
            let n = u16::from_le_bytes([o[0], o[1]]) as usize;
            let payload = o.get(2..).unwrap_or(&[]);
            if payload.len() == n {
                pushdata_summary(payload)
            } else {
                pushdata_summary(o)
            }
        }
        OpCode::PUSHDATA4 if o.len() >= 4 => {
            let n = u32::from_le_bytes([o[0], o[1], o[2], o[3]]) as usize;
            let payload = o.get(4..).unwrap_or(&[]);
            if payload.len() == n {
                pushdata_summary(payload)
            } else {
                pushdata_summary(o)
            }
        }
        OpCode::TRY if o.len() == 2 => {
            let catch = o[0] as i8 as i32;
            let finally = o[1] as i8 as i32;
            format!("catch={:+05x} finally={:+05x}", catch, finally)
        }
        _ if op.is_change_pc_short() && o.len() == 1 => {
            let relative = o[0] as i8 as i32;
            let target = offset as i32 + relative;
            format!("relative={:+05x} target={:+05x}", relative, target)
        }
        _ if op.is_change_pc_long() && o.len() == 4 => {
            // NeoVM: signed offset in bytes from the **first byte of this instruction**
            // (the opcode) to the target instruction — not an absolute script index.
            let relative = i32::from_le_bytes([o[0], o[1], o[2], o[3]]);
            let target = offset as i32 + relative;
            format!("relative={:+05x} target={:+05x}", relative, target)
        }
        _ if o.is_empty() => String::new(),
        _ => format!("operands=0x{}", hex::encode(o)),
    }
}

fn is_visible_ascii_byte(b: u8) -> bool {
    (0x20..=0x7E).contains(&b)
}

/// Escape only `"` and `\` for a double-quoted ASCII run (bytes already verified visible).
fn escape_ascii_for_display(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'"' => s.push_str("\\\""),
            b'\\' => s.push_str("\\\\"),
            _ if is_visible_ascii_byte(b) => s.push(char::from(b)),
            _ => s.push_str(&format!("\\x{:02x}", b)),
        }
    }
    s
}

fn pushdata_summary(payload: &[u8]) -> String {
    const MAX: usize = 48;
    let shown = payload.len().min(MAX);
    let prefix = &payload[..shown];
    let truncated = payload.len() > shown;
    let text = escape_ascii_for_display(prefix);
    let tail = if truncated { " ..." } else { "" };
    format!("len={} \"{text}\"{tail}", payload.len())
}

fn format_stack_item_type(ty: u8) -> String {
    if ty == StackItemType::Any as u8 {
        "Any".into()
    } else if ty == StackItemType::Pointer as u8 {
        "Pointer".into()
    } else if ty == StackItemType::Boolean as u8 {
        "Boolean".into()
    } else if ty == StackItemType::Integer as u8 {
        "Integer".into()
    } else if ty == StackItemType::ByteString as u8 {
        "ByteString".into()
    } else if ty == StackItemType::Buffer as u8 {
        "Buffer".into()
    } else if ty == StackItemType::Array as u8 {
        "Array".into()
    } else if ty == StackItemType::Map as u8 {
        "Map".into()
    } else if ty == StackItemType::InteropInterface as u8 {
        "InteropInterface".into()
    } else {
        format!("UnknownStackItemType({})", ty)
    }
}
