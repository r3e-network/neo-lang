# Neo N3 Compiler and Devpack Coverage Plan

Date: 2026-05-05
Branch: codex/neo-n3-coverage-audit

## Goal

Make `neo-lang` a credible, production-oriented Neo N3 smart contract stack:

- Compiler output is valid, deterministic NeoVM bytecode and complete NEF + manifest artifacts.
- Language and standard library expose the Neo N3 contract surface in a typed, ergonomic way.
- Devpack includes reusable native contract wrappers, storage APIs, token templates, tests, examples, docs, and validation tooling.
- Coverage is measured against official Neo N3 docs, NEPs, and the reference DevPack, not only against local unit tests.

## Current State

The repository currently contains one Rust crate, `neo-compiler`, plus two examples. There is no separate devpack/framework/testing/template package yet.

Implemented compiler areas:

- Lexer/parser for the custom language, including contract, package, import syntax, structs, functions, arrays, maps, loops, attributes, and events.
- Type checker for many expression and statement cases.
- IR lowering, simple SSA-style optimization, stackification, NeoVM instruction builder, NEF writer, manifest writer, asm and disasm commands.
- Partial runtime syscall support under `runtime.*`.
- Partial storage support for contract fields and map-backed storage.
- Partial NEP-17 example.

Recently fixed in active PR work:

- Long jump opcode classification.
- Contract field initializers emitted in deploy bytecode.
- Signed 256-bit integer literals.
- Contract manifest attributes for author, standards, permissions, trusts, and groups.
- Compiler-side NEP-17/NEP-11 compatibility validation for `supportedStandards`.
- Compiler compile checks for all built-in devpack templates.
- GitHub Actions CI and docs examples marked with `neo,compile`.
- NEP-17 starter template sender witness checks and `onNEP17Payment` callback scaffolding through typed `Contract.Call` flags.
- Devpack native-value helper for validating Neo N3 Base58Check addresses and converting them into `Hash160` values for native calls.
- Devpack native-value constructors for validated hash256, public key, signature, byte array, and buffer values.
- NEP-26/NEP-27 receiver callback ABI validation for contracts that declare token payment callback support.
- StdLib/CryptoLib native wrapper pilot for typed serialization, base encoding, hashing, and ECDSA invocation helpers.
- README attribute mismatch for implemented attributes.

## Primary Gaps

1. Devpack does not exist as a first-class product.
   The official Neo DevPack includes framework APIs, compiler, testing, disassembler, analyzer, templates, interface generation, debug artifacts, gas/coverage tooling, and sample standards. This repository currently provides only a compiler CLI and examples.

2. Manifest and NEF output are minimal.
   Manifest generation currently leaves `groups`, `supported_standards`, `permissions`, and `trusts` mostly defaulted, while Neo N3 behavior depends on declared ABI, permissions, trusts, groups, supported standards, and extra metadata. NEF method tokens and source/debug metadata are not emitted.

3. Neo N3 system API coverage is incomplete.
   Local syscall metadata includes many `Runtime` and `Storage` syscalls, but user-facing APIs are narrow. Iterator, service classes, full storage context APIs, contract call flags, native contract wrappers, and native method surface are missing.

4. Native contracts are not exposed as typed devpack modules.
   Official Neo N3 native contracts include ContractManagement, CryptoLib, GAS, Ledger, NEO, Oracle, Policy, RoleManagement, and StdLib. The language currently only exposes generic `runtime.contractCall` and some direct runtime/storage behavior.

5. Standards support is incomplete.
   NEP-17, NEP-11, NEP-26, and NEP-27 now have devpack metadata plus ABI validation for token and payment callback shapes, but still need deeper runtime semantics such as private-network execution fixtures and negative callback behavior tests. NEP-24, NEP-29, NEP-30, and NEP-31 still need deeper typed helpers beyond metadata.

6. Imports/packages are parsed but not a resolved module system.
   `import name from "library";` appears in AST and docs, but there is no package resolver, multi-file compilation, dependency graph, export model, namespace hygiene, or artifact packaging.

7. Type system needs Neo-specific value types.
   `hash160`, `hash256`, public keys, signatures, byte strings, and Neo N3 addresses now have devpack-side native-value validation, but compiler literals, nullable values, iterators, storage contexts, contract/state structures, and call flags still need precise source-level representations. Hash/address literal handling should be made explicit in the language.

8. Testing is useful but not complete.
   Local unit tests pass, but there is no golden corpus for NEF/manifest, no fixture-based standard contract tests, no cross-check against NeoVM execution, no property/fuzz tests for bytecode layout, no gas regression tests, no integration tests with Neo Express/private net, and no docs-example compile checks.

## Coverage Matrix To Build

Compiler:

- Syntax: all documented language forms, negative parse tests, stable diagnostics.
- Type checker: every operator, conversion, generic collection case, storage type, native wrapper type, standard callback signature.
- IR/codegen: every AST form to IR, every IR instruction to bytecode, jump/call relayout, stack depth invariants, local/argument slot limits, storage serialization.
- NEF: header, source, tokens, script, checksum, size limits, method token generation.
- Manifest: ABI, safe methods, groups, permissions, trusts, supported standards, extra metadata, standard compatibility validation.
- Optimization: behavior-preserving transformations with golden bytecode and VM execution comparison.
- CLI: build, asm, ast, disasm, diagnostics, output dirs, debug artifacts, templates.

Devpack:

- `neo-lang-framework`: typed modules for Runtime, Storage, Contract, Crypto, Iterator, native contracts, standards, and common Neo value types.
- `neo-lang-testing`: storage simulator, native contract mocks, VM execution harness, gas and coverage reports.
- `neo-lang-template`: NEP-17, NEP-11, oracle consumer, upgradeable contract, witness-verified admin contract, storage patterns.
- `neo-lang-analyzer`: static checks for permissions, unsafe calls, missing witness checks, reentrancy risk, unbounded storage iteration, unsupported standard signatures.
- `neo-lang-docs`: user guide, API reference, examples, cookbook, migration notes.

## Implementation Phases

### Phase 0: Baseline and Gates

- Keep PR #1 separate and merge it first.
- Add CI for `cargo fmt --all -- --check`, `cargo test --workspace --all-targets`, and `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- Add a coverage tracking document generated from official source lists.
- Add docs-example compilation checks for every `.neo` block that claims to compile.

Acceptance:

- CI is required for every PR.
- README examples compile.
- Coverage matrix has an owner, status, and tests for every Neo N3 feature row.

### Phase 1: Artifact Correctness

- Complete manifest builder: attributes for `author`, `email`, `description`, `source`, `supportedStandards`, `permission`, `trust`, `group`, `safe`.
- Add standard compatibility validators for NEP-17 and NEP-11.
- Emit NEF method tokens for external calls where static targets are known.
- Add debug information aligned with NEP-19.
- Add golden NEF/manifest tests and disasm snapshots.

Acceptance:

- Generated manifest passes Neo N3 schema and standard validators.
- NEP-17/NEP-11 compatibility failures are surfaced as compiler diagnostics.
- Golden artifacts are deterministic across runs.

### Phase 2: Neo N3 API Surface

- Design typed devpack modules for Runtime, Storage, Contract, Crypto, Iterator, and native contracts.
- Replace generic `runtime.contractCall` with typed `Contract.call` and explicit call flags while keeping a low-level escape hatch. The NEP-17 template now uses this path for receiver callbacks.
- Add native wrappers for ContractManagement, StdLib, CryptoLib, Ledger, NEO, GAS, Policy, RoleManagement, Oracle.
- Add storage context, read-only context, storage map, find options, iterator next/key/value APIs.

Acceptance:

- Every official interop syscall has either a typed wrapper or an intentional documented exclusion.
- Every native contract method has a typed wrapper, tests, and docs.
- Unsupported APIs fail at compile time with clear diagnostics.

### Phase 3: Language Completeness

- Implement a real package/import resolver with multi-file compilation and artifact packaging.
- Add Neo-specific value types and literals: Hash160, Hash256, ECPoint/PublicKey, Signature, Address, ByteString, nullable values.
- Add contract lifecycle methods: `_deploy`, `_initialize`, `verify`, `destroy`, update callbacks, NEP payment callbacks.
- Decide and implement semantics for `#[pure]`, `#[noreentrant]`, storage/event/call permissions, or remove them from docs.
- Add safe method enforcement and static side-effect analysis.

Acceptance:

- Common production contracts do not need raw syscalls for standard operations.
- Package imports are deterministic, cacheable, and tested.
- Attribute behavior is enforced, not only documented.

### Phase 4: Standards and Templates

- Implement production-grade NEP-17 template and example.
- Implement NEP-11 divisible and non-divisible templates.
- Add NEP-24 royalty support, NEP-26/NEP-27 payment callback helpers, NEP-29 deploy/update callback helpers, NEP-30 verification callback, NEP-31 destroy guidance.
- Add examples for oracle requests, NEO/GAS native calls, policy checks, role management, update/destroy, and cross-contract calls.

Acceptance:

- Each template compiles, passes manifest compatibility checks, and has unit + integration tests.
- Examples are documented and compile in CI.

### Phase 5: Testing and Developer Experience

- Add VM execution tests for compiled scripts.
- Add private-net integration tests using Neo Express or equivalent.
- Add storage/native mocks for fast unit tests.
- Add gas snapshots and regression thresholds.
- Add fuzz/property tests for parser, bytecode encoding, jump relayout, and NEF round trips.
- Add source spans and structured diagnostics.
- Add generated API docs and language reference.

Acceptance:

- Contract authors can compile, test, inspect, and deploy a standard contract with a documented workflow.
- Runtime behavior is tested against NeoVM, not only instruction shape.
- Gas regressions are visible before merge.

## Immediate Next PRs

1. Add runtime-oriented negative callback behavior coverage for NEP-17/NEP-11 receiver contracts.
2. Typed Runtime/Contract/Storage devpack module design and first implementation.
3. Extend typed native wrapper strategy from StdLib/CryptoLib to ContractManagement, Ledger, NEO, GAS, Policy, RoleManagement, and Oracle.
4. Golden NEF/manifest snapshots and debug artifact planning.
5. VM execution harness spike for compiled scripts.

## Source References

- Neo DevPack for .NET components: https://github.com/neo-project/neo-devpack-dotnet
- Neo N3 interop services: https://docs.neo.org/docs/n3/reference/scapi/interop.html
- Neo N3 native contracts: https://docs.neo.org/docs/n3/reference/scapi/framework/native/index.html
- Neo N3 framework services: https://docs.neo.org/docs/n3/reference/scapi/framework/services.html
- NEF and manifest fields: https://docs.neo.org/docs/n3/develop/write/manifest.html
- NEP proposals index: https://github.com/neo-project/proposals
- NEP-17 developer reference: https://docs.neo.org/docs/n3/develop/write/nep17.html
- NEP-11 developer reference: https://docs.neo.org/docs/n3/develop/write/nep11.html
