# Neo DevPack Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a tested `neo-devpack` workspace crate that provides Neo N3 API metadata, manifest helpers, standard validators, templates, and testing utilities for `neo-lang`.

**Architecture:** The new crate is independent from `neo-compiler` and exposes small modules with stable data structures. `neo-compiler` can depend on `neo-devpack` to validate imports and progressively lower framework modules without duplicating catalog data.

**Tech Stack:** Rust 2021, Cargo workspace, serde/serde_json, unit and integration tests.

---

### Task 1: Workspace Crate and Red Tests

**Files:**
- Modify: `Cargo.toml`
- Create: `neo-devpack/Cargo.toml`
- Create: `neo-devpack/src/lib.rs`
- Create: `neo-devpack/tests/devpack_foundation.rs`

- [ ] Write tests importing `neo_devpack::{api, standards, templates, testing}` and asserting the desired public API.
- [ ] Run `cargo test -p neo-devpack`; expected failure: unresolved items.
- [ ] Implement modules until tests pass.
- [ ] Run `cargo test -p neo-devpack`; expected pass.

### Task 2: API Catalog

**Files:**
- Create: `neo-devpack/src/types.rs`
- Create: `neo-devpack/src/api.rs`

- [ ] Add `NeoType`, `ParameterSpec`, `FunctionSpec`, `ModuleSpec`, `NativeContractSpec`, `CallFlags`, and `ApiCatalog`.
- [ ] Include `runtime`, `storage`, `contract`, `crypto`, and `iterator` modules.
- [ ] Include native contracts: ContractManagement, StdLib, CryptoLib, Ledger, NEO, GAS, Policy, RoleManagement, Oracle.
- [ ] Test lookup, native hashes, and important method signatures.

### Task 3: Manifest and Standards

**Files:**
- Create: `neo-devpack/src/manifest.rs`
- Create: `neo-devpack/src/standards.rs`

- [ ] Add serializable manifest structs aligned with Neo N3 field names.
- [ ] Add `ContractShape` and `CompatibilityError`.
- [ ] Implement NEP-17 and NEP-11 validators.
- [ ] Add standard index entries for NEP-17, NEP-11, NEP-24, NEP-26, NEP-27, NEP-29, NEP-30, and NEP-31.

### Task 4: Templates

**Files:**
- Create: `neo-devpack/src/templates.rs`

- [ ] Add `TemplateKind`, `TemplateOptions`, `RenderedTemplate`, and `TemplateFile`.
- [ ] Add hello world, NEP-17 token, NEP-11 NFT, storage map, oracle consumer, and upgradeable admin templates.
- [ ] Test that rendered files have no unresolved placeholders and include expected ABI members.

### Task 5: Testing Utilities

**Files:**
- Create: `neo-devpack/src/testing.rs`

- [ ] Add `StorageFixture`, `NotificationRecorder`, `Notification`, `GasMeter`, and `DevPackTestContext`.
- [ ] Test storage get/put/delete/prefix, notifications, and gas budget errors.

### Task 6: Documentation and Verification

**Files:**
- Create: `neo-devpack/README.md`
- Modify: `README.md`

- [ ] Document crate purpose, modules, and first-phase limitations.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo test --workspace --all-targets`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.

### Task 7: First Compiler Import Integration

**Files:**
- Create: `neo-compiler/src/devpack.rs`
- Modify: `neo-compiler/Cargo.toml`
- Modify: `neo-compiler/src/main.rs`
- Modify: `neo-compiler/src/typecheck/mod.rs`
- Modify: `neo-compiler/src/ir/lower/mod.rs`
- Modify: `neo-compiler/src/ir/lower/builder.rs`
- Modify: `neo-compiler/src/codegen/mod.rs`
- Modify: `neo-compiler/src/codegen/function.rs`
- Modify: `neo-compiler/src/codegen/expr/mod.rs`
- Modify: `neo-compiler/src/codegen/expr/builtin_call.rs`
- Modify: compiler tests and devpack README

- [x] Add red tests for `import rt from "neo-devpack/runtime"; rt.getNetwork()`.
- [x] Validate unknown `neo-devpack/<module>` imports during type checking.
- [x] Route runtime import aliases through existing runtime syscall typecheck, IR lowering, and legacy codegen paths.
- [x] Document supported compiler import syntax and current non-runtime limitations.

### Task 8: Framework Syscall Import Lowering

**Files:**
- Modify: `neo-compiler/src/devpack.rs`
- Modify: `neo-compiler/src/typecheck/mod.rs`
- Modify: `neo-compiler/src/codegen/expr/mod.rs`
- Create: `neo-compiler/src/codegen/expr/devpack_call.rs`
- Modify: `neo-compiler/src/codegen/expr/runtime_call.rs`
- Modify: `neo-compiler/src/target/syscall.rs`
- Modify: compiler tests and devpack README

- [x] Add red tests for storage, contract, and crypto import aliases.
- [x] Map devpack storage/contract/crypto methods to existing NeoVM syscall metadata.
- [x] Type check devpack syscall arguments and return types through the shared syscall table.
- [x] Emit syscalls from devpack import aliases and preserve a disposable value for void syscall statement expressions.
- [x] Correct `System.Contract.Call` and `System.Storage.AsReadOnly` syscall metadata used by the devpack mapping.

### Task 9: Iterator Syscall Import Lowering

**Files:**
- Modify: `neo-compiler/src/target/syscall.rs`
- Modify: `neo-compiler/src/devpack.rs`
- Modify: `neo-compiler/src/typecheck/mod.rs`
- Modify: compiler tests and devpack README

- [x] Add red tests for `storage.localFind(...)` plus `iterator.next/value(...)` imports.
- [x] Add `System.Iterator.Next` and `System.Iterator.Value` syscall metadata.
- [x] Map `neo-devpack/iterator` helpers to the new syscall metadata.
- [x] Allow `any` iterator handles to satisfy syscall `InteropInterface` parameters.
- [x] Document full direct framework syscall import coverage.

### Task 10: Typed Native Contract Bindings

**Files:**
- Create: `neo-devpack/src/native.rs`
- Modify: `neo-devpack/src/lib.rs`
- Modify: `neo-devpack/tests/devpack_foundation.rs`
- Modify: `neo-devpack/README.md`

- [x] Add red tests for NEO/GAS native contract invocation metadata.
- [x] Provide `NativeContract` enum for all catalog native contracts.
- [x] Provide `NativeValue` typed arguments and hash160 validation.
- [x] Validate native method arity and argument Neo types against the catalog.
- [x] Return `NativeInvocation` metadata suitable for future compiler/native lowering.
