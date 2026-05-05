use neo_devpack::analyzer::{Analyzer, FindingSeverity};
use neo_devpack::api::{ApiCatalog, CallFlags};
use neo_devpack::framework::{Contract, FrameworkValue, Runtime, Storage};
use neo_devpack::manifest::{ContractManifest, ManifestBuilder};
use neo_devpack::native::{
    ContractManagement, CryptoLib, GasToken, Ledger, NativeContract, NativeValue, NeoToken, Oracle,
    Policy, RoleManagement, StdLib,
};
use neo_devpack::standards::{standard_index, validate_standard, ContractShape, NepStandard};
use neo_devpack::templates::{render_template, TemplateKind, TemplateOptions};
use neo_devpack::testing::{DevPackTestContext, GasError, NativeMockRegistry, StorageFindEntry};
use neo_devpack::types::{FindOptions, FunctionSpec, NeoType, ParameterSpec};

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
fn native_stdlib_and_cryptolib_helpers_build_typed_invocations() {
    let serialized =
        StdLib::serialize(NativeValue::String("hello".into())).expect("StdLib.serialize wrapper");
    assert_eq!(serialized.contract.name, "StdLib");
    assert_eq!(serialized.method.name, "serialize");
    assert_eq!(serialized.method.return_type, NeoType::ByteArray);
    assert_eq!(serialized.argument_types(), vec![NeoType::String]);

    let decoded = StdLib::base64_decode("aGVsbG8=").expect("StdLib.base64Decode wrapper");
    assert_eq!(decoded.method.name, "base64Decode");
    assert_eq!(decoded.argument_types(), vec![NeoType::String]);

    let message = NativeValue::byte_array("0xdeadbeef").expect("message bytes");
    let hash = CryptoLib::sha256(message.clone()).expect("CryptoLib.sha256 wrapper");
    assert_eq!(hash.contract.name, "CryptoLib");
    assert_eq!(hash.method.name, "sha256");
    assert_eq!(hash.method.return_type, NeoType::Hash256);

    let pub_key = NativeValue::public_key(
        "0x021111111111111111111111111111111111111111111111111111111111111111",
    )
    .expect("public key");
    let signature_hex = format!("0x{}", "aa".repeat(64));
    let signature = NativeValue::signature(&signature_hex).expect("signature");
    let verified = CryptoLib::verify_with_ecdsa(message, pub_key, signature, 23)
        .expect("CryptoLib.verifyWithECDsa wrapper");
    assert_eq!(verified.method.name, "verifyWithECDsa");
    assert_eq!(
        verified.argument_types(),
        vec![
            NeoType::ByteArray,
            NeoType::PublicKey,
            NeoType::Signature,
            NeoType::Integer,
        ]
    );

    let bad_hash = CryptoLib::sha256(NativeValue::integer(1))
        .expect_err("sha256 should enforce byte-array input");
    assert!(bad_hash.to_string().contains("expected `ByteArray`"));
}

#[test]
fn native_neo_and_gas_helpers_build_typed_invocations() {
    let alice = NativeValue::address("NTRAJ9EEjHFHhHZvMKEKfkceg5V9ppx5ZP").expect("address");
    let bob = NativeValue::hash160("0x2222222222222222222222222222222222222222").expect("hash160");

    let transfer = GasToken::transfer(alice.clone(), bob.clone(), 42, NativeValue::null())
        .expect("GAS transfer wrapper");
    assert_eq!(transfer.contract.name, "GAS");
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

    let balance = GasToken::balance_of(alice.clone()).expect("GAS balanceOf wrapper");
    assert!(balance.method.safe);
    assert_eq!(balance.method.name, "balanceOf");

    let supply = NeoToken::total_supply().expect("NEO totalSupply wrapper");
    assert_eq!(supply.contract.name, "NEO");
    assert_eq!(supply.method.return_type, NeoType::Integer);

    let unclaimed = NeoToken::unclaimed_gas(alice.clone(), 123).expect("NEO unclaimedGas wrapper");
    assert_eq!(unclaimed.method.name, "unclaimedGas");
    assert_eq!(
        unclaimed.argument_types(),
        vec![NeoType::Hash160, NeoType::Integer]
    );

    let vote_to = NativeValue::public_key(
        "0x021111111111111111111111111111111111111111111111111111111111111111",
    )
    .expect("public key");
    let vote = NeoToken::vote(alice, vote_to).expect("NEO vote wrapper");
    assert_eq!(vote.method.name, "vote");
    assert_eq!(
        vote.argument_types(),
        vec![NeoType::Hash160, NeoType::PublicKey]
    );

    let bad_transfer = GasToken::transfer(NativeValue::integer(1), bob, 1, NativeValue::null())
        .expect_err("transfer wrapper should keep hash160 type validation");
    assert!(bad_transfer.to_string().contains("expected `Hash160`"));
}

#[test]
fn remaining_native_contract_helpers_build_typed_invocations() {
    let account = NativeValue::hash160("0x1111111111111111111111111111111111111111").unwrap();
    let hash256 =
        NativeValue::hash256("0x2222222222222222222222222222222222222222222222222222222222222222")
            .unwrap();
    let nef = NativeValue::byte_array("0x4e454633").unwrap();

    let fee =
        ContractManagement::get_minimum_deployment_fee().expect("ContractManagement fee wrapper");
    assert_eq!(fee.contract.name, "ContractManagement");
    assert_eq!(fee.method.name, "getMinimumDeploymentFee");
    assert!(fee.method.safe);
    assert_eq!(fee.method.return_type, NeoType::Integer);

    let deploy = ContractManagement::deploy(nef.clone(), "{\"name\":\"demo\"}")
        .expect("ContractManagement deploy wrapper");
    assert_eq!(deploy.method.name, "deploy");
    assert_eq!(
        deploy.argument_types(),
        vec![NeoType::ByteArray, NeoType::String]
    );

    let block = Ledger::get_block(NativeValue::integer(123)).expect("Ledger getBlock wrapper");
    assert_eq!(block.contract.name, "Ledger");
    assert_eq!(block.method.name, "getBlock");
    assert!(block.method.safe);
    assert_eq!(block.argument_types(), vec![NeoType::Integer]);

    let tx = Ledger::get_transaction(hash256.clone()).expect("Ledger getTransaction wrapper");
    assert_eq!(tx.method.name, "getTransaction");
    assert_eq!(tx.argument_types(), vec![NeoType::Hash256]);

    let blocked = Policy::is_blocked(account.clone()).expect("Policy isBlocked wrapper");
    assert_eq!(blocked.contract.name, "Policy");
    assert_eq!(blocked.method.name, "isBlocked");
    assert_eq!(blocked.argument_types(), vec![NeoType::Hash160]);

    let designated = RoleManagement::get_designated_by_role(4, 100)
        .expect("RoleManagement getDesignatedByRole wrapper");
    assert_eq!(designated.method.name, "getDesignatedByRole");
    assert_eq!(
        designated.argument_types(),
        vec![NeoType::Integer, NeoType::Integer]
    );

    let request = Oracle::request(
        "https://example.com/price",
        "$.price",
        "onOracleResponse",
        NativeValue::String("request-1".into()),
        10_000_000,
    )
    .expect("Oracle request wrapper");
    assert_eq!(request.contract.name, "Oracle");
    assert_eq!(request.method.name, "request");
    assert_eq!(
        request.argument_types(),
        vec![
            NeoType::String,
            NeoType::String,
            NeoType::String,
            NeoType::String,
            NeoType::Integer,
        ]
    );

    let bad_policy =
        Policy::is_blocked(NativeValue::integer(1)).expect_err("Policy wrapper validates hash160");
    assert!(bad_policy.to_string().contains("expected `Hash160`"));

    let bad_ledger =
        Ledger::get_transaction(account).expect_err("Ledger wrapper validates hash256");
    assert!(bad_ledger.to_string().contains("expected `Hash256`"));
}

#[test]
fn framework_helpers_build_typed_syscall_invocations() {
    let network = Runtime::get_network().expect("Runtime.getNetwork wrapper");
    assert_eq!(network.module.name, "runtime");
    assert_eq!(network.function.name, "getNetwork");
    assert!(network.function.safe);
    assert_eq!(network.function.return_type, NeoType::Integer);

    let witness = Runtime::check_witness(FrameworkValue::PublicKey(vec![0x02; 33]))
        .expect("Runtime.checkWitness wrapper");
    assert_eq!(witness.function.name, "checkWitness");
    assert_eq!(witness.argument_types(), vec![NeoType::PublicKey]);

    let put = Storage::put(
        FrameworkValue::ByteArray(vec![b'k']),
        FrameworkValue::ByteArray(vec![b'v']),
    )
    .expect("Storage.put wrapper");
    assert_eq!(put.module.name, "storage");
    assert_eq!(put.function.name, "put");
    assert_eq!(put.function.required_call_flags, CallFlags::WriteStates);
    assert_eq!(
        put.argument_types(),
        vec![NeoType::ByteArray, NeoType::ByteArray]
    );

    let contract_call = Contract::call(
        FrameworkValue::Hash160("0x1111111111111111111111111111111111111111".into()),
        "balanceOf",
        CallFlags::ReadOnly,
        vec![FrameworkValue::Hash160(
            "0x2222222222222222222222222222222222222222".into(),
        )],
    )
    .expect("Contract.call wrapper");
    assert_eq!(contract_call.module.name, "contract");
    assert_eq!(contract_call.function.name, "call");
    assert_eq!(
        contract_call.argument_types(),
        vec![
            NeoType::Hash160,
            NeoType::String,
            NeoType::Integer,
            NeoType::Array,
        ]
    );

    let bad_put = Storage::put(
        FrameworkValue::Integer(1),
        FrameworkValue::ByteArray(vec![b'v']),
    )
    .expect_err("Storage.put should validate byte-array keys");
    assert!(bad_put.to_string().contains("expected `ByteArray`"));
}

#[test]
fn find_options_encode_and_validate_storage_find_flags() {
    assert_eq!(FindOptions::NONE.neo_bits(), 0);
    assert_eq!(
        FindOptions::KEYS_ONLY
            .with(FindOptions::REMOVE_PREFIX)
            .unwrap()
            .neo_bits(),
        0x03
    );

    let find = Storage::find(
        FrameworkValue::ByteArray(vec![b'p']),
        FindOptions::DESERIALIZE_VALUES
            .with(FindOptions::PICK_FIELD_0)
            .unwrap(),
    )
    .expect("Storage.find accepts typed FindOptions");
    assert_eq!(
        find.argument_types(),
        vec![NeoType::ByteArray, NeoType::Integer]
    );

    let values_and_keys = FindOptions::KEYS_ONLY
        .with(FindOptions::VALUES_ONLY)
        .expect_err("KeysOnly and ValuesOnly are mutually exclusive");
    assert!(values_and_keys.to_string().contains("KeysOnly"));

    let pick_without_deserialize = FindOptions::PICK_FIELD_1
        .validate()
        .expect_err("PickField requires DeserializeValues");
    assert!(pick_without_deserialize
        .to_string()
        .contains("DeserializeValues"));
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
fn standards_validate_payment_callback_receiver_shapes() {
    let nep17_receiver = ContractShape::new("Vault")
        .supported_standard(NepStandard::Nep27)
        .method(FunctionSpec::new(
            "onNEP17Payment",
            vec![
                ParameterSpec::new("from", NeoType::Hash160),
                ParameterSpec::new("amount", NeoType::Integer),
                ParameterSpec::new("data", NeoType::Any),
            ],
            NeoType::Void,
        ));
    validate_standard(NepStandard::Nep27, &nep17_receiver).expect("valid NEP-27 receiver");

    let missing_nep17_callback = ContractShape::new("Vault").supported_standard(NepStandard::Nep27);
    let errors = validate_standard(NepStandard::Nep27, &missing_nep17_callback).unwrap_err();
    assert!(errors.iter().any(|error| error
        .to_string()
        .contains("missing method `onNEP17Payment`")));

    let wrong_nep17_callback = ContractShape::new("Vault")
        .supported_standard(NepStandard::Nep27)
        .method(FunctionSpec::new(
            "onNEP17Payment",
            vec![
                ParameterSpec::new("from", NeoType::Hash160),
                ParameterSpec::new("amount", NeoType::Integer),
            ],
            NeoType::Void,
        ));
    let errors = validate_standard(NepStandard::Nep27, &wrong_nep17_callback).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.to_string().contains("signature")));

    let nep11_receiver = ContractShape::new("Gallery")
        .supported_standard(NepStandard::Nep26)
        .method(FunctionSpec::new(
            "onNEP11Payment",
            vec![
                ParameterSpec::new("from", NeoType::Hash160),
                ParameterSpec::new("amount", NeoType::Integer),
                ParameterSpec::new("tokenId", NeoType::ByteArray),
                ParameterSpec::new("data", NeoType::Any),
            ],
            NeoType::Void,
        ));
    validate_standard(NepStandard::Nep26, &nep11_receiver).expect("valid NEP-26 receiver");
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

    let receiver = ContractShape::new("Vault").supported_standard(NepStandard::Nep27);
    let findings = Analyzer::new()
        .require_standard(NepStandard::Nep27)
        .analyze(&receiver);
    assert!(findings
        .iter()
        .any(|finding| finding.code == "NEP27_MISSING_METHOD"
            && finding.message.contains("onNEP17Payment")));
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

#[test]
fn testing_storage_fixture_applies_find_options() {
    let mut ctx = DevPackTestContext::new("0x0123456789abcdef0123456789abcdef01234567");
    ctx.storage.put("balances:alice", [100]);
    ctx.storage.put("balances:bob", [7]);
    ctx.storage.put("owner:alice", [1]);

    let all = ctx
        .storage
        .find("balances:", FindOptions::NONE)
        .expect("default find");
    assert_eq!(
        all,
        vec![
            StorageFindEntry::KeyValue {
                key: b"balances:alice".to_vec(),
                value: vec![100],
            },
            StorageFindEntry::KeyValue {
                key: b"balances:bob".to_vec(),
                value: vec![7],
            },
        ]
    );

    let keys = ctx
        .storage
        .find(
            "balances:",
            FindOptions::KEYS_ONLY
                .with(FindOptions::REMOVE_PREFIX)
                .unwrap(),
        )
        .expect("keys only find");
    assert_eq!(
        keys,
        vec![
            StorageFindEntry::Key(b"alice".to_vec()),
            StorageFindEntry::Key(b"bob".to_vec()),
        ]
    );

    let values = ctx
        .storage
        .find("balances:", FindOptions::VALUES_ONLY)
        .expect("values only find");
    assert_eq!(
        values,
        vec![
            StorageFindEntry::Value(vec![100]),
            StorageFindEntry::Value(vec![7]),
        ]
    );
}

#[test]
fn testing_native_mocks_execute_typed_invocations() {
    let account = NativeValue::hash160("0x1111111111111111111111111111111111111111").unwrap();
    let balance = GasToken::balance_of(account.clone()).expect("GAS balanceOf invocation");
    let transfer = GasToken::transfer(
        account.clone(),
        NativeValue::hash160("0x2222222222222222222222222222222222222222").unwrap(),
        1,
        NativeValue::null(),
    )
    .expect("GAS transfer invocation");

    let mut mocks = NativeMockRegistry::new();
    mocks.when("GAS", "balanceOf", NativeValue::Integer(42));

    assert_eq!(mocks.invoke(&balance).unwrap(), NativeValue::Integer(42));

    let missing = mocks
        .invoke(&transfer)
        .expect_err("missing mock should fail");
    assert!(missing.to_string().contains("no native mock"));

    let mut ctx = DevPackTestContext::new("0xabc");
    ctx.native
        .when("GAS", "balanceOf", NativeValue::String("bad".into()));
    let type_error = ctx
        .native
        .invoke(&balance)
        .expect_err("mock response should match native return type");
    assert!(type_error.to_string().contains("expected `Integer`"));
}
