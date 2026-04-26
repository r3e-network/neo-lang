use crate::ir::*;
use crate::syntax::ast::{BinaryOp, Literal};

use super::builder::Builder;
use super::env::Env;
use super::helpers::LowerError;

impl<'a> Builder<'a> {
    pub(crate) fn lower_short_circuit(
        &mut self,
        op: BinaryOp,
        left: &crate::syntax::ast::Expr,
        right: &crate::syntax::ast::Expr,
        env: &mut Env,
    ) -> Result<ValueRef, LowerError> {
        let header_bb = self.current_block;
        let left_value = self.lower_expr(left, env)?;
        let env_after_left = env.clone();
        let eval_right_bb = self.new_block();
        let const_bb = self.new_block();
        let join_bb = self.new_block();
        let (then_bb, else_bb) = match op {
            BinaryOp::And => (eval_right_bb, const_bb),
            BinaryOp::Or => (const_bb, eval_right_bb),
            _ => unreachable!(),
        };
        self.set_term(
            header_bb,
            Terminator::Branch {
                cond: left_value,
                then_bb,
                then_args: Vec::new(),
                else_bb,
                else_args: Vec::new(),
            },
        );

        self.current_block = const_bb;
        let const_value_out = self.new_value();
        let lit = match op {
            BinaryOp::And => Literal::Bool(false),
            BinaryOp::Or => Literal::Bool(true),
            _ => unreachable!(),
        };
        self.emit(const_value_out, Instr::Const(lit));

        self.current_block = eval_right_bb;
        *env = env_after_left.clone();
        let right_value = self.lower_expr(right, env)?;
        let env_after_right = env.clone();

        let mut join_param_names: Vec<String> = Vec::new();
        let mut const_args: Vec<ValueRef> = vec![ValueRef::Value(const_value_out)];
        let mut right_args: Vec<ValueRef> = vec![right_value];

        let mut declared_names: Vec<String> = env_after_left.declared.iter().cloned().collect();
        declared_names.sort();

        for name in declared_names {
            let const_value = env_after_left.get(&name);
            let right_value = env_after_right.get(&name);
            let (Some(const_value), Some(right_value)) = (const_value, right_value) else {
                continue;
            };
            if const_value != right_value {
                join_param_names.push(name.clone());
                const_args.push(const_value);
                right_args.push(right_value);
            }
        }

        {
            let join = self.blocks.get_mut(&join_bb).unwrap();
            join.params.push(BlockParam {
                name: "_sc".into(),
                ty: PrimTy::Bool,
            });
            for name in &join_param_names {
                join.params.push(BlockParam {
                    name: name.clone(),
                    ty: PrimTy::Any,
                });
            }
        }

        self.set_term(
            const_bb,
            Terminator::Jump {
                target: join_bb,
                args: const_args,
            },
        );
        self.set_term(
            eval_right_bb,
            Terminator::Jump {
                target: join_bb,
                args: right_args,
            },
        );

        let mut new_env = env_after_left.clone();
        for (index, name) in join_param_names.into_iter().enumerate() {
            new_env.set(&name, ValueRef::Param(ParamId(index + 1)));
        }
        *env = new_env;
        self.current_block = join_bb;
        Ok(ValueRef::Param(ParamId(0)))
    }
}
