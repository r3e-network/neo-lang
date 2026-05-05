use crate::codegen::CodegenError;
use crate::syntax::ast::Expr;
use crate::target::opcode::OpCode;

use super::ExprGen;

impl ExprGen<'_, '_> {
    pub(super) fn compile_builtin_method_call(
        &mut self,
        receiver: &Expr,
        method: &str,
        args: &[Expr],
    ) -> Result<bool, CodegenError> {
        let err_method =
            |msg: &str| -> CodegenError { CodegenError::Unsupported(format!("`{method}` {msg}")) };
        match method {
            "size" => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                self.compile_expr(receiver)?;
                self.builder.emit(OpCode::SIZE);
                Ok(true)
            }
            "sub" => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                self.compile_expr(receiver)?;
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.builder.emit(OpCode::SUBSTR);
                Ok(true)
            }
            "sqrt" => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                self.compile_expr(receiver)?;
                self.builder.emit(OpCode::SQRT);
                Ok(true)
            }
            "modmul" => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                self.compile_expr(receiver)?;
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.builder.emit(OpCode::MODMUL);
                Ok(true)
            }
            "modpow" => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                self.compile_expr(receiver)?;
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.builder.emit(OpCode::MODPOW);
                Ok(true)
            }
            "within" => {
                if args.len() != 2 {
                    return Err(err_method("expects 2 arguments"));
                }
                self.compile_expr(receiver)?;
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.builder.emit(OpCode::WITHIN);
                Ok(true)
            }
            "push" => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                self.compile_expr(receiver)?;
                self.compile_expr(&args[0])?;
                self.builder.emit(OpCode::APPEND);
                Ok(true)
            }
            "pop" => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                self.compile_expr(receiver)?;
                self.builder.emit(OpCode::POPITEM);
                Ok(true)
            }
            "clear" => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                self.compile_expr(receiver)?;
                self.builder.emit(OpCode::CLEARITEMS);
                Ok(true)
            }
            "keys" => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                self.compile_expr(receiver)?;
                self.builder.emit(OpCode::KEYS);
                Ok(true)
            }
            "values" => {
                if !args.is_empty() {
                    return Err(err_method("expects 0 arguments"));
                }
                self.compile_expr(receiver)?;
                self.builder.emit(OpCode::VALUES);
                Ok(true)
            }
            "has" => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                if let Some((map_name, key_ty, _)) = self.contract_storage_map_receiver(receiver) {
                    self.emit_contract_map_has(&map_name, &key_ty, &args[0])?;
                    return Ok(true);
                }
                self.compile_expr(receiver)?;
                self.compile_expr(&args[0])?;
                self.builder.emit(OpCode::HASKEY);
                Ok(true)
            }
            "remove" => {
                if args.len() != 1 {
                    return Err(err_method("expects 1 argument"));
                }
                if let Some((map_name, key_ty, _)) = self.contract_storage_map_receiver(receiver) {
                    self.emit_contract_map_delete(&map_name, &key_ty, &args[0])?;
                    return Ok(true);
                }
                self.compile_expr(receiver)?;
                self.compile_expr(&args[0])?;
                self.builder.emit(OpCode::REMOVE);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
