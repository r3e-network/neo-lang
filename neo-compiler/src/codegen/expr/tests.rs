use super::*;
use crate::target::method_token::MethodTokenRegistry;
use crate::target::syscall::Syscall;
use crate::target::Instruction;

fn compile_expr_stub<'a>(
    params: &[Param],
    structs: &'a [StructDecl],
    value_struct: &mut HashMap<String, String>,
    expr: &Expr,
) -> Result<Vec<Instruction>, CodegenError> {
    let mut env = VarEnv::new(params)?;
    let mut builder = Builder::new();
    let mut pending = Vec::new();
    let empty_fns = HashMap::new();
    ExprGen {
        builder: &mut builder,
        env: &mut env,
        structs,
        value_struct,
        contract_fields: &[],
        contract_name: None,
        contract_fns: None,
        pending_call_l: &mut pending,
        package_fns: &empty_fns,
        method_tokens: &mut MethodTokenRegistry::new(),
    }
    .compile_expr(expr)?;
    Ok(builder.into_instructions())
}

#[test]
fn convert_operand_maps_primitive_types() {
    assert_eq!(
        get_operand_for_type(&Type::Bool),
        Some(StackItemType::Boolean as u8)
    );
    assert_eq!(
        get_operand_for_type(&Type::Int),
        Some(StackItemType::Integer as u8)
    );
    assert_eq!(
        get_operand_for_type(&Type::String),
        Some(StackItemType::ByteString as u8)
    );
    assert_eq!(
        get_operand_for_type(&Type::Hash160),
        Some(StackItemType::ByteString as u8)
    );
    assert_eq!(
        get_operand_for_type(&Type::Buffer),
        Some(StackItemType::Buffer as u8)
    );
    assert_eq!(
        get_operand_for_type(&Type::Array(Box::new(Type::Int))),
        Some(StackItemType::Array as u8)
    );
    assert_eq!(
        get_operand_for_type(&Type::Map {
            key: Box::new(Type::Int),
            value: Box::new(Type::String),
        }),
        Some(StackItemType::Map as u8)
    );
}

#[test]
fn convert_operand_named_and_void_are_none() {
    assert_eq!(get_operand_for_type(&Type::Named("T".into())), None);
    assert_eq!(get_operand_for_type(&Type::Void), None);
    assert_eq!(get_operand_for_type(&Type::Any), None);
}

#[test]
fn parse_int_decimal_hex_binary() {
    assert_eq!(parse_int_literal("0"), Some(0));
    assert_eq!(parse_int_literal("-42"), Some(-42));
    assert_eq!(parse_int_literal("1_000"), Some(1000));
    assert_eq!(parse_int_literal("0xFF"), Some(255));
    assert_eq!(parse_int_literal("0X10"), Some(16));
    assert_eq!(parse_int_literal("0b1010"), Some(10));
    assert_eq!(parse_int_literal("0B11"), Some(3));
}

#[test]
fn parse_int_invalid() {
    assert_eq!(parse_int_literal(""), None);
    assert_eq!(parse_int_literal("0x"), None);
    assert_eq!(parse_int_literal("0b"), None);
    assert_eq!(parse_int_literal("not_a_number"), None);
}

#[test]
fn expr_literal_bool_null() {
    let mut value_struct = HashMap::new();
    let inst = compile_expr_stub(
        &[],
        &[],
        &mut value_struct,
        &Expr::Literal(Literal::Bool(true)),
    )
    .unwrap();
    assert_eq!(inst.len(), 1);
    assert_eq!(inst[0].opcode, OpCode::PUSHT);

    let inst =
        compile_expr_stub(&[], &[], &mut value_struct, &Expr::Literal(Literal::Null)).unwrap();
    assert_eq!(inst[0].opcode, OpCode::PUSHNULL);
}

#[test]
fn expr_literal_int_string() {
    let mut value_struct = HashMap::new();
    let inst = compile_expr_stub(
        &[],
        &[],
        &mut value_struct,
        &Expr::Literal(Literal::Int("7".into())),
    )
    .unwrap();
    assert_eq!(inst[0].opcode, OpCode::PUSH7);

    let inst = compile_expr_stub(
        &[],
        &[],
        &mut value_struct,
        &Expr::Literal(Literal::String("ab".into())),
    )
    .unwrap();
    assert_eq!(inst[0].opcode, OpCode::PUSHDATA1);
    assert_eq!(inst[0].operands[1..], *b"ab");
}

#[test]
fn expr_ident_loads_arg() {
    let mut value_struct = HashMap::new();
    let params = vec![Param {
        ty: Type::Int,
        name: "n".into(),
    }];
    let inst =
        compile_expr_stub(&params, &[], &mut value_struct, &Expr::Ident("n".into())).unwrap();
    assert_eq!(inst[0].opcode, OpCode::LDARG0);
}

#[test]
fn expr_map_remove_emits_remove_with_key_on_top() {
    let mut value_struct = HashMap::new();
    let params = vec![Param {
        ty: Type::Map {
            key: Box::new(Type::Int),
            value: Box::new(Type::Int),
        },
        name: "m".into(),
    }];
    let expr = Expr::Call {
        callee: Box::new(Expr::Member {
            base: Box::new(Expr::Ident("m".into())),
            field: "remove".into(),
        }),
        args: vec![Expr::Literal(Literal::Int("1".into()))],
    };
    let inst = compile_expr_stub(&params, &[], &mut value_struct, &expr).unwrap();
    assert_eq!(inst.len(), 3);
    assert_eq!(inst[0].opcode, OpCode::LDARG0);
    assert_eq!(inst[1].opcode, OpCode::PUSH1);
    assert_eq!(inst[2].opcode, OpCode::REMOVE);
}

#[test]
fn expr_binary_add_and_compare() {
    let mut value_struct = HashMap::new();
    let expr = Expr::Binary {
        op: BinaryOp::Add,
        left: Box::new(Expr::Literal(Literal::Int("1".into()))),
        right: Box::new(Expr::Literal(Literal::Int("2".into()))),
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    assert!(inst.iter().any(|i| i.opcode == OpCode::PUSH1));
    assert!(inst.iter().any(|i| i.opcode == OpCode::PUSH2));
    assert!(inst.iter().any(|i| i.opcode == OpCode::ADD));

    let expr = Expr::Binary {
        op: BinaryOp::Eq,
        left: Box::new(Expr::Literal(Literal::Bool(true))),
        right: Box::new(Expr::Literal(Literal::Bool(false))),
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    assert!(inst.iter().any(|i| i.opcode == OpCode::EQUAL));
}

#[test]
fn expr_ne_null_emits_isnull_not() {
    let mut value_struct = HashMap::new();
    let params = vec![Param {
        name: "dest".into(),
        ty: Type::Hash160,
    }];
    let expr = Expr::Binary {
        op: BinaryOp::Ne,
        left: Box::new(Expr::Ident("dest".into())),
        right: Box::new(Expr::Literal(Literal::Null)),
    };
    let inst = compile_expr_stub(&params, &[], &mut value_struct, &expr).unwrap();
    let opcodes: Vec<_> = inst.iter().map(|i| i.opcode).collect();
    assert_eq!(opcodes, vec![OpCode::LDARG0, OpCode::ISNULL, OpCode::NOT]);
}

#[test]
fn expr_unary_not() {
    let mut value_struct = HashMap::new();
    let expr = Expr::Unary {
        op: UnaryOp::Not,
        expr: Box::new(Expr::Literal(Literal::Bool(false))),
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    assert_eq!(inst.last().unwrap().opcode, OpCode::NOT);
}

#[test]
fn expr_cast_int_to_bool_emits_convert() {
    let mut value_struct = HashMap::new();
    let expr = Expr::Cast {
        expr: Box::new(Expr::Literal(Literal::Int("0".into()))),
        ty: Type::Bool,
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    let conv = inst.iter().find(|i| i.opcode == OpCode::CONVERT).unwrap();
    assert_eq!(conv.operands, vec![StackItemType::Boolean as u8]);
}

#[test]
fn expr_array_and_map_pack() {
    let mut value_struct = HashMap::new();
    let expr = Expr::ArrayLit {
        ty: Type::Array(Box::new(Type::Int)),
        elements: vec![
            Expr::Literal(Literal::Int("1".into())),
            Expr::Literal(Literal::Int("2".into())),
        ],
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    assert!(inst.iter().any(|i| i.opcode == OpCode::PACK));

    let expr = Expr::MapLit {
        ty: Type::Map {
            key: Box::new(Type::String),
            value: Box::new(Type::Int),
        },
        pairs: vec![(
            Expr::Literal(Literal::String("k".into())),
            Expr::Literal(Literal::Int("1".into())),
        )],
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    assert!(inst.iter().any(|i| i.opcode == OpCode::PACKMAP));
}

#[test]
fn expr_call_min_max_assert() {
    let mut value_struct = HashMap::new();
    let expr = Expr::Call {
        callee: Box::new(Expr::Ident("min".into())),
        args: vec![
            Expr::Literal(Literal::Int("3".into())),
            Expr::Literal(Literal::Int("5".into())),
        ],
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    assert_eq!(inst.last().unwrap().opcode, OpCode::MIN);

    let expr = Expr::Call {
        callee: Box::new(Expr::Ident("assert".into())),
        args: vec![
            Expr::Literal(Literal::Bool(true)),
            Expr::Literal(Literal::String("ok".into())),
        ],
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    assert_eq!(inst.last().unwrap().opcode, OpCode::ASSERTMSG);
}

#[test]
fn expr_runtime_log_syscall() {
    let mut value_struct = HashMap::new();
    let expr = Expr::Call {
        callee: Box::new(Expr::Member {
            base: Box::new(Expr::Ident("runtime".into())),
            field: "log".into(),
        }),
        args: vec![Expr::Literal(Literal::String("m".into()))],
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    let syscall = inst.iter().find(|i| i.opcode == OpCode::SYSCALL).unwrap();
    assert_eq!(
        syscall.operands,
        Syscall::RUNTIME_LOG.token().to_le_bytes().to_vec()
    );
}

#[test]
fn expr_runtime_get_network_syscall() {
    let mut value_struct = HashMap::new();
    let expr = Expr::Call {
        callee: Box::new(Expr::Member {
            base: Box::new(Expr::Ident("runtime".into())),
            field: "getNetwork".into(),
        }),
        args: vec![],
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    let syscall = inst.iter().find(|i| i.opcode == OpCode::SYSCALL).unwrap();
    assert_eq!(
        syscall.operands,
        Syscall::RUNTIME_GET_NETWORK.token().to_le_bytes().to_vec()
    );
}

#[test]
fn expr_member_pickitem_with_struct_meta() {
    let structs = vec![StructDecl {
        name: "Point".into(),
        fields: vec![
            StructField {
                ty: Type::Int,
                name: "x".into(),
                init: None,
            },
            StructField {
                ty: Type::Int,
                name: "y".into(),
                init: None,
            },
        ],
        methods: vec![],
    }];
    let mut value_struct = HashMap::new();
    value_struct.insert("p".into(), "Point".into());
    let params = vec![Param {
        ty: Type::Named("Point".into()),
        name: "p".into(),
    }];
    let expr = Expr::Member {
        base: Box::new(Expr::Ident("p".into())),
        field: "y".into(),
    };
    let inst = compile_expr_stub(&params, &structs, &mut value_struct, &expr).unwrap();
    assert_eq!(inst[0].opcode, OpCode::LDARG0);
    assert!(inst.iter().any(|i| i.opcode == OpCode::PICKITEM));
    let push_idx = inst.iter().position(|i| i.opcode == OpCode::PUSH1);
    assert!(push_idx.is_some(), "field y is index 1");
}

#[test]
fn expr_struct_literal_pack() {
    let structs = vec![StructDecl {
        name: "S".into(),
        fields: vec![StructField {
            ty: Type::Int,
            name: "a".into(),
            init: None,
        }],
        methods: vec![],
    }];
    let mut value_struct = HashMap::new();
    let expr = Expr::StructLit {
        name: "S".into(),
        fields: vec![("a".into(), Expr::Literal(Literal::Int("9".into())))],
    };
    let inst = compile_expr_stub(&[], &structs, &mut value_struct, &expr).unwrap();
    assert!(inst.iter().any(|i| i.opcode == OpCode::PACK));
}

#[test]
fn expr_paren_passthrough() {
    let mut value_struct = HashMap::new();
    let expr = Expr::Paren(Box::new(Expr::Literal(Literal::Bool(true))));
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    assert_eq!(inst[0].opcode, OpCode::PUSHT);
}

#[test]
fn expr_short_circuit_and_or_shape() {
    let mut value_struct = HashMap::new();
    let expr = Expr::Binary {
        op: BinaryOp::And,
        left: Box::new(Expr::Literal(Literal::Bool(false))),
        right: Box::new(Expr::Literal(Literal::Bool(true))),
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    assert!(inst.iter().any(|i| i.opcode == OpCode::JMPIFNOT_L));
    assert!(inst.iter().any(|i| i.opcode == OpCode::JMP_L));

    let expr = Expr::Binary {
        op: BinaryOp::Or,
        left: Box::new(Expr::Literal(Literal::Bool(true))),
        right: Box::new(Expr::Literal(Literal::Bool(false))),
    };
    let inst = compile_expr_stub(&[], &[], &mut value_struct, &expr).unwrap();
    assert!(inst.iter().any(|i| i.opcode == OpCode::JMPIF_L));
}
