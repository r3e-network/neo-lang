# neo-devpack

`neo-devpack` is the Neo N3 development pack foundation for `neo-lang`.

It is modeled after the responsibilities of `neo-project/neo-devpack-dotnet`, adapted for the current Rust-based `neo-lang` workspace.

## Modules

- `api`: typed Neo N3 framework, interop, and native contract catalog.
- `manifest`: Neo N3 manifest model and builder helpers.
- `standards`: NEP standard index and compatibility validators.
- `analyzer`: actionable findings built on top of standards validation.
- `templates`: built-in `.neo` contract templates.
- `testing`: fast in-memory storage, notification, and gas test primitives.

## Included Foundation Coverage

- Framework modules: Runtime, Storage, Contract, Crypto, Iterator.
- Native contracts: ContractManagement, StdLib, CryptoLib, Ledger, NEO, GAS, Policy, RoleManagement, Oracle.
- Standards index: NEP-11, NEP-17, NEP-24, NEP-26, NEP-27, NEP-29, NEP-30, NEP-31.
- Deep validators: NEP-17 and NEP-11 ABI/event shape.
- Templates: hello world, NEP-17 token, NEP-11 NFT, storage map, oracle consumer, upgradeable admin.
- Compiler integration: `neo-compiler` consumes this catalog for `neo-devpack` import validation and supports `runtime` import aliases.

## Compiler Imports

The compiler currently accepts direct runtime imports:

```neo
import rt from "neo-devpack/runtime";

contract NetworkInfo {
    #[safe]
    int network() {
        return rt.getNetwork();
    }
}
```

The root module form is also validated:

```neo
import runtime from "neo-devpack";
```

Unknown `neo-devpack/<module>` imports are rejected during type checking. Non-runtime modules are recognized by the catalog and intentionally report an explicit "not supported by neo-compiler yet" diagnostic until their syscall/native-contract lowerings are wired in.

## Example

```rust
use neo_devpack::api::ApiCatalog;
use neo_devpack::standards::{validate_standard, ContractShape, NepStandard};

let catalog = ApiCatalog::neo_n3();
let neo = catalog.native_contract("NEO").expect("NEO native contract");
assert!(neo.function("transfer").is_some());

let shape = ContractShape::new("Token").supported_standard(NepStandard::Nep17);
let errors = validate_standard(NepStandard::Nep17, &shape).unwrap_err();
assert!(errors.iter().any(|error| error.to_string().contains("transfer")));
```

## Current Limits

This crate is the devpack foundation. It does not yet execute NeoVM bytecode, generate debug info, lower every non-runtime framework module through imports, or run a Neo private network. Those are planned as follow-up layers after this stable API/test foundation.
