use crate::codegen::CodegenError;
use crate::syntax::ast::{Literal, Type};
use crate::target::opcode::OpCode;

use super::ExprGen;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParsedIntLiteral {
    I128(i128),
    I256([u8; 32]),
}

#[cfg(test)]
impl ParsedIntLiteral {
    pub(crate) fn as_i128(self) -> Option<i128> {
        match self {
            Self::I128(n) => Some(n),
            Self::I256(_) => None,
        }
    }
}

pub(crate) fn parse_int_literal(raw: &str) -> Option<ParsedIntLiteral> {
    let raw: String = raw.chars().filter(|&c| c != '_').collect();
    if raw.is_empty() {
        return None;
    }

    let (negative, digits) = raw
        .strip_prefix('-')
        .map(|digits| (true, digits))
        .unwrap_or((false, raw.as_str()));
    if digits.is_empty() {
        return None;
    }

    let (base, digits) = if let Some(digits) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16u8, digits)
    } else if let Some(digits) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        (2u8, digits)
    } else {
        (10u8, digits)
    };
    if digits.is_empty() {
        return None;
    }

    let mut magnitude = [0u8; 32];
    for byte in digits.bytes() {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => 10 + byte - b'a',
            b'A'..=b'F' => 10 + byte - b'A',
            _ => return None,
        };
        if digit >= base {
            return None;
        }
        mul_add_u8_le(&mut magnitude, base, digit)?;
    }

    if negative {
        if !fits_negative_i256_magnitude(&magnitude) {
            return None;
        }
        twos_complement_negate(&mut magnitude);
    } else if !fits_positive_i256_magnitude(&magnitude) {
        return None;
    }

    if let Some(n) = signed_le_bytes_to_i128_if_fits(&magnitude) {
        Some(ParsedIntLiteral::I128(n))
    } else {
        Some(ParsedIntLiteral::I256(magnitude))
    }
}

fn mul_add_u8_le(bytes: &mut [u8; 32], mul: u8, add: u8) -> Option<()> {
    let mut carry = add as u16;
    for byte in bytes.iter_mut() {
        let next = (*byte as u16) * (mul as u16) + carry;
        *byte = next as u8;
        carry = next >> 8;
    }
    (carry == 0).then_some(())
}

fn fits_positive_i256_magnitude(bytes: &[u8; 32]) -> bool {
    bytes[31] & 0x80 == 0
}

fn fits_negative_i256_magnitude(bytes: &[u8; 32]) -> bool {
    if bytes[31] < 0x80 {
        return true;
    }
    bytes[31] == 0x80 && bytes[..31].iter().all(|&b| b == 0)
}

fn twos_complement_negate(bytes: &mut [u8; 32]) {
    for byte in bytes.iter_mut() {
        *byte = !*byte;
    }
    let mut carry = 1u16;
    for byte in bytes.iter_mut() {
        let next = *byte as u16 + carry;
        *byte = next as u8;
        carry = next >> 8;
        if carry == 0 {
            break;
        }
    }
}

fn signed_le_bytes_to_i128_if_fits(bytes: &[u8; 32]) -> Option<i128> {
    let negative = bytes[31] & 0x80 != 0;
    let sign_extension = if negative { 0xFF } else { 0x00 };
    if bytes[16..].iter().any(|&b| b != sign_extension) {
        return None;
    }
    let mut narrow = [0u8; 16];
    narrow.copy_from_slice(&bytes[..16]);
    let value = i128::from_le_bytes(narrow);
    if (value < 0) == negative {
        Some(value)
    } else {
        None
    }
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
