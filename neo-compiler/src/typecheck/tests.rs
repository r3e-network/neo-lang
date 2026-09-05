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
    parse_source_file(src)
        .expect("parse")
        .type_check()
        .expect("typecheck");
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
    parse_source_file(src)
        .expect("parse")
        .type_check()
        .expect("typecheck");
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
    parse_source_file(src)
        .expect("parse")
        .type_check()
        .expect("typecheck");
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
fn accepts_any_array_literal_with_mixed_element_types() {
    let src = r#"
        contract C {
            void m(hash160 source, int amount) {
                runtime.call(source, "OnNEP17Payment", any[]{source, amount, null});
            }
        }
    "#;
    parse_source_file(src)
        .expect("parse")
        .type_check()
        .expect("typecheck");
}

#[test]
fn accepts_native_contract_call() {
    let src = r#"
        contract C {
            bool m(hash160 dest) {
                return ContractManagement.isContract(dest);
            }
        }
    "#;
    parse_source_file(src)
        .expect("parse")
        .type_check()
        .expect("typecheck");
}

#[test]
fn accepts_stdlib_native_call() {
    let src = r#"
        contract C {
            int m(string s) {
                return StdLib.strLen(s);
            }
        }
    "#;
    parse_source_file(src)
        .expect("parse")
        .type_check()
        .expect("typecheck");
}

#[test]
fn accepts_builtin_transaction_struct_literal_and_member() {
    let src = r#"
        contract C {
            hash256 m() {
                var tx = Transaction {
                    hash: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                    version: 0,
                    nonce: 0,
                    sender: "0x1234567890abcdef1234567890abcdef12345678",
                    systemFee: 0,
                    networkFee: 0,
                    validUntilBlock: 0,
                    script: ""
                };
                return tx.hash;
            }
        }
    "#;
    parse_source_file(src)
        .expect("parse")
        .type_check()
        .expect("typecheck");
}

#[test]
fn rejects_user_struct_shadowing_builtin_transaction() {
    let src = r#"
        struct Transaction {
            int x;
        }
        contract C {
            void m() {}
        }
    "#;
    let ast = parse_source_file(src).expect("parse");
    let err = ast.type_check().unwrap_err();
    assert!(err.to_string().contains("Transaction"), "{err}");
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
