# Neo DevPack Foundation Design

Date: 2026-05-05

## Scope

This first devpack increment creates a production-oriented foundation for `neo-lang`, modeled after the major responsibilities of `neo-project/neo-devpack-dotnet`: framework API metadata, standards, templates, analyzers, and testing utilities. It does not attempt to finish the compiler import resolver or execute NeoVM bytecode yet; those become follow-up layers once the foundation is in place.

## Architecture

Add a new workspace crate, `neo-devpack`, with focused modules:

- `api`: typed catalog of Neo N3 framework services, interop syscalls, and native contracts.
- `manifest`: serializable manifest model and small builder for devpack-generated artifacts.
- `standards`: NEP standard metadata and compatibility checks for method/event shapes.
- `templates`: built-in `.neo` templates for common contracts.
- `testing`: in-memory contract storage, notification capture, and gas accounting primitives for fast tests.

The crate stays independent from `neo-compiler` so the devpack has no compiler dependency. The compiler may depend on `neo-devpack` to consume the shared catalog for import validation and lowering metadata.

## Devpack API Model

The catalog exposes public functions to inspect supported modules and native contracts:

- `ApiCatalog::neo_n3()` returns the complete foundation catalog.
- `catalog.module("runtime")`, `catalog.module("storage")`, and `catalog.native_contract("NEO")` provide typed metadata.
- Entries include source name, VM/syscall/native target, parameters, return type, and required call flags.

## Standards

The first standard validators cover NEP-17 and NEP-11 ABI shape. They validate names, parameter types, return types, events, and manifest `supportedstandards`. NEP-24, NEP-26, NEP-27, NEP-29, NEP-30, and NEP-31 are included in the standard index for discovery and later deeper validators.

## Templates

The first template set provides:

- `hello-world`
- `nep17-token`
- `nep11-nft`
- `storage-map`
- `oracle-consumer`
- `upgradeable-admin`

Templates render from `TemplateOptions` and return named files. They are intended as source material for future CLI/template commands and docs.

## Testing Utilities

The devpack testing module provides deterministic primitives:

- `StorageFixture` for per-contract key-value storage.
- `NotificationRecorder` for emitted events.
- `GasMeter` for simple gas budgets and accounting.

These do not replace a NeoVM integration harness; they are the fast unit-test layer that later compiler/runtime tests can reuse.

## Compiler Integration

The first compiler integration layer uses `neo-devpack::api::ApiCatalog` to validate imports in these forms:

- `import runtime from "neo-devpack";`
- `import rt from "neo-devpack/runtime";`

Runtime module aliases lower to the existing `System.Runtime.*` syscall path. Storage, contract, and crypto module imports lower through a shared devpack-to-syscall mapping. Unknown devpack modules are rejected during type checking. Iterator helpers currently remain catalog-only until iterator syscall lowering is implemented.

## Testing Strategy

Use TDD for the first crate:

- API catalog tests prove framework/native coverage and important method metadata.
- Standards tests prove NEP-17 and NEP-11 validators catch missing ABI.
- Template tests prove every template renders without unresolved placeholders and includes standard-required members.
- Testing fixture tests prove storage, notifications, and gas accounting semantics.
- Workspace tests, fmt, clippy, and docs build must pass.
