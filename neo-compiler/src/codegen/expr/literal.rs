use crate::codegen::CodegenError;
use crate::syntax::ast::{Literal, Type};
use crate::target::opcode::OpCode;

use super::ExprGen;

pub(crate) fn parse_int_literal(raw: &str) -> Option<i128> {
    let raw: String = raw.chars().filter(|&c| c != '_').collect();
    let value = if raw.len() > 2 && (raw.starts_with("0x") || raw.starts_with("0X")) {
        i128::from_str_radix(&raw[2..], 16).ok()?
    } else if raw.len() > 2 && (raw.starts_with("0b") || raw.starts_with("0B")) {
        i128::from_str_radix(&raw[2..], 2).ok()?
    } else {
        raw.parse::<i128>().ok()?
    };
    Some(value)
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
                if n < i64::MIN as i128 || n > i64::MAX as i128 {
                    self.builder.push_int128(n);
                } else {
                    self.builder.push_int(n as i64);
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
