//! Build the target file (NEF + manifest) for a source file.

use std::collections::HashMap;
use std::fs;

use crate::codegen::field::field_getter_specs;
use crate::codegen::{Codegen, CompiledSourceFile};
use crate::syntax::ast::*;
use crate::syntax::parser;
use crate::target::nef::*;
use crate::MAX_SOURCE_FILE_BYTES;

pub(crate) fn run_build(source: &std::path::Path) -> Result<(), String> {
    let meta = fs::metadata(source).map_err(|e| format!("{}: {e}", source.display()))?;
    if meta.len() > MAX_SOURCE_FILE_BYTES {
        return Err(format!(
            "{}: file too large (max {} KiB)",
            source.display(),
            MAX_SOURCE_FILE_BYTES / 1024
        ));
    }
    let src = fs::read_to_string(source)
        .map_err(|e| format!("build: read source file {} error: {e}", source.display()))?;
    let ast = parser::parse_source_file(&src)
        .map_err(|e| format!("build: parse error at line {}: {}", e.line, e.message))?;
    let compiled = Codegen::new()
        .codegen_source_file(&ast)
        .map_err(|e| e.to_string())?;

    let contract = ast
        .contract
        .as_ref()
        .ok_or_else(|| "build: a `contract { ... }` block is required".to_string())?;

    let out_dir = source
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "build: cannot determine output directory".to_string())?;
    let script = compiled.flatten_to_bytes();
    let compiler = format!("neo-compiler {}", env!("CARGO_PKG_VERSION"));
    let nef = Nef3 {
        magic: Nef3::MAGIC,
        compiler: {
            let mut compiler_fixed = [0u8; 64];
            let bytes = compiler.as_bytes();
            let n = bytes.len().min(64);
            compiler_fixed[..n].copy_from_slice(&bytes[..n]);
            compiler_fixed
        },
        source: Vec::new(),
        tokens: compiled.method_tokens.clone(),
        script,
    };
    let nef_bytes = nef.to_bytes();

    let manifest =
        build_manifest(&ast, &compiled).map_err(|e| format!("build: manifest error: {e}"))?;
    let manifest = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("build: manifest serialization error: {e}"))?;

    let nef_path = out_dir.join(format!("{}.nef", contract.name));
    let manifest_path = out_dir.join(format!("{}.manifest.json", contract.name));
    fs::write(&nef_path, nef_bytes)
        .map_err(|e| format!("build: write NEF file {} error: {e}", nef_path.display()))?;
    fs::write(&manifest_path, manifest).map_err(|e| {
        format!(
            "build: write manifest file {} error: {e}",
            manifest_path.display()
        )
    })?;

    Ok(())
}

fn is_predefined_fn(name: &str) -> bool {
    name == "_deploy" || name == "_initialize"
}

fn build_extra_metadata(ast: &SourceFile) -> Result<HashMap<String, String>, String> {
    let Some(attrs) = ast.contract.as_ref() else {
        return Ok(HashMap::new());
    };
    let mut extra = HashMap::new();
    for attr in &attrs.attributes {
        let values = attr.args.join(",");
        extra.insert(attr.name.clone(), values);
    }
    Ok(extra)
}

fn build_manifest(ast: &SourceFile, compiled: &CompiledSourceFile) -> Result<Manifest, String> {
    let contract = ast
        .contract
        .as_ref()
        .ok_or_else(|| "missing contract".to_string())?;

    // Compute script offsets for all routines in the flattened script.
    let mut off: u32 = 0;
    let mut contract_method_offset: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    for f in &compiled.package_functions {
        off = off
            .checked_add(
                f.instructions
                    .iter()
                    .map(|instruction| instruction.encoded_len() as u32)
                    .sum::<u32>(),
            )
            .ok_or_else(|| "script too large".to_string())?;
    }
    for f in &compiled.struct_methods {
        off = off
            .checked_add(
                f.instructions
                    .iter()
                    .map(|instruction| instruction.encoded_len() as u32)
                    .sum::<u32>(),
            )
            .ok_or_else(|| "script too large".to_string())?;
    }
    for f in &compiled.contract_methods {
        contract_method_offset.insert(f.name.clone(), off);
        off = off
            .checked_add(
                f.instructions
                    .iter()
                    .map(|instruction| instruction.encoded_len() as u32)
                    .sum::<u32>(),
            )
            .ok_or_else(|| "script too large".to_string())?;
    }

    let mut methods = Vec::new();
    let mut events = Vec::new();
    let getter_specs = field_getter_specs(contract);
    for spec in &getter_specs {
        let offset = *contract_method_offset.get(&spec.func.name).ok_or_else(|| {
            format!(
                "no compiled offset for property getter `{}`",
                spec.func.name
            )
        })?;
        methods.push(ContractMethod {
            name: spec.func.name.clone(),
            parameters: vec![],
            return_type: manifest_type_name(&spec.func.return_ty),
            offset,
            safe: spec.is_pure,
        });
    }
    for member in &contract.members {
        match member {
            ContractMember::Function(func) => {
                if func.name.starts_with('_') && !is_predefined_fn(&func.name) {
                    continue;
                }

                let offset = *contract_method_offset.get(&func.name).ok_or_else(|| {
                    format!("no compiled offset for contract method `{}`", func.name)
                })?;
                methods.push(ContractMethod {
                    name: func.name.clone(),
                    parameters: func
                        .params
                        .iter()
                        .map(|param| ContractParameter {
                            name: param.name.clone(),
                            ty: manifest_type_name(&param.ty),
                        })
                        .collect(),
                    return_type: manifest_type_name(&func.return_ty),
                    offset,
                    safe: func.attributes.iter().any(|attr| attr.name == "safe"),
                });
            }
            ContractMember::Event(event) => {
                events.push(ContractEvent {
                    name: event.name.clone(),
                    parameters: event
                        .params
                        .iter()
                        .map(|param| ContractParameter {
                            name: param.name.clone(),
                            ty: manifest_type_name(&param.ty),
                        })
                        .collect(),
                });
            }
            _ => {}
        }
    }

    Ok(Manifest {
        name: contract.name.clone(),
        groups: vec![],
        supported_standards: vec![],
        abi: ContractAbi { methods, events },
        permissions: vec![ContractPermission {
            contract: WILDCARD.into(),
            methods: PermissionRule::All,
        }], // TODO: get permissions of the contract
        trusts: PermissionRule::All, // TODO: get trusts of the contract
        extra: build_extra_metadata(ast)?,
    })
}

fn manifest_type_name(ty: &Type) -> String {
    match ty {
        Type::Void => "Void",
        Type::Bool => "Boolean",
        Type::Int => "Integer",
        Type::String => "String",
        Type::Hash160 => "Hash160",
        Type::Hash256 => "Hash256",
        Type::Buffer => "ByteArray",
        Type::Any => "Any",
        Type::Named(_) => "Any",
        Type::Array(_) => "Array",
        Type::Map { .. } => "Map",
    }
    .to_string()
}
