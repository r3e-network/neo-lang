use crate::ir::*;
use crate::syntax::ast::*;

use lower::builder::Builder;
use lower::env::Env;
use lower::helpers::*;

impl<'a> Builder<'a> {
    pub(crate) fn lower_builtin_call(
        &mut self,
        name: &str,
        args: &[Expr],
        env: &mut Env,
    ) -> Result<Option<ValueRef>, LowerError> {
        match name {
            "abort" => {
                if args.len() != 1 {
                    return Err(err("`abort` expects 1 argument"));
                }
                let message = self.lower_expr(&args[0], env)?;
                let out = self.new_value();
                self.emit(out, Instr::Abort { message });
                Ok(Some(ValueRef::Value(out)))
            }
            "min" => {
                if args.len() != 2 {
                    return Err(err("`min` expects 2 arguments"));
                }
                let left = self.lower_expr(&args[0], env)?;
                let right = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(out, Instr::Min { left, right });
                Ok(Some(ValueRef::Value(out)))
            }
            "max" => {
                if args.len() != 2 {
                    return Err(err("`max` expects 2 arguments"));
                }
                let left = self.lower_expr(&args[0], env)?;
                let right = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(out, Instr::Max { left, right });
                Ok(Some(ValueRef::Value(out)))
            }
            "assert" => {
                if args.len() != 2 {
                    return Err(err("`assert` expects 2 arguments"));
                }
                let cond = self.lower_expr(&args[0], env)?;
                let message = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(out, Instr::Assert { cond, message });
                Ok(Some(ValueRef::Value(out)))
            }
            _ => Ok(None),
        }
    }

    /// `self.<map>.has` / `self.<map>.remove` lowered to storage IR; [`None`] if `base` is not a contract map field.
    fn try_lower_contract_map_storage_method(
        &mut self,
        base: &Expr,
        field: &str,
        args: &[Expr],
        env: &mut Env,
    ) -> Result<Option<ValueRef>, LowerError> {
        let Some((map_name, key_ty, _val_ty)) = self.contract_self_map_types(base) else {
            return Ok(None);
        };
        match field {
            "has" => {
                if args.len() != 1 {
                    return Err(err("`has` expects 1 argument"));
                }
                let key = self.lower_expr(&args[0], env)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::ContractMapStorageHas {
                        field: map_name,
                        key_ty,
                        key,
                    },
                );
                Ok(Some(ValueRef::Value(out)))
            }
            "remove" => {
                if args.len() != 1 {
                    return Err(err("`remove` expects 1 argument"));
                }
                let key = self.lower_expr(&args[0], env)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::ContractMapStorageRemove {
                        field: map_name,
                        key_ty,
                        key,
                    },
                );
                Ok(Some(ValueRef::Value(out)))
            }
            _ => Err(err(format!(
                "contract storage map does not support `{field}`; use `has`, `remove`, or `self.<map>[key]`"
            ))),
        }
    }

    pub(crate) fn lower_builtin_method_call(
        &mut self,
        base: &Expr,
        field: &str,
        args: &[Expr],
        env: &mut Env,
    ) -> Result<Option<ValueRef>, LowerError> {
        if let Some(v) = self.try_lower_contract_map_storage_method(base, field, args, env)? {
            return Ok(Some(v));
        }
        match field {
            "size" => {
                if !args.is_empty() {
                    return Err(err("`size` expects 0 arguments"));
                }
                let value = self.lower_expr(base, env)?;
                let out = self.new_value();
                self.emit(out, Instr::Size { value });
                return Ok(Some(ValueRef::Value(out)));
            }
            "sub" => {
                if args.len() != 2 {
                    return Err(err("`sub` expects 2 arguments"));
                }
                let value = self.lower_expr(base, env)?;
                let start = self.lower_expr(&args[0], env)?;
                let length = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::SubStr {
                        value,
                        start,
                        length,
                    },
                );
                return Ok(Some(ValueRef::Value(out)));
            }
            "sqrt" => {
                if !args.is_empty() {
                    return Err(err("`sqrt` expects 0 arguments"));
                }
                let value = self.lower_expr(base, env)?;
                let out = self.new_value();
                self.emit(out, Instr::Sqrt { value });
                return Ok(Some(ValueRef::Value(out)));
            }
            "modmul" => {
                if args.len() != 2 {
                    return Err(err("`modmul` expects 2 arguments"));
                }
                let value = self.lower_expr(base, env)?;
                let other = self.lower_expr(&args[0], env)?;
                let modulus = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::ModMul {
                        value,
                        other,
                        modulus,
                    },
                );
                return Ok(Some(ValueRef::Value(out)));
            }
            "modpow" => {
                if args.len() != 2 {
                    return Err(err("`modpow` expects 2 arguments"));
                }
                let value = self.lower_expr(base, env)?;
                let exponent = self.lower_expr(&args[0], env)?;
                let modulus = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::ModPow {
                        value,
                        exponent,
                        modulus,
                    },
                );
                return Ok(Some(ValueRef::Value(out)));
            }
            "within" => {
                if args.len() != 2 {
                    return Err(err("`within` expects 2 arguments"));
                }
                let value = self.lower_expr(base, env)?;
                let min_inclusive = self.lower_expr(&args[0], env)?;
                let max_exclusive = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::Within {
                        value,
                        min_inclusive,
                        max_exclusive,
                    },
                );
                return Ok(Some(ValueRef::Value(out)));
            }
            "push" => {
                if args.len() != 1 {
                    return Err(err("`push` expects 1 arguments"));
                }
                let array = self.lower_expr(base, env)?;
                let value = self.lower_expr(&args[0], env)?;
                let out = self.new_value();
                self.emit(out, Instr::ArrayAppend { array, value });
                return Ok(Some(ValueRef::Value(out)));
            }
            "pop" => {
                if !args.is_empty() {
                    return Err(err("`pop` expects 0 arguments"));
                }
                let array = self.lower_expr(base, env)?;
                let out = self.new_value();
                self.emit(out, Instr::ArrayPop { array });
                return Ok(Some(ValueRef::Value(out)));
            }
            "clear" => {
                if !args.is_empty() {
                    return Err(err("`clear` expects 0 arguments"));
                }
                let collection = self.lower_expr(base, env)?;
                let out = self.new_value();
                self.emit(out, Instr::ClearItems { collection });
                return Ok(Some(ValueRef::Value(out)));
            }
            "keys" => {
                if !args.is_empty() {
                    return Err(err("`keys` expects 0 arguments"));
                }
                let map = self.lower_expr(base, env)?;
                let out = self.new_value();
                self.emit(out, Instr::Keys { map });
                return Ok(Some(ValueRef::Value(out)));
            }
            "values" => {
                if !args.is_empty() {
                    return Err(err("`values` expects 0 arguments"));
                }
                let map = self.lower_expr(base, env)?;
                let out = self.new_value();
                self.emit(out, Instr::Values { map });
                return Ok(Some(ValueRef::Value(out)));
            }
            "has" => {
                if args.len() != 1 {
                    return Err(err("`has` expects 1 arguments"));
                }
                let map = self.lower_expr(base, env)?;
                let key = self.lower_expr(&args[0], env)?;
                let out = self.new_value();
                self.emit(out, Instr::HasKey { map, key });
                return Ok(Some(ValueRef::Value(out)));
            }
            "remove" => {
                if args.len() != 1 {
                    return Err(err("`remove` expects 1 arguments"));
                }
                let map = self.lower_expr(base, env)?;
                let key = self.lower_expr(&args[0], env)?;
                let out = self.new_value();
                self.emit(out, Instr::Remove { map, key });
                return Ok(Some(ValueRef::Value(out)));
            }
            _ => Ok(None),
        }
    }

    // the package must be "runtime"
    pub(crate) fn lower_runtime_call(
        &mut self,
        method: &str,
        args: &[Expr],
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        match method {
            "log" => {
                if args.len() != 1 {
                    return Err(err("`log` expects 1 arguments"));
                }
                let message = self.lower_expr(&args[0], env)?;
                let out = self.new_value();
                self.emit(out, Instr::RuntimeLog { message });
                return Ok(ValueRef::Value(out));
            }
            "notify" => {
                if args.len() != 2 {
                    return Err(err("`notify` expects 2 arguments"));
                }
                let event_name = self.lower_expr(&args[0], env)?;
                let state = self.lower_expr(&args[1], env)?;
                let out = self.new_value();
                self.emit(out, Instr::RuntimeNotify { event_name, state });
                return Ok(ValueRef::Value(out));
            }
            "contractCall" => {
                if args.len() != 3 {
                    return Err(err("`contractCall` expects 3 arguments"));
                }
                let contract = self.lower_expr(&args[0], env)?;
                let method = self.lower_expr(&args[1], env)?;
                let params = self.lower_expr(&args[2], env)?;
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::ContractCallReadOnly {
                        contract,
                        method,
                        params,
                    },
                );
                return Ok(ValueRef::Value(out));
            }
            _ => Err(err(format!("runtime.{method} is not a known method"))),
        }
    }
}
