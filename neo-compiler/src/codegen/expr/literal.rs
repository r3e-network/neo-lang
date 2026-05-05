use crate::codegen::CodegenError;
use crate::syntax::ast::{Literal, Type};
use crate::target::opcode::OpCode;

use super::ExprGen;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParsedIntLiteral {
    I128(i128),
    I256([u8; 32]),
}

impl ParsedIntLiteral {
    pub(crate) fn is_i32_sized(self) -> bool {
        match self {
            Self::I128(n) => (i32::MIN as i128..=i32::MAX as i128).contains(&n),
            Self::I256(_) => false,
        }
    }
}

pub(crate) fn parse_int_literal(raw: &str) -> Option<ParsedIntLiteral> {
    let raw: String = raw.chars().filter(|&c| c != '_').collect();
    let (negative, digits) = match raw.as_bytes().first()? {
        b'-' => (true, &raw[1..]),
        b'+' => (false, &raw[1..]),
        _ => (false, raw.as_str()),
    };

    let (radix, digits) =
        if digits.len() > 2 && (digits.starts_with("0x") || digits.starts_with("0X")) {
            (16, &digits[2..])
        } else if digits.len() > 2 && (digits.starts_with("0b") || digits.starts_with("0B")) {
            (2, &digits[2..])
        } else {
            (10, digits)
        };
    if digits.is_empty() {
        return None;
    }

    let mut bytes = parse_u256_magnitude(digits, radix)?;
    if negative {
        if is_zero(&bytes) {
            return Some(ParsedIntLiteral::I128(0));
        }
        if bytes[31] > 0x80 || (bytes[31] == 0x80 && bytes[..31].iter().any(|b| *b != 0)) {
            return None;
        }
        twos_complement_negate(&mut bytes);
    } else if bytes[31] > 0x7f {
        return None;
    }

    Some(classify_signed_int(bytes))
}

fn parse_u256_magnitude(digits: &str, radix: u32) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    for ch in digits.chars() {
        let digit = ch.to_digit(radix)?;
        if !mul_add_u256_le(&mut out, radix, digit) {
            return None;
        }
    }
    Some(out)
}

fn mul_add_u256_le(bytes: &mut [u8; 32], multiplier: u32, addend: u32) -> bool {
    let mut carry = addend;
    for byte in bytes.iter_mut() {
        let value = u32::from(*byte) * multiplier + carry;
        *byte = value as u8;
        carry = value >> 8;
    }
    carry == 0
}

fn twos_complement_negate(bytes: &mut [u8; 32]) {
    for byte in bytes.iter_mut() {
        *byte = !*byte;
    }
    let mut carry = 1u16;
    for byte in bytes.iter_mut() {
        let value = u16::from(*byte) + carry;
        *byte = value as u8;
        carry = value >> 8;
        if carry == 0 {
            break;
        }
    }
}

fn classify_signed_int(bytes: [u8; 32]) -> ParsedIntLiteral {
    let mut lower = [0u8; 16];
    lower.copy_from_slice(&bytes[..16]);
    let upper = &bytes[16..];
    let fits_i128 = (upper.iter().all(|byte| *byte == 0) && bytes[15] <= 0x7f)
        || (upper.iter().all(|byte| *byte == 0xff) && bytes[15] >= 0x80);
    if fits_i128 {
        ParsedIntLiteral::I128(i128::from_le_bytes(lower))
    } else {
        ParsedIntLiteral::I256(bytes)
    }
}

fn is_zero(bytes: &[u8; 32]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

impl ExprGen<'_, '_> {
    pub(super) fn emit_default_for_type(&mut self, ty: &Type) -> Result<(), CodegenError> {
        match ty {
            Type::Bool => self.builder.push_bool(false),
            Type::Int => self.builder.push_int(0),
            Type::String | Type::Hash160 | Type::Hash256 => self.builder.push_data(&[]),
            Type::Buffer => {
                self.builder.push_int(0);
                self.builder.emit(OpCode::NEWBUFFER);
            }
            Type::Array(_) | Type::Map { .. } => {
                self.builder.push_null();
            }
            Type::Any => self.builder.push_null(),
            Type::Void | Type::Named(_) => {
                return Err(CodegenError::Unsupported(format!(
                    "no default value for field type `{ty:?}` in struct literal"
                )));
            }
        }
        Ok(())
    }

    pub(super) fn emit_literal(&mut self, lit: &Literal) -> Result<(), CodegenError> {
        match lit {
            Literal::Null => {
                self.builder.push_null();
                Ok(())
            }
            Literal::Bool(b) => {
                self.builder.push_bool(*b);
                Ok(())
            }
            Literal::Int(s) => {
                let n = parse_int_literal(s)
                    .ok_or_else(|| CodegenError::BadIntegerLiteral(s.clone()))?;
                match n {
                    ParsedIntLiteral::I128(n) => {
                        if n < i64::MIN as i128 || n > i64::MAX as i128 {
                            self.builder.push_int128(n);
                        } else {
                            self.builder.push_int(n as i64);
                        }
                    }
                    ParsedIntLiteral::I256(bytes) => self.builder.push_int256(&bytes),
                }
                Ok(())
            }
            Literal::String(s) => {
                self.builder.push_data(s.as_bytes());
                Ok(())
            }
            Literal::Buffer(s) => {
                self.builder.push_data(s.as_bytes());
                Ok(())
            }
        }
    }
}
