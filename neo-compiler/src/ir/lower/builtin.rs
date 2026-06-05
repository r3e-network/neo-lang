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
        let Some(builtin) = crate::target::builtin::BuiltinMethod::resolve(name) else {
            return Ok(None);
        };
        if args.len() != builtin.source_arg_count() {
            return Err(err(format!(
                "`{name}` expects {} argument(s), got {}",
                builtin.source_arg_count(),
                args.len()
            )));
        }
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.lower_expr(arg, env)?);
        }
        let out = self.new_value();
        self.emit(
            out,
            Instr::BuiltinCall {
                builtin,
                args: values,
            },
        );
        Ok(Some(ValueRef::Value(out)))
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
        let Some(binding) = crate::target::syscall::RuntimeMethod::resolve(method) else {
            return Err(err(format!("runtime.{method} is not a known method")));
        };
        if args.len() != binding.source_arg_count() {
            return Err(err(format!(
                "runtime.{method} expects {} argument(s), got {}",
                binding.source_arg_count(),
                args.len()
            )));
        }
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.lower_expr(arg, env)?);
        }
        let out = self.new_value();
        self.emit(
            out,
            Instr::RuntimeCall {
                method: binding,
                args: values,
            },
        );
        Ok(ValueRef::Value(out))
    }
}
