use crate::syntax::parser::parse_source_file;

#[test]
fn rejects_package_call_when_arg_types_dont_match() {
    let src = r#"
        package demo;
        int add(int a, int b) { return a + b; }
        contract C {
            void m() {
                var x = add("a", "b");
            }
        }
    "#;
    let ast = parse_source_file(src).expect("parse");
    let err = ast.type_check().unwrap_err();
    assert!(
        err.to_string().contains("add") && err.to_string().contains("type mismatch"),
        "{err}"
    );
}

#[test]
fn accepts_matching_package_call() {
    let src = r#"
        package demo;
        int add(int a, int b) { return a + b; }
        contract C {
            void m() {
                var x = add(1, 2);
            }
        }
    "#;
    let ast = parse_source_file(src).expect("parse");
    ast.type_check().expect("typecheck");
}

#[test]
fn rejects_map_with_non_primitive_key_type() {
    let src = r#"
        package demo;
        contract C {
            void m() {
                var n = map[map[int, int], int] { map[int, int] { 1: 2 }: 5 };
            }
        }
    "#;
    let ast = parse_source_file(src).expect("parse");
    let err = ast.type_check().unwrap_err();
    assert!(err.to_string().contains("map key type must be"), "{err}");
}