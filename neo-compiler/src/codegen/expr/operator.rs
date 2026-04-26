use crate::codegen::CodegenError;
use crate::syntax::ast::{BinaryOp, Expr, UnaryOp};
use crate::target::opcode::{OpCode, ToOpCode};

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_binary(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<(), CodegenError> {
        match op {
            BinaryOp::And => {
                self.compile_expr(left)?;
                let jump_short = self.builder.emit_jmpifnot_l_placeholder();
                self.compile_expr(right)?;
                let jump_end = self.builder.emit_jmp_l_placeholder();
                let false_label = self.builder.cursor();
                self.builder
                    .patch_jmp_target_at_instruction(jump_short, false_label);
                self.builder.push_bool(false);
                let end = self.builder.cursor();
                self.builder.patch_jmp_target_at_instruction(jump_end, end);
                Ok(())
            }
            BinaryOp::Or => {
                self.compile_expr(left)?;
                self.builder.emit(OpCode::DUP);
                let jump_done = self.builder.emit_jmpif_l_placeholder();
                self.builder.emit(OpCode::DROP);
                self.compile_expr(right)?;
                let end = self.builder.cursor();
                self.builder.patch_jmp_target_at_instruction(jump_done, end);
                Ok(())
            }
            _ => {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.builder.emit(op.to_op_code());
                Ok(())
            }
        }
    }

    pub(super) fn compile_unary(&mut self, op: UnaryOp, expr: &Expr) -> Result<(), CodegenError> {
        match op {
            UnaryOp::Positive => self.compile_expr(expr),
            UnaryOp::Negative => {
                self.compile_expr(expr)?;
                self.builder.emit(OpCode::NEGATE);
                Ok(())
            }
            UnaryOp::Not => {
                self.compile_expr(expr)?;
                self.builder.emit(OpCode::NOT);
                Ok(())
            }
            UnaryOp::BitNot => {
                self.compile_expr(expr)?;
                self.builder.emit(OpCode::INVERT);
                Ok(())
            }
        }
    }
}

