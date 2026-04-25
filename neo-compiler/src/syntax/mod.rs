//! Abstract syntax tree for neo-lang (see README.md).

pub mod ast;
pub mod lexer;
pub mod parser;

#[cfg(test)]
mod tests {
    use super::ast::*;
    use super::parser::parse_source_file;

    fn assert_parse_ok(source: &str) {
        parse_source_file(source).unwrap_or_else(|e| panic!("parse error: {:?}", e));
    }

    #[test]
    fn package_declaration() {
        let source = r#"
        package math;
        "#;
        let sf = parse_source_file(source).unwrap();
        assert_eq!(sf.package.as_deref(), Some("math"));
        assert!(sf.imports.is_empty());
        assert!(sf.structs.is_empty());
        assert!(sf.functions.is_empty());
        assert!(sf.contract.is_none());
    }

    #[test]
    fn import_declaration() {
        let source = r#"
        import foo from "mylib";
        "#;
        let sf = parse_source_file(source).unwrap();
        assert_eq!(sf.imports.len(), 1);
        assert_eq!(sf.imports[0].name, "foo");
        assert_eq!(sf.imports[0].library, "mylib");
    }

    #[test]
    fn package_then_imports() {
        let source = r#"
        package p;
        import a from "liba";
        import b from "libb";
        "#;
        let sf = parse_source_file(source).unwrap();
        assert_eq!(sf.package.as_deref(), Some("p"));
        assert_eq!(sf.imports.len(), 2);
    }

    #[test]
    fn struct_with_fields_and_optional_init() {
        let source = r#"
        struct Transfer {
            hash160 sender;
            int amount = 0;
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        assert_eq!(sf.structs.len(), 1);
        let s = &sf.structs[0];
        assert_eq!(s.name, "Transfer");
        assert_eq!(s.fields.len(), 2);
        assert_eq!(s.fields[0].name, "sender");
        assert_eq!(s.fields[0].ty, Type::Hash160);
        assert!(s.fields[0].init.is_none());
        assert_eq!(s.fields[1].name, "amount");
        assert_eq!(s.fields[1].ty, Type::Int);
        assert_eq!(
            s.fields[1].init,
            Some(Expr::Literal(Literal::Int("0".into())))
        );
        assert!(s.methods.is_empty());
    }

    #[test]
    fn struct_with_method() {
        let source = r#"
        struct Point {
            int x;
            int y;
            int distanceTo(Point other) {
                return (self.x - other.x) * (self.x - other.x)
                    + (self.y - other.y) * (self.y - other.y);
            }
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        assert_eq!(sf.structs.len(), 1);
        let s = &sf.structs[0];
        assert_eq!(s.name, "Point");
        assert_eq!(s.fields.len(), 2);
        assert_eq!(s.methods.len(), 1);
        assert_eq!(s.methods[0].name, "distanceTo");
        assert_eq!(s.methods[0].params.len(), 1);
        assert_eq!(s.methods[0].params[0].name, "other");
    }

    #[test]
    fn contract_with_attributes() {
        let source = r#"
        #[auther("AuthorName")]
        #[version("0.0.1")]
        contract Example {
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        let c = sf.contract.as_ref().unwrap();
        assert_eq!(c.name, "Example");
        assert_eq!(c.attributes.len(), 2);
        assert_eq!(c.attributes[0].name, "auther");
        assert_eq!(c.attributes[0].args, vec!["AuthorName".to_string()]);
        assert_eq!(c.attributes[1].name, "version");
        assert_eq!(c.attributes[1].args, vec!["0.0.1".to_string()]);
    }

    #[test]
    fn contract_const_and_mutable_fields() {
        let source = r#"
        contract C {
            const string symbol = "Ex";
            const int decimals = 8;
            int totalSupply = 1000;
            map[hash160, int] balances;
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        let m = &sf.contract.as_ref().unwrap().members;
        assert_eq!(m.len(), 4);
        match &m[0] {
            ContractMember::ConstProp(p) => {
                assert_eq!(p.name, "symbol");
                assert_eq!(p.ty, Type::String);
                assert_eq!(p.init, Expr::Literal(Literal::String("Ex".into())));
            }
            _ => panic!("expected const"),
        }
        match &m[2] {
            ContractMember::Field(f) => {
                assert_eq!(f.name, "totalSupply");
                assert_eq!(f.ty, Type::Int);
                assert_eq!(f.init, Some(Expr::Literal(Literal::Int("1000".into()))));
            }
            _ => panic!("expected field"),
        }
        match &m[3] {
            ContractMember::Field(f) => {
                assert_eq!(f.name, "balances");
                assert_eq!(
                    f.ty,
                    Type::Map {
                        key: Box::new(Type::Hash160),
                        value: Box::new(Type::Int),
                    }
                );
                assert!(f.init.is_none());
            }
            _ => panic!("expected field"),
        }
    }

    #[test]
    fn contract_event() {
        let source = r#"
        contract C {
            event transfer(hash160 sender, hash160 to, int amount);
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        match &sf.contract.as_ref().unwrap().members[0] {
            ContractMember::Event(event) => {
                assert_eq!(event.name, "transfer");
                assert_eq!(event.params.len(), 3);
                assert_eq!(event.params[0].name, "sender");
            }
            _ => panic!("expected event"),
        }
    }

    #[test]
    fn contract_method_with_attributes_and_body() {
        let source = r#"
        contract C {
            #[pure]
            bool transfer(hash160 sender, int amount) {
                return true;
            }
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        match &sf.contract.as_ref().unwrap().members[0] {
            ContractMember::Function(f) => {
                assert_eq!(f.name, "transfer");
                assert_eq!(f.return_ty, Type::Bool);
                assert_eq!(f.attributes.len(), 1);
                assert_eq!(f.attributes[0].name, "pure");
                assert!(f.attributes[0].args.is_empty());
                assert_eq!(f.params.len(), 2);
                match &f.body.stmts[..] {
                    [Stmt::Return(Some(Expr::Literal(Literal::Bool(true))))] => {}
                    _ => panic!("unexpected body: {:?}", f.body.stmts),
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn top_level_package_function() {
        let source = r#"
        package math;
        int add(int a, int b) {
            return a + b;
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        assert_eq!(sf.functions.len(), 1);
        let f = &sf.functions[0];
        assert_eq!(f.name, "add");
        assert_eq!(f.return_ty, Type::Int);
    }

    #[test]
    fn only_one_contract_allowed() {
        let source = r#"
        contract A {}
        contract B {}
        "#;
        let err = parse_source_file(source).unwrap_err();
        assert!(err.message.contains("only one contract"));
    }

    #[test]
    fn literals_null_bool_int_string_buffer() {
        let source = r#"
        void f() {
            var a = null;
            var b = true;
            var c = false;
            var d = 42;
            var e = 0xFF;
            var f2 = 0b1010;
            var g = "hi";
            var h = b"dead";
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        let stmts = &sf.functions[0].body.stmts;
        assert_eq!(stmts.len(), 8);
        let expected = [
            Literal::Null,
            Literal::Bool(true),
            Literal::Bool(false),
            Literal::Int("42".into()),
            Literal::Int("0xFF".into()),
            Literal::Int("0b1010".into()),
            Literal::String("hi".into()),
            Literal::Buffer("dead".into()),
        ];
        for (index, lit) in expected.iter().enumerate() {
            match &stmts[index] {
                Stmt::Var {
                    init: Some(Expr::Literal(l)),
                    ..
                } => assert_eq!(l, lit),
                _ => panic!("stmt {index}: {:?}", stmts[index]),
            }
        }
    }

    #[test]
    fn expr_self_member_index_call() {
        let source = r#"
        void f() {
            self.totalSupply = self.totalSupply + 1;
            var x = balances[caller];
            assert(x > 0, "message");
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        let stmts = &sf.functions[0].body.stmts;
        assert!(matches!(&stmts[0], Stmt::Expr(Expr::Assign { .. })));
        assert!(matches!(&stmts[1], Stmt::Var { .. }));
        match &stmts[2] {
            Stmt::Expr(Expr::Call { callee, args }) => {
                match callee.as_ref() {
                    Expr::Ident(name) => assert_eq!(name, "assert"),
                    _ => panic!("callee {:?}", callee),
                }
                assert_eq!(args.len(), 2);
            }
            _ => panic!("{:?}", stmts[2]),
        }
    }

    #[test]
    fn cast_as_chain() {
        let source = r#"
        void f() {
            var b = 1 as bool as string;
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        match &sf.functions[0].body.stmts[0] {
            Stmt::Var { init: Some(e), .. } => match e {
                Expr::Cast {
                    ty: Type::String,
                    expr,
                } => match expr.as_ref() {
                    Expr::Cast {
                        ty: Type::Bool,
                        expr: inner,
                    } => match inner.as_ref() {
                        Expr::Literal(Literal::Int(s)) => assert_eq!(s, "1"),
                        _ => panic!(),
                    },
                    _ => panic!(),
                },
                _ => panic!("{:?}", e),
            },
            _ => panic!(),
        }
    }

    #[test]
    fn struct_literal_pascal_case() {
        let source = r#"
        void f() {
            var t = Transfer { sender: x, amount: 100 };
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        match &sf.functions[0].body.stmts[0] {
            Stmt::Var {
                init: Some(Expr::StructLit { name, fields }),
                ..
            } => {
                assert_eq!(name, "Transfer");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].0, "sender");
                assert_eq!(fields[1].0, "amount");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn array_and_map_literals() {
        let source = r#"
        void f() {
            var a = int[] { 1, 2, 3 };
            var m = map[string, int] { "k1": 1 };
            var m2 = map[int, string] { 2: "v2" };
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        match &sf.functions[0].body.stmts[0] {
            Stmt::Var {
                init: Some(Expr::ArrayLit { ty, elements }),
                ..
            } => {
                assert_eq!(ty, &Type::Array(Box::new(Type::Int)),);
                assert_eq!(elements.len(), 3);
            }
            _ => panic!(),
        }
        match &sf.functions[0].body.stmts[1] {
            Stmt::Var {
                init: Some(Expr::MapLit { ty, pairs }),
                ..
            } => {
                assert_eq!(
                    ty,
                    &Type::Map {
                        key: Box::new(Type::String),
                        value: Box::new(Type::Int),
                    }
                );
                assert_eq!(pairs.len(), 1);
            }
            _ => panic!(),
        }
        match &sf.functions[0].body.stmts[2] {
            Stmt::Var {
                init: Some(Expr::MapLit { ty, pairs }),
                ..
            } => {
                assert_eq!(
                    ty,
                    &Type::Map {
                        key: Box::new(Type::Int),
                        value: Box::new(Type::String),
                    }
                );
                assert_eq!(pairs.len(), 1);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn bare_bracket_array_literal_is_rejected() {
        let source = r#"
        void f() {
            var a = [1, 2, 3];
        }
        "#;
        let e = parse_source_file(source).expect_err("bare [ ] array should not parse");
        assert!(
            e.message.contains("ElemType[]"),
            "unexpected error: {:?}",
            e.message
        );
    }

    #[test]
    fn bare_brace_map_literal_is_rejected() {
        let source = r#"
        void f() {
            var m = { "a": 1 };
        }
        "#;
        let e = parse_source_file(source).expect_err("bare { } map should not parse");
        assert!(
            e.message.contains("map<KeyType, ValueType>"),
            "unexpected error: {:?}",
            e.message
        );
    }

    #[test]
    fn arithmetic_and_bitwise_binary_ops() {
        let source = r#"
        void f() {
            var x = 1 + 2 * 3 - 4 / 2 % 2;
            var y = 1 << 2 >> 1;
            var z = 3 & 5 | 6 ^ 7;
        }
        "#;
        assert_parse_ok(source);
    }

    #[test]
    fn comparison_and_logical_ops() {
        let source = r#"
        void f() {
            var a = 1 == 2 != 3 < 4 <= 5 > 6 >= 7;
            var b = !true && false || true;
        }
        "#;
        assert_parse_ok(source);
    }

    #[test]
    fn unary_plus_minus_not_bitnot() {
        let source = r#"
        void f() {
            var x = +1;
            var y = -2;
            var a = !false;
            var b = ~0;
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        for stmt in &sf.functions[0].body.stmts {
            assert!(matches!(stmt, Stmt::Var { .. }));
        }
    }

    #[test]
    fn compound_assignments() {
        let source = r#"
        void f() {
            x = 1;
            x += 1;
            x -= 1;
            x *= 1;
            x /= 1;
            x %= 1;
            x >>= 1;
            x <<= 1;
            x &= 1;
            x |= 1;
            x ^= 1;
        }
        "#;
        assert_parse_ok(source);
    }

    #[test]
    fn paren_expr() {
        let source = r#"
        void f() {
            var x = (1 + 2) * 3;
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        match &sf.functions[0].body.stmts[0] {
            Stmt::Var {
                init:
                    Some(Expr::Binary {
                        op: BinaryOp::Mul, ..
                    }),
                ..
            } => {}
            _ => panic!(),
        }
    }

    #[test]
    fn stmt_if_else_while() {
        let source = r#"
        void f() {
            if a {
                return;
            } else {
                b = 1;
            }
            while cond {
                c = 2;
            }
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        let s = &sf.functions[0].body.stmts;
        assert!(matches!(
            &s[0],
            Stmt::If {
                else_block: Some(_),
                ..
            }
        ));
        assert!(matches!(&s[1], Stmt::While { .. }));
    }

    #[test]
    fn stmt_for_in_array_and_map() {
        let source = r#"
        void f() {
            for item in arr {
                x = item;
            }
            for k, v in m {
                y = k;
            }
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        let s = &sf.functions[0].body.stmts;
        match &s[0] {
            Stmt::ForArray { item, .. } => assert_eq!(item, "item"),
            _ => panic!(),
        }
        match &s[1] {
            Stmt::ForMap { key, value, .. } => {
                assert_eq!(key, "k");
                assert_eq!(value, "v");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn stmt_emit_and_nested_block() {
        let source = r#"
        void f() {
            emit transfer(sender, to, amount);
            {
                var inner = 1;
            }
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        let s = &sf.functions[0].body.stmts;
        match &s[0] {
            Stmt::Emit { name, args } => {
                assert_eq!(name, "transfer");
                assert_eq!(args.len(), 3);
            }
            _ => panic!(),
        }
        assert!(matches!(&s[1], Stmt::Block(_)));
    }

    #[test]
    fn types_void_named_and_array_params() {
        let source = r#"
        struct Point { int x; }
        void usePoint(Point p, int[] items, Point[] refs) {
            return;
        }
        "#;
        let sf = parse_source_file(source).unwrap();
        assert_eq!(sf.structs.len(), 1);
        let f = &sf.functions[0];
        assert_eq!(f.return_ty, Type::Void);
        assert_eq!(f.params[0].ty, Type::Named("Point".into()));
        assert_eq!(f.params[1].ty, Type::Array(Box::new(Type::Int)));
        assert_eq!(
            f.params[2].ty,
            Type::Array(Box::new(Type::Named("Point".into())))
        );
    }

    #[test]
    fn line_comment_skipped() {
        let source = r#"
        // leading
        void f() {
            return 1; // trailing
        }
        "#;
        assert_parse_ok(source);
    }
}
