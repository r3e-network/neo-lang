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
fn accepts_simple_contract_self_method_call() {
    let src = r#"
        contract C {
            bool helper(int x) { return x >= 0; }
            bool m(int x) { return self.helper(x); }
        }
    "#;
    parse_source_file(src).expect("parse").type_check().expect("typecheck");
}

#[test]
fn accepts_contract_self_method_call_in_if_condition() {
    let src = r#"
        contract C {
            bool helper(int x) { return x >= 0; }
            bool m(int x) {
                if !self.helper(x) { return false; }
                return true;
            }
        }
    "#;
    parse_source_file(src).expect("parse").type_check().expect("typecheck");
}

#[test]
fn accepts_contract_map_index_in_method() {
    let src = r#"
        contract C {
            map[hash160, int] _balances;
            int get(hash160 owner) {
                return self._balances[owner];
            }
        }
    "#;
    parse_source_file(src).expect("parse").type_check().expect("typecheck");
}

#[test]
fn accepts_contract_self_method_call() {
    let src = r#"
        contract NEP17 {
            map[hash160, int] _balances;
            bool transfer(hash160 source, hash160 dest, int amount) {
                if amount > 0 {
                    if !self._updateBalance(source, -amount) {
                        return false;
                    }
                }
                return true;
            }
            bool _updateBalance(hash160 owner, int amount) {
                var balance = self._balances[owner];
                balance += amount;
                return balance >= 0;
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
