use neo_devpack::analyzer::{Analyzer, FindingSeverity};
use neo_devpack::api::{ApiCatalog, CallFlags};
use neo_devpack::manifest::{ContractManifest, ManifestBuilder};
use neo_devpack::native::{NativeContract, NativeValue};
use neo_devpack::standards::{standard_index, validate_standard, ContractShape, NepStandard};
use neo_devpack::templates::{render_template, TemplateKind, TemplateOptions};
use neo_devpack::testing::{DevPackTestContext, GasError};
use neo_devpack::types::{FunctionSpec, NeoType, ParameterSpec};

#[test]
fn api_catalog_exposes_core_framework_and_native_contracts() {
    let catalog = ApiCatalog::neo_n3();

    let runtime = catalog.module("runtime").expect("runtime module");
    assert!(runtime.function("checkWitness").is_some());
    assert_eq!(
        runtime
            .function("getExecutingScriptHash")
            .unwrap()
            .return_type,
        NeoType::Hash160
    );

    let storage = catalog.module("storage").expect("storage module");
    assert_eq!(
        storage.function("find").unwrap().required_call_flags,
        CallFlags::ReadStates
    );

    let neo = catalog.native_contract("NEO").expect("NEO native contract");
    assert_eq!(neo.hash, "0xef4073a0f2b305a38ec4050e4d3d28bc40ea63f5");
    assert!(neo.function("transfer").is_some());

    let native_names: Vec<_> = catalog
        .native_contracts()
        .iter()
        .map(|contract| contract.name)
        .collect();
    assert_eq!(
        native_names,
        vec![
            "ContractManagement",
            "StdLib",
            "CryptoLib",
            "Ledger",
            "NEO",
            "GAS",
            "Policy",
            "RoleManagement",
            "Oracle",
        ]
    );
}

#[test]
fn native_contract_bindings_validate_arguments_and_surface_call_metadata() {
    let alice = NativeValue::hash160("0x1111111111111111111111111111111111111111").unwrap();
    let bob = NativeValue::hash160("0x2222222222222222222222222222222222222222").unwrap();

    let transfer = NativeContract::Gas
        .call("transfer")
        .arg(alice.clone())
        .arg(bob)
        .arg(NativeValue::integer(42))
        .arg(NativeValue::null())
        .build()
        .expect("GAS transfer binding");

    assert_eq!(transfer.contract.name, "GAS");
    assert_eq!(
        transfer.contract_hash,
        "0xd2a4cff31913016155e38e474a2c06d08be276cf"
    );
    assert_eq!(transfer.method.name, "transfer");
    assert_eq!(transfer.method.return_type, NeoType::Boolean);
    assert_eq!(
        transfer.argument_types(),
        vec![
            NeoType::Hash160,
            NeoType::Hash160,
            NeoType::Integer,
            NeoType::Any,
        ]
    );

    let balance = NativeContract::Neo
        .call("balanceOf")
        .arg(alice.clone())
        .build()
        .expect("NEO balanceOf binding");
    assert!(balance.method.safe);
    assert_eq!(balance.method.return_type, NeoType::Integer);

    let arity_error = NativeContract::Gas
        .call("transfer")
        .arg(alice.clone())
        .build()
        .unwrap_err();
    assert!(arity_error.to_string().contains("expects 4 argument(s)"));

    let type_error = NativeContract::Neo
        .call("balanceOf")
        .arg(NativeValue::integer(1))
        .build()
        .unwrap_err();
    assert!(type_error.to_string().contains("expected `Hash160`"));
}

#[test]
fn native_value_accepts_neo_n3_addresses_as_hash160() {
    let account =
        NativeValue::address("NTRAJ9EEjHFHhHZvMKEKfkceg5V9ppx5ZP").expect("valid Neo N3 address");
    assert_eq!(
        account,
        NativeValue::Hash160("0x524e37b70139c896ebd54a8648d3fa786b264876".into())
    );

    let balance = NativeContract::Gas
        .call("balanceOf")
        .arg(account)
        .build()
        .expect("address should satisfy hash160 native parameter");
    assert_eq!(balance.argument_types(), vec![NeoType::Hash160]);

    let checksum_error = NativeValue::address("NTRAJ9EEjHFHhHZvMKEKfkceg5V9ppx5ZQ")
        .expect_err("mutated address checksum should fail");
    assert!(checksum_error.to_string().contains("checksum"));

    let version_error = NativeValue::address("1BoatSLRHtKNngkdXEeobR76b53LETtpyT")
        .expect_err("non-Neo address version should fail");
    assert!(version_error.to_string().contains("address version"));

    let base58_error =
        NativeValue::address("N0").expect_err("invalid Base58 characters should fail");
    assert!(base58_error.to_string().contains("Base58"));
}

#[test]
fn native_values_validate_common_neo_byte_types() {
    let hash256_hex = format!("0x{}", "ff".repeat(32));
    let hash256 = NativeValue::hash256(&hash256_hex).expect("valid hash256");
    assert_eq!(hash256.ty(), NeoType::Hash256);

    let public_key = NativeValue::public_key(
        "0x021111111111111111111111111111111111111111111111111111111111111111",
    )
    .expect("valid compressed public key");
    assert_eq!(public_key.ty(), NeoType::PublicKey);

    let signature_hex = format!("0x{}", "aa".repeat(64));
    let signature = NativeValue::signature(&signature_hex).expect("valid signature");
    assert_eq!(signature.ty(), NeoType::Signature);

    assert_eq!(
        NativeValue::byte_array("0xdeadbeef").expect("valid byte array"),
        NativeValue::ByteArray(vec![0xde, 0xad, 0xbe, 0xef])
    );
    assert_eq!(
        NativeValue::buffer("0xcafe").expect("valid buffer"),
        NativeValue::Buffer(vec![0xca, 0xfe])
    );

    let public_key_error =
        NativeValue::public_key("0x0211").expect_err("short public key should fail");
    assert!(public_key_error.to_string().contains("public key"));

    let hex_error = NativeValue::byte_array("0xabc").expect_err("odd hex length should fail");
    assert!(hex_error.to_string().contains("hex"));
}

#[test]
fn manifest_builder_serializes_neo_n3_fields() {
    let manifest = ManifestBuilder::new("Token")
        .supported_standard("NEP-17")
        .method(FunctionSpec::new("symbol", vec![], NeoType::String).safe())
        .event(FunctionSpec::new(
            "Transfer",
            vec![
                ParameterSpec::new("from", NeoType::Hash160),
                ParameterSpec::new("to", NeoType::Hash160),
                ParameterSpec::new("amount", NeoType::Integer),
            ],
            NeoType::Void,
        ))
        .extra("author", "core-dev")
        .build();

    let json = serde_json::to_string(&manifest).expect("manifest json");
    assert!(json.contains("\"supportedstandards\":[\"NEP-17\"]"));
    assert!(json.contains("\"safe\":true"));

    let roundtrip: ContractManifest = serde_json::from_str(&json).expect("roundtrip");
    assert_eq!(roundtrip.name, "Token");
    assert_eq!(roundtrip.abi.methods[0].name, "symbol");
}

#[test]
fn standards_validate_nep17_shape_and_report_missing_members() {
    let valid = ContractShape::new("Token")
        .supported_standard(NepStandard::Nep17)
        .method(FunctionSpec::new("totalSupply", vec![], NeoType::Integer).safe())
        .method(FunctionSpec::new("symbol", vec![], NeoType::String).safe())
        .method(FunctionSpec::new("decimals", vec![], NeoType::Integer).safe())
        .method(
            FunctionSpec::new(
                "balanceOf",
                vec![ParameterSpec::new("account", NeoType::Hash160)],
                NeoType::Integer,
            )
            .safe(),
        )
        .method(FunctionSpec::new(
            "transfer",
            vec![
                ParameterSpec::new("from", NeoType::Hash160),
                ParameterSpec::new("to", NeoType::Hash160),
                ParameterSpec::new("amount", NeoType::Integer),
                ParameterSpec::new("data", NeoType::Any),
            ],
            NeoType::Boolean,
        ))
        .event(FunctionSpec::new(
            "Transfer",
            vec![
                ParameterSpec::new("from", NeoType::Hash160),
                ParameterSpec::new("to", NeoType::Hash160),
                ParameterSpec::new("amount", NeoType::Integer),
            ],
            NeoType::Void,
        ));

    validate_standard(NepStandard::Nep17, &valid).expect("valid NEP-17");

    let invalid = ContractShape::new("Token").supported_standard(NepStandard::Nep17);
    let errors = validate_standard(NepStandard::Nep17, &invalid).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.to_string().contains("missing method `transfer`")));
}

#[test]
fn standards_validate_nep11_shape_and_publish_standard_index() {
    let index_names: Vec<_> = standard_index()
        .iter()
        .map(|standard| standard.standard.manifest_name())
        .collect();
    assert_eq!(
        index_names,
        vec!["NEP-11", "NEP-17", "NEP-24", "NEP-26", "NEP-27", "NEP-29", "NEP-30", "NEP-31"]
    );

    let valid = ContractShape::new("Collectible")
        .supported_standard(NepStandard::Nep11)
        .method(FunctionSpec::new("symbol", vec![], NeoType::String).safe())
        .method(FunctionSpec::new("decimals", vec![], NeoType::Integer).safe())
        .method(FunctionSpec::new("totalSupply", vec![], NeoType::Integer).safe())
        .method(FunctionSpec::new("tokens", vec![], NeoType::Iterator).safe())
        .method(
            FunctionSpec::new(
                "balanceOf",
                vec![ParameterSpec::new("owner", NeoType::Hash160)],
                NeoType::Integer,
            )
            .safe(),
        )
        .method(
            FunctionSpec::new(
                "tokensOf",
                vec![ParameterSpec::new("owner", NeoType::Hash160)],
                NeoType::Iterator,
            )
            .safe(),
        )
        .method(
            FunctionSpec::new(
                "ownerOf",
                vec![ParameterSpec::new("tokenId", NeoType::ByteArray)],
                NeoType::Hash160,
            )
            .safe(),
        )
        .method(
            FunctionSpec::new(
                "properties",
                vec![ParameterSpec::new("tokenId", NeoType::ByteArray)],
                NeoType::Map,
            )
            .safe(),
        )
        .method(FunctionSpec::new(
            "transfer",
            vec![
                ParameterSpec::new("to", NeoType::Hash160),
                ParameterSpec::new("tokenId", NeoType::ByteArray),
                ParameterSpec::new("data", NeoType::Any),
            ],
            NeoType::Boolean,
        ))
        .event(FunctionSpec::new(
            "Transfer",
            vec![
                ParameterSpec::new("from", NeoType::Hash160),
                ParameterSpec::new("to", NeoType::Hash160),
                ParameterSpec::new("amount", NeoType::Integer),
                ParameterSpec::new("tokenId", NeoType::ByteArray),
            ],
            NeoType::Void,
        ));

    validate_standard(NepStandard::Nep11, &valid).expect("valid NEP-11");
}

#[test]
fn analyzer_turns_standard_errors_into_actionable_findings() {
    let shape = ContractShape::new("BrokenToken").supported_standard(NepStandard::Nep17);
    let findings = Analyzer::new()
        .require_standard(NepStandard::Nep17)
        .analyze(&shape);

    assert!(findings
        .iter()
        .all(|finding| finding.severity == FindingSeverity::Error));
    assert!(findings
        .iter()
        .any(|finding| finding.code == "NEP17_MISSING_METHOD"
            && finding.message.contains("missing method `transfer`")));
}

#[test]
fn templates_render_professional_starting_points_without_unresolved_markers() {
    for kind in [
        TemplateKind::HelloWorld,
        TemplateKind::Nep17Token,
        TemplateKind::Nep11Nft,
        TemplateKind::StorageMap,
        TemplateKind::OracleConsumer,
        TemplateKind::UpgradeableAdmin,
    ] {
        let rendered = render_template(kind, &TemplateOptions::new("Sample")).expect("template");
        assert!(!rendered.files.is_empty());
        for file in &rendered.files {
            assert!(file.path.ends_with(".neo") || file.path.ends_with(".md"));
            assert!(!file.contents.contains("{{"));
            assert!(!file.contents.contains("}}"));
        }
    }

    let nep17 = render_template(
        TemplateKind::Nep17Token,
        &TemplateOptions::new("Token").symbol("TOK").decimals(8),
    )
    .unwrap();
    let source = &nep17.files[0].contents;
    assert!(source.contains("bool transfer(hash160 source, hash160 dest, int amount, any data)"));
    assert!(source.contains("event Transfer(hash160 source, hash160 dest, int amount);"));
    assert!(source.contains("runtime.checkWitness(source)"));
    assert!(source.contains("contractApi.call(dest"));
    assert!(source.contains("\"onNEP17Payment\""));

    let nep11 = render_template(TemplateKind::Nep11Nft, &TemplateOptions::new("Collectible"))
        .expect("NEP-11 template");
    let source = &nep11.files[0].contents;
    assert!(source.contains("iterator tokens()"));
    assert!(source.contains("bool transfer(hash160 to, buffer tokenId, any data)"));
    assert!(source
        .contains("event Transfer(hash160 source, hash160 dest, int amount, buffer tokenId);"));
}

#[test]
fn testing_context_tracks_storage_notifications_and_gas() {
    let mut ctx = DevPackTestContext::new("0x0123456789abcdef0123456789abcdef01234567");

    ctx.storage.put("balances:alice", 100_i32.to_le_bytes());
    assert_eq!(
        ctx.storage.get("balances:alice").unwrap(),
        100_i32.to_le_bytes().to_vec()
    );
    ctx.storage.put("balances:bob", 7_i32.to_le_bytes());
    assert_eq!(ctx.storage.find_prefix("balances:").len(), 2);
    ctx.storage.delete("balances:bob");
    assert!(ctx.storage.get("balances:bob").is_none());

    ctx.notifications
        .notify("Transfer", vec!["alice".into(), "bob".into(), "1".into()]);
    assert_eq!(ctx.notifications.all()[0].event_name, "Transfer");

    ctx.gas.charge(10).expect("within budget");
    assert_eq!(ctx.gas.consumed(), 10);
    assert_eq!(
        ctx.gas.charge(ctx.gas.remaining() + 1),
        Err(GasError::BudgetExceeded)
    );
}
