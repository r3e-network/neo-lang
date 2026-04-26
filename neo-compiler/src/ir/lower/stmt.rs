use std::collections::HashSet;

use crate::ir::*;
use crate::syntax::ast::*;

use lower::builder::Builder;
use lower::env::Env;
use lower::helpers::*;

impl<'a> Builder<'a> {
    pub(crate) fn lower_stmt(
        &mut self,
        stmt: &Stmt,
        env: &mut Env,
        return_ty: &Type,
    ) -> Result<(), LowerError> {
        match stmt {
            Stmt::Var { name, init } => {
                let value = if let Some(expr) = init {
                    self.lower_expr(expr, env)?
                } else {
                    let out = self.new_value();
                    self.emit(out, Instr::Const(Literal::Null));
                    ValueRef::Value(out)
                };
                env.set(name, value);
                if let Some(Expr::StructLit {
                    name: struct_name, ..
                }) = init.as_ref()
                {
                    env.set_struct_var(name, struct_name);
                }
                Ok(())
            }
            Stmt::Expr(expr) => {
                let _ = self.lower_expr(expr, env)?;
                Ok(())
            }
            Stmt::Return(opt) => {
                let return_value = match opt {
                    Some(expr) => Some(self.lower_expr(expr, env)?),
                    None => None,
                };
                let bb = self.current_block;
                self.set_term(bb, Terminator::Return(return_value));
                let _ = return_ty;
                Ok(())
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => self.lower_if(cond, then_block, else_block.as_ref(), env, return_ty),
            Stmt::While { cond, body } => self.lower_while(cond, body, env, return_ty),
            Stmt::ForArray { item, iter, body } => {
                self.lower_for_array(item, iter, body, env, return_ty)
            }
            Stmt::ForMap {
                key,
                value,
                map,
                body,
            } => self.lower_for_map(key, value, map, body, env, return_ty),
            Stmt::Block(block) => self.lower_block(block, env, return_ty),
            Stmt::Emit { name, args } => {
                let mut values = Vec::new();
                for arg in args {
                    values.push(self.lower_expr(arg, env)?);
                }
                let out = self.new_value();
                self.emit(
                    out,
                    Instr::Emit {
                        name: name.clone(),
                        args: values,
                    },
                );
                Ok(())
            }
        }
    }

    fn lower_for_array(
        &mut self,
        item: &str,
        iter: &Expr,
        body: &Block,
        env: &mut Env,
        return_ty: &Type,
    ) -> Result<(), LowerError> {
        let preheader = self.current_block;

        let array_ref = self.lower_expr(iter, env)?;
        let array_variable_name = self.fresh_tmp("for_array");
        let array_value_out = self.new_value();
        self.emit(array_value_out, Instr::Copy(array_ref));
        env.set(&array_variable_name, ValueRef::Value(array_value_out));

        let index_variable_name = self.fresh_tmp("for_index");
        let initial_index_out = self.new_value();
        self.emit(initial_index_out, Instr::Const(Literal::Int("0".into())));
        env.set(&index_variable_name, ValueRef::Value(initial_index_out));

        let saved_item = env.get(item);
        let saved_item_declared = env.declared.contains(item);

        let loop_header = self.new_block();
        let body_bb = self.new_block();
        let exit_bb = self.new_block();

        let mut assigned: HashSet<String> = HashSet::new();
        collect_assigned_vars_in_block(body, &mut assigned);
        assigned.remove(item);

        let mut loop_vars: Vec<String> = Vec::new();
        for var in assigned {
            if env.declared.contains(&var) {
                loop_vars.push(var);
            }
        }
        loop_vars.push(index_variable_name.clone());
        loop_vars.sort();

        {
            let hdr = self.blocks.get_mut(&loop_header).unwrap();
            for var in &loop_vars {
                hdr.params.push(BlockParam {
                    name: var.clone(),
                    ty: PrimTy::Any,
                });
            }
        }

        let mut init_args = Vec::new();
        for v in &loop_vars {
            let vv = env
                .get(v)
                .ok_or_else(|| err(format!("undefined loop var `{v}`")))?;
            init_args.push(vv);
        }
        self.set_term(
            preheader,
            Terminator::Jump {
                target: loop_header,
                args: init_args,
            },
        );

        let env_in = env.clone();
        let mut env_hdr = env_in.clone();
        for (index, var) in loop_vars.iter().enumerate() {
            env_hdr.set(var, ValueRef::Param(ParamId(index)));
        }

        self.current_block = loop_header;
        let index_value = env_hdr
            .get(&index_variable_name)
            .ok_or_else(|| err("missing for-loop index"))?;
        let array_value = env_hdr
            .get(&array_variable_name)
            .ok_or_else(|| err("missing for-loop array"))?;
        let size_out = self.new_value();
        self.emit(size_out, Instr::Size { value: array_value });
        let less_than_out = self.new_value();
        self.emit(
            less_than_out,
            Instr::Binary {
                op: BinaryOp::Lt,
                left: index_value,
                right: ValueRef::Value(size_out),
            },
        );
        self.set_term(
            loop_header,
            Terminator::Branch {
                cond: ValueRef::Value(less_than_out),
                then_bb: body_bb,
                then_args: Vec::new(),
                else_bb: exit_bb,
                else_args: Vec::new(),
            },
        );

        self.current_block = body_bb;
        let mut env_body = env_hdr.clone();
        let item_val = self.new_value();
        self.emit(
            item_val,
            Instr::IndexGet {
                base: array_value,
                index: index_value,
            },
        );
        let item_copy = self.new_value();
        self.emit(item_copy, Instr::Copy(ValueRef::Value(item_val)));
        env_body.set(item, ValueRef::Value(item_copy));

        self.lower_block(body, &mut env_body, return_ty)?;

        if !matches!(self.blocks[&body_bb].term, Terminator::Return(_)) {
            let one = self.new_value();
            self.emit(one, Instr::Const(Literal::Int("1".into())));
            let next_index = self.new_value();
            self.emit(
                next_index,
                Instr::Binary {
                    op: BinaryOp::Add,
                    left: index_value,
                    right: ValueRef::Value(one),
                },
            );
            let next_index_copy = self.new_value();
            self.emit(next_index_copy, Instr::Copy(ValueRef::Value(next_index)));
            env_body.set(&index_variable_name, ValueRef::Value(next_index_copy));

            let mut back_args = Vec::new();
            for var in &loop_vars {
                let vv = env_body.get(var).or_else(|| env_hdr.get(var)).unwrap();
                back_args.push(vv);
            }
            self.set_term(
                body_bb,
                Terminator::Jump {
                    target: loop_header,
                    args: back_args,
                },
            );
        }

        *env = env_hdr;
        self.current_block = exit_bb;

        if let Some(v) = saved_item {
            env.set(item, v);
        } else if !saved_item_declared {
            env.locals.remove(item);
            env.declared.remove(item);
        }
        Ok(())
    }

    fn lower_for_map(
        &mut self,
        key: &str,
        value: &str,
        map: &Expr,
        body: &Block,
        env: &mut Env,
        return_ty: &Type,
    ) -> Result<(), LowerError> {
        let preheader = self.current_block;

        let map_ref = self.lower_expr(map, env)?;
        let map_variable_name = self.fresh_tmp("for_map");
        let map_value_out = self.new_value();
        self.emit(map_value_out, Instr::Copy(map_ref));
        env.set(&map_variable_name, ValueRef::Value(map_value_out));

        let keys_array_variable_name = self.fresh_tmp("for_keys");
        let keys_out = self.new_value();
        self.emit(
            keys_out,
            Instr::Keys {
                map: ValueRef::Value(map_value_out),
            },
        );
        let keys_value_out = self.new_value();
        self.emit(keys_value_out, Instr::Copy(ValueRef::Value(keys_out)));
        env.set(&keys_array_variable_name, ValueRef::Value(keys_value_out));

        let index_variable_name = self.fresh_tmp("for_index");
        let initial_index_out = self.new_value();
        self.emit(initial_index_out, Instr::Const(Literal::Int("0".into())));
        env.set(&index_variable_name, ValueRef::Value(initial_index_out));

        let saved_key = env.get(key);
        let saved_key_declared = env.declared.contains(key);
        let saved_val = env.get(value);
        let saved_val_declared = env.declared.contains(value);

        let loop_header = self.new_block();
        let body_bb = self.new_block();
        let exit_bb = self.new_block();

        let mut assigned: HashSet<String> = HashSet::new();
        collect_assigned_vars_in_block(body, &mut assigned);
        assigned.remove(key);
        assigned.remove(value);

        let mut loop_vars: Vec<String> = Vec::new();
        for v in assigned {
            if env.declared.contains(&v) {
                loop_vars.push(v);
            }
        }
        loop_vars.push(index_variable_name.clone());
        loop_vars.sort();

        {
            let hdr = self.blocks.get_mut(&loop_header).unwrap();
            for v in &loop_vars {
                hdr.params.push(BlockParam {
                    name: v.clone(),
                    ty: PrimTy::Any,
                });
            }
        }

        let mut init_args = Vec::new();
        for v in &loop_vars {
            let vv = env
                .get(v)
                .ok_or_else(|| err(format!("undefined loop var `{v}`")))?;
            init_args.push(vv);
        }
        self.set_term(
            preheader,
            Terminator::Jump {
                target: loop_header,
                args: init_args,
            },
        );

        let env_in = env.clone();
        let mut env_hdr = env_in.clone();
        for (index, var) in loop_vars.iter().enumerate() {
            env_hdr.set(var, ValueRef::Param(ParamId(index)));
        }

        self.current_block = loop_header;
        let index_value = env_hdr
            .get(&index_variable_name)
            .ok_or_else(|| err("missing for-loop index"))?;
        let keys_array_value = env_hdr
            .get(&keys_array_variable_name)
            .ok_or_else(|| err("missing for-loop keys"))?;
        let size_out = self.new_value();
        self.emit(
            size_out,
            Instr::Size {
                value: keys_array_value,
            },
        );
        let less_than_out = self.new_value();
        self.emit(
            less_than_out,
            Instr::Binary {
                op: BinaryOp::Lt,
                left: index_value,
                right: ValueRef::Value(size_out),
            },
        );
        self.set_term(
            loop_header,
            Terminator::Branch {
                cond: ValueRef::Value(less_than_out),
                then_bb: body_bb,
                then_args: Vec::new(),
                else_bb: exit_bb,
                else_args: Vec::new(),
            },
        );

        self.current_block = body_bb;
        let mut env_body = env_hdr.clone();

        let key_out = self.new_value();
        self.emit(
            key_out,
            Instr::IndexGet {
                base: keys_array_value,
                index: index_value,
            },
        );
        let key_copy = self.new_value();
        self.emit(key_copy, Instr::Copy(ValueRef::Value(key_out)));
        env_body.set(key, ValueRef::Value(key_copy));

        let map_value = env_hdr
            .get(&map_variable_name)
            .ok_or_else(|| err("missing for-loop map"))?;
        let val_out = self.new_value();
        self.emit(
            val_out,
            Instr::IndexGet {
                base: map_value,
                index: ValueRef::Value(key_copy),
            },
        );
        let val_copy = self.new_value();
        self.emit(val_copy, Instr::Copy(ValueRef::Value(val_out)));
        env_body.set(value, ValueRef::Value(val_copy));

        self.lower_block(body, &mut env_body, return_ty)?;

        if !matches!(self.blocks[&body_bb].term, Terminator::Return(_)) {
            let one = self.new_value();
            self.emit(one, Instr::Const(Literal::Int("1".into())));
            let next_index = self.new_value();
            self.emit(
                next_index,
                Instr::Binary {
                    op: BinaryOp::Add,
                    left: index_value,
                    right: ValueRef::Value(one),
                },
            );
            let next_index_copy = self.new_value();
            self.emit(next_index_copy, Instr::Copy(ValueRef::Value(next_index)));
            env_body.set(&index_variable_name, ValueRef::Value(next_index_copy));

            let mut back_args = Vec::new();
            for v in &loop_vars {
                let vv = env_body.get(v).or_else(|| env_hdr.get(v)).unwrap();
                back_args.push(vv);
            }
            self.set_term(
                body_bb,
                Terminator::Jump {
                    target: loop_header,
                    args: back_args,
                },
            );
        }

        *env = env_hdr;
        self.current_block = exit_bb;

        if let Some(v) = saved_key {
            env.set(key, v);
        } else if !saved_key_declared {
            env.locals.remove(key);
            env.declared.remove(key);
        }
        if let Some(v) = saved_val {
            env.set(value, v);
        } else if !saved_val_declared {
            env.locals.remove(value);
            env.declared.remove(value);
        }
        Ok(())
    }
}
