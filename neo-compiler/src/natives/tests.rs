use super::{load_decl_file, native_contract_by_name, parse_hash160_hex};
use crate::syntax::ast::Type;

#[test]
fn parse_hash160_hex_accepts_prefixed_literal() {
    let hash = parse_hash160_hex("0xef4073a0f2b305a38ec4050e4d3d28bc40ea63f5").unwrap();
    assert_eq!(hash[0], 0xef);
    assert_eq!(hash[19], 0xf5);
}

#[test]
fn builtin_registry_contains_native_contracts() {
    let neo = native_contract_by_name("NEO").expect("NEO");
    assert_eq!(neo.hash[0], 0xef);
    assert!(neo.resolve_method("balanceOf", 1).is_some());
    assert!(native_contract_by_name("ContractManagement").is_some());
    assert!(native_contract_by_name("StdLib").is_some());
}

#[test]
fn stdlib_resolves_overloads_by_arg_count() {
    let stdlib = native_contract_by_name("StdLib").expect("StdLib");
    assert!(stdlib.resolve_method("memorySearch", 2).is_some());
    assert!(stdlib.resolve_method("memorySearch", 3).is_some());
    assert!(stdlib.resolve_method("memorySearch", 4).is_some());
    assert!(stdlib.resolve_method("memorySearch", 1).is_none());
}

#[test]
fn load_decl_file_rejects_missing_hash_attribute() {
    let src = r#"
        declare contract Example {
            int foo();
        }
    "#;
    let err = load_decl_file(src).unwrap_err();
    assert!(err.to_string().contains("hash160"));
}

#[test]
fn load_user_decl_file_with_struct_return_type() {
    let src = r#"
        #[hash160("0x1234567890abcdef1234567890abcdef12345678")]
        declare contract Example {
            Transaction getTransaction(hash256 hash);
        }
    "#;
    let contracts = load_decl_file(src).unwrap();
    assert_eq!(contracts.len(), 1);
    assert_eq!(
        contracts[0].methods[0].return_ty,
        Type::Named("Transaction".into())
    );
}

#[test]
fn load_user_decl_file() {
    let src = r#"
        #[hash160("0x1234567890abcdef1234567890abcdef12345678")]
        declare contract MyToken {
            int balanceOf(hash160 account);
            bool transfer(hash160 from, hash160 to, int amount);
        }
    "#;
    let contracts = load_decl_file(src).unwrap();
    assert_eq!(contracts.len(), 1);
    assert_eq!(contracts[0].name, "MyToken");
    assert_eq!(contracts[0].methods.len(), 2);
}
