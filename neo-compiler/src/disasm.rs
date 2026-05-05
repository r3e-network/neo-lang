//! Decode raw NeoVM script bytes into [`Instruction`](crate::target::Instruction) for listing / `disasm`.

use std::collections::BTreeSet;
use std::io::{self, Write};

use crate::asm_dump::{format_operands, AsmDump};
use crate::target::nef::Manifest;
use crate::target::opcode::OpCode;
use crate::target::Instruction;

/// Decode a full script into instructions (no trailing bytes allowed).
pub fn decode_script(bytes: &[u8]) -> Result<Vec<Instruction>, String> {
    let mut ip = 0usize;
    let mut out = Vec::new();
    while ip < bytes.len() {
        let (inst, size) = decode_one(bytes, ip)?;
        out.push(inst);
        ip += size;
    }
    Ok(out)
}

fn need(data: &[u8], start: usize, n: usize) -> Result<(), String> {
    if start + n <= data.len() {
        Ok(())
    } else {
        Err(format!(
            "disasm: unexpected EOF at {start}: need {n} byte(s), have {}",
            data.len().saturating_sub(start)
        ))
    }
}

fn decode_one(data: &[u8], ip: usize) -> Result<(Instruction, usize), String> {
    let b0 = *data
        .get(ip)
        .ok_or_else(|| "disasm: no available opcode at offset {ip:#x}".to_string())?;
    let op = OpCode::try_from(b0)
        .map_err(|e| format!("disasm: unknown opcode 0x{b0:02x} at offset {ip:#x}: {e}"))?;
    let ostart = ip + 1;
    let operand_len = match op {
        OpCode::PUSHINT8 => {
            need(data, ostart, 1)?;
            1
        }
        OpCode::PUSHINT16 => {
            need(data, ostart, 2)?;
            2
        }
        OpCode::PUSHINT32 => {
            need(data, ostart, 4)?;
            4
        }
        OpCode::PUSHINT64 => {
            need(data, ostart, 8)?;
            8
        }
        OpCode::PUSHINT128 => {
            need(data, ostart, 16)?;
            16
        }
        OpCode::PUSHINT256 => {
            need(data, ostart, 32)?;
            32
        }
        OpCode::PUSHA => {
            need(data, ostart, 4)?;
            4
        }
        OpCode::PUSHDATA1 => {
            need(data, ostart, 1)?;
            let n = data[ostart] as usize;
            need(data, ostart, 1 + n)?;
            1 + n
        }
        OpCode::PUSHDATA2 => {
            need(data, ostart, 2)?;
            let n = u16::from_le_bytes([data[ostart], data[ostart + 1]]) as usize;
            need(data, ostart, 2 + n)?;
            2 + n
        }
        OpCode::PUSHDATA4 => {
            need(data, ostart, 4)?;
            let n = u32::from_le_bytes([
                data[ostart],
                data[ostart + 1],
                data[ostart + 2],
                data[ostart + 3],
            ]) as usize;
            need(data, ostart, 4 + n)?;
            4 + n
        }
        OpCode::JMP
        | OpCode::JMPIF
        | OpCode::JMPIFNOT
        | OpCode::JMPEQ
        | OpCode::JMPNE
        | OpCode::JMPGT
        | OpCode::JMPGE
        | OpCode::JMPLT
        | OpCode::JMPLE
        | OpCode::CALL
        | OpCode::ENDTRY => {
            need(data, ostart, 1)?;
            1
        }
        OpCode::JMP_L
        | OpCode::JMPIF_L
        | OpCode::JMPIFNOT_L
        | OpCode::JMPEQ_L
        | OpCode::JMPNE_L
        | OpCode::JMPGT_L
        | OpCode::JMPGE_L
        | OpCode::JMPLT_L
        | OpCode::JMPLE_L
        | OpCode::CALL_L
        | OpCode::ENDTRY_L => {
            need(data, ostart, 4)?;
            4
        }
        OpCode::TRY => {
            need(data, ostart, 2)?;
            2
        }
        OpCode::TRY_L => {
            need(data, ostart, 8)?;
            8
        }
        OpCode::SYSCALL => {
            need(data, ostart, 4)?;
            4
        }
        OpCode::CALLT => {
            need(data, ostart, 2)?;
            2
        }
        OpCode::INITSSLOT => {
            need(data, ostart, 1)?;
            1
        }
        OpCode::INITSLOT => {
            need(data, ostart, 2)?;
            2
        }
        OpCode::LDSFLD
        | OpCode::STSFLD
        | OpCode::LDLOC
        | OpCode::STLOC
        | OpCode::LDARG
        | OpCode::STARG
        | OpCode::ISTYPE
        | OpCode::CONVERT
        | OpCode::NEWARRAY_T => {
            need(data, ostart, 1)?;
            1
        }
        _ => 0,
    };
    let operands = data[ostart..ostart + operand_len].to_vec();
    Ok((
        Instruction {
            opcode: op,
            operands,
        },
        1 + operand_len,
    ))
}

/// Parse a contract manifest JSON (same shape as `build` output).
pub fn parse_manifest_json(json: &str) -> Result<Manifest, String> {
    serde_json::from_str(json).map_err(|e| format!("disasm: parse manifest error: {e}"))
}

/// Build `(byte_offset, section_title)` markers from manifest ABI method offsets.
///
/// Offsets must lie on instruction boundaries in `instructions`. The compiler layout is
/// `package` routines, then `struct` methods, then `contract` methods; only contract methods
/// appear in the manifest, so bytes before the smallest manifest offset are labeled as
/// `package / struct routines`.
pub fn build_manifest_breaks(
    manifest: &Manifest,
    instructions: &[Instruction],
) -> Result<Vec<(usize, String)>, String> {
    let script_len: usize = instructions
        .iter()
        .map(|instruction| instruction.encoded_len())
        .sum();
    let mut starts = BTreeSet::new();
    let mut o = 0usize;
    for inst in instructions {
        starts.insert(o);
        o += inst.encoded_len();
    }
    if o != script_len {
        return Err("internal: instruction length sum mismatch".into());
    }

    let mut methods: Vec<(usize, &str)> = manifest
        .abi
        .methods
        .iter()
        .map(|m| (m.offset as usize, m.name.as_str()))
        .collect();
    methods.sort_by_key(|(off, _)| *off);

    let mut last_off = None;
    for (off, name) in &methods {
        if *off > script_len {
            return Err(format!(
                "manifest method `{name}` offset {off:#x} is past script end {script_len:#x}"
            ));
        }
        if !starts.contains(off) {
            return Err(format!(
                "manifest method `{name}` offset {off:#x} is not on an instruction boundary"
            ));
        }
        if let Some(p) = last_off {
            if *off == p {
                return Err(format!(
                    "manifest: duplicate method offset {off:#x} (`{name}`)"
                ));
            }
        }
        last_off = Some(*off);
    }

    let mut breaks: Vec<(usize, String)> = Vec::new();
    if let Some((first_off, _)) = methods.first() {
        if *first_off > 0 {
            breaks.push((
                0,
                "package / struct routines — not listed in manifest ABI".to_string(),
            ));
        }
    }
    for (off, name) in methods {
        breaks.push((off, format!("{}::{}", manifest.name, name)));
    }

    Ok(breaks)
}

/// Write a full listing (same line format as `asm`), optionally splitting on manifest method offsets.
pub fn write_disassembly_listing<W: Write>(
    w: &mut W,
    script_title: &str,
    instructions: &[Instruction],
    manifest: Option<&Manifest>,
) -> io::Result<()> {
    writeln!(w, "; {script_title}")?;
    match manifest {
        None => {
            AsmDump::new(w).dump_instructions(0, instructions)?;
        }
        Some(m) => {
            let breaks = build_manifest_breaks(m, instructions)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let mut offset = 0usize;
            let mut breaks_index = 0usize;
            for inst in instructions {
                while breaks_index < breaks.len() && breaks[breaks_index].0 == offset {
                    writeln!(w, "\n; {}", breaks[breaks_index].1)?;
                    breaks_index += 1;
                }
                let detail = format_operands(offset, inst);
                let op = format!("{:?}", inst.opcode);
                if detail.is_empty() {
                    writeln!(w, "{:04x}  {}", offset, op)?;
                } else {
                    writeln!(w, "{:04x}  {:<14} {}", offset, op, detail)?;
                }
                offset += inst.encoded_len();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::opcode::OpCode;

    #[test]
    fn decode_initslot_and_ret() {
        let bytes = [OpCode::INITSLOT as u8, 0, 2, OpCode::RET as u8];
        let v = decode_script(&bytes).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].opcode, OpCode::INITSLOT);
        assert_eq!(v[0].operands, vec![0u8, 2]);
        assert_eq!(v[1].opcode, OpCode::RET);
        assert!(v[1].operands.is_empty());
    }

    #[test]
    fn manifest_breaks_preamble_and_contract_method() {
        let json = r#"{
            "name": "C",
            "groups": [],
            "supportedstandards": [],
            "abi": {
                "methods": [
                    {"name": "f", "parameters": [], "returntype": "Void", "offset": 3, "safe": false}
                ],
                "events": []
            },
            "permissions": [{"contract": "*", "methods": "*"}],
            "trusts": "*",
            "extra": {}
        }"#;
        let manifest = parse_manifest_json(json).expect("disasm: parse manifest error");
        let instructions =
            decode_script(&[OpCode::INITSLOT as u8, 0, 2, OpCode::RET as u8]).unwrap();
        let breaks = build_manifest_breaks(&manifest, &instructions).unwrap();
        assert_eq!(breaks.len(), 2);
        assert_eq!(breaks[0].0, 0);
        assert!(breaks[0].1.contains("package"));
        assert_eq!(breaks[1], (3, "C::f".to_string()));
    }
}
