# neo-devpack

`neo-devpack` is the Neo N3 development pack foundation for `neo-lang`.

It is modeled after the responsibilities of `neo-project/neo-devpack-dotnet`, adapted for the current Rust-based `neo-lang` workspace.

## Modules

- `api`: typed Neo N3 framework, interop, and native contract catalog.
- `framework`: typed framework syscall invocation metadata and argument validation.
- `manifest`: Neo N3 manifest model and builder helpers.
- `native`: typed native-contract invocation metadata and argument validation.
- `standards`: NEP standard index and compatibility validators.
- `analyzer`: actionable findings built on top of standards validation.
- `templates`: built-in `.neo` contract templates.
- `testing`: fast in-memory storage, notification, gas, and native-call mock test primitives.

## Included Foundation Coverage

- Framework modules: Runtime, Storage, Contract, Crypto, Iterator.
- Framework helpers: `Runtime`, `Storage`, `Contract`, `Crypto`, and `IteratorApi` build typed syscall invocations using the shared API catalog.
- Native contracts: ContractManagement, StdLib, CryptoLib, Ledger, NEO, GAS, Policy, RoleManagement, Oracle.
- Typed native-contract invocation builders with arity/type validation.
- `StdLib` and `CryptoLib` helper wrappers build typed native invocations for serialization, base encodings, hashing, and ECDSA verification.
- `GasToken` and `NeoToken` helper wrappers build typed native invocations for balances, transfers, supply, and NEO governance calls.
- `ContractManagement`, `Ledger`, `Policy`, `RoleManagement`, and `Oracle` helper wrappers cover the remaining Neo N3 native contracts with typed arity and argument validation.
- `NativeValue::address` validates Neo N3 Base58Check addresses, version `0x35`, and checksums before converting to `Hash160`.
- `NativeValue` constructors validate hash160, hash256, public key, signature, byte array, and buffer inputs before they reach native-call builders.
- Standards index: NEP-11, NEP-17, NEP-24, NEP-26, NEP-27, NEP-29, NEP-30, NEP-31.
- Deep validators: NEP-17 and NEP-11 ABI/event shape, plus NEP-26/NEP-27 payment receiver callback shape.
- Templates: hello world, NEP-17 token, NEP-11 NFT, storage map, oracle consumer, upgradeable admin.
- NEP-17 starter contracts include sender witness validation and an `onNEP17Payment` receiver callback scaffold using explicit `Contract.Call` flags.
- Analyzer findings flag missing or malformed NEP-26/NEP-27 receiver callbacks before contracts are used as token recipients.
- Compiler integration: `neo-compiler` consumes this catalog for `neo-devpack` import validation, runtime/storage/contract/crypto/iterator syscall imports, and NEP-17/NEP-11 `supportedStandards` ABI validation.
- Template compile checks: all built-in `.neo` templates are parsed, type checked, code generated, and converted to manifests in the compiler test suite.
- Testing helpers include `NativeMockRegistry` for deterministic native-contract call responses with return-type validation.

## Compiler Imports

The compiler currently accepts direct framework imports:

```neo,compile
import rt from "neo-devpack/runtime";
import storage from "neo-devpack";
import contractApi from "neo-devpack/contract";
import crypto from "neo-devpack";
import iterator from "neo-devpack";

contract NetworkInfo {
    #[safe]
    int network() {
        return rt.getNetwork();
    }

    #[safe]
    buffer read() {
        return storage.localGet("key");
    }

    #[safe]
    int flags() {
        return contractApi.getCallFlags();
    }

    #[safe]
    bool verify(buffer pubKey, buffer signature) {
        return crypto.checkSig(pubKey, signature);
    }

    #[safe]
    bool hasPrefix() {
        var entries = storage.localFind("prefix", 0);
        return iterator.next(entries);
    }
}
```

The root module form is also validated:

```neo
import runtime from "neo-devpack";
```

Unknown `neo-devpack/<module>` imports are rejected during type checking. Runtime, storage, contract, crypto, and iterator methods with direct NeoVM syscall mappings are type checked and emitted through the compiler.

## Example

```rust
use neo_devpack::api::ApiCatalog;
use neo_devpack::framework::{Contract, FrameworkValue, Runtime, Storage};
use neo_devpack::native::{
    CryptoLib, GasToken, NativeContract, NativeValue, Oracle, Policy, StdLib,
};
use neo_devpack::standards::{validate_standard, ContractShape, NepStandard};
use neo_devpack::testing::DevPackTestContext;

let catalog = ApiCatalog::neo_n3();
let neo = catalog.native_contract("NEO").expect("NEO native contract");
assert!(neo.function("transfer").is_some());

let shape = ContractShape::new("Token").supported_standard(NepStandard::Nep17);
let errors = validate_standard(NepStandard::Nep17, &shape).unwrap_err();
assert!(errors.iter().any(|error| error.to_string().contains("transfer")));

let alice = NativeValue::address("NTRAJ9EEjHFHhHZvMKEKfkceg5V9ppx5ZP")?;

let transfer = NativeContract::Gas
    .call("transfer")
    .arg(alice)
    .arg(NativeValue::hash160("0x2222222222222222222222222222222222222222")?)
    .arg(NativeValue::integer(1))
    .arg(NativeValue::null())
    .build()?;
assert_eq!(transfer.method.name, "transfer");

let gas_transfer = GasToken::transfer(
    NativeValue::address("NTRAJ9EEjHFHhHZvMKEKfkceg5V9ppx5ZP")?,
    NativeValue::hash160("0x2222222222222222222222222222222222222222")?,
    1,
    NativeValue::null(),
)?;
assert_eq!(gas_transfer.method.name, "transfer");

let digest = CryptoLib::sha256(NativeValue::byte_array("0xdeadbeef")?)?;
assert_eq!(digest.method.name, "sha256");

let encoded = StdLib::base64_encode(NativeValue::byte_array("0x68656c6c6f")?)?;
assert_eq!(encoded.method.name, "base64Encode");

let network = Runtime::get_network()?;
assert_eq!(network.function.name, "getNetwork");

let storage_put = Storage::put(
    FrameworkValue::ByteArray(vec![b'k']),
    FrameworkValue::ByteArray(vec![b'v']),
)?;
assert_eq!(storage_put.function.name, "put");

let low_level_call = Contract::call(
    FrameworkValue::from(NativeValue::address("NTRAJ9EEjHFHhHZvMKEKfkceg5V9ppx5ZP")?),
    "balanceOf",
    neo_devpack::types::CallFlags::ReadOnly,
    vec![],
)?;
assert_eq!(low_level_call.function.name, "call");

let blocked = Policy::is_blocked(NativeValue::address("NTRAJ9EEjHFHhHZvMKEKfkceg5V9ppx5ZP")?)?;
assert_eq!(blocked.method.name, "isBlocked");

let oracle_request = Oracle::request(
    "https://example.com/price",
    "$.price",
    "onOracleResponse",
    NativeValue::String("request-1".into()),
    10_000_000,
)?;
assert_eq!(oracle_request.method.name, "request");

let balance = GasToken::balance_of(NativeValue::address("NTRAJ9EEjHFHhHZvMKEKfkceg5V9ppx5ZP")?)?;
let mut ctx = DevPackTestContext::new("0x0123456789abcdef0123456789abcdef01234567");
ctx.native.when("GAS", "balanceOf", NativeValue::Integer(100));
assert_eq!(ctx.native.invoke(&balance)?, NativeValue::Integer(100));
```

## Current Limits

This crate is the devpack foundation. It does not yet execute NeoVM bytecode, generate debug info, compile typed native-contract calls directly from `.neo` source, or run a Neo private network. Those are planned as follow-up layers after this stable API/test foundation.
