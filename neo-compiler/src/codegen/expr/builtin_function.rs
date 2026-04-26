use crate::codegen::CodegenError;
use crate::syntax::ast::Expr;
use crate::target::opcode::OpCode;

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_builtin_call(&mut self, name: &str, args: &[Expr]) -> Result<bool, CodegenError> {
        let err_call =
            |msg: &str| -> CodegenError { CodegenError::Unsupported(format!("`{name}` {msg}")) };
        match name {
            "assert" => {
                if args.len() != 2 {
                    return Err(err_call("expects 2 arguments"));
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.builder.emit(OpCode::ASSERTMSG);
                Ok(true)
            }
            "abort" => {
                if args.len() != 1 {
                    return Err(err_call("expects 1 argument"));
                }
                self.compile_expr(&args[0])?;
                self.builder.emit(OpCode::ABORTMSG);
                Ok(true)
            }
            "min" => {
                if args.len() != 2 {
                    return Err(err_call("expects 2 arguments"));
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.builder.emit(OpCode::MIN);
                Ok(true)
            }
            "max" => {
                if args.len() != 2 {
                    return Err(err_call("expects 2 arguments"));
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.builder.emit(OpCode::MAX);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

