//! Build the target file (NEF + manifest) for a source file.

use std::collections::HashMap;
use std::fs;

use crate::codegen::{Codegen, CompiledSourceFile};
use crate::syntax::ast::*;
use crate::syntax::parser;
use crate::target::nef::*;
use crate::MAX_SOURCE_FILE_BYTES;
use neo_devpack::standards::{validate_standard, ContractShape, NepStandard};
use neo_devpack::types::{FunctionSpec, NeoType, ParameterSpec};

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
    let nef = Nef3::new(script, &compiler);
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

#[derive(Default)]
struct ManifestAttributes {
    groups: Vec<ContractGroup>,
    supported_standards: Vec<String>,
    permissions: Option<Vec<ContractPermission>>,
    trusts: Option<PermissionRule>,
    extra: HashMap<String, String>,
}

fn parse_manifest_attributes(contract: &ContractDecl) -> Result<ManifestAttributes, String> {
    let mut parsed = ManifestAttributes::default();
    for attr in &contract.attributes {
        match attr.name.as_str() {
            "author" | "auther" => {
                parsed.extra.insert(
                    "author".into(),
                    single_attr_arg(attr, "author")?.to_string(),
                );
            }
            "email" | "description" | "source" | "version" => {
                parsed.extra.insert(
                    attr.name.clone(),
                    single_attr_arg(attr, &attr.name)?.to_string(),
                );
            }
            "supportedStandards" | "supportedstandards" => {
                if attr.args.is_empty() {
                    return Err(
                        "attribute `supportedStandards` requires at least one standard".into(),
                    );
                }
                parsed.supported_standards.extend(attr.args.iter().cloned());
            }
            "permission" => {
                if attr.args.is_empty() {
                    return Err("attribute `permission` requires a contract target".into());
                }
                let methods = permission_rule_from_args(&attr.args[1..], "permission methods")?;
                parsed
                    .permissions
                    .get_or_insert_with(Vec::new)
                    .push(ContractPermission {
                        contract: attr.args[0].clone(),
                        methods,
                    });
            }
            "trust" | "trusts" => {
                parsed.trusts = Some(permission_rule_from_args(&attr.args, "trusts")?);
            }
            "group" => {
                if attr.args.len() != 2 {
                    return Err(
                        "attribute `group` requires exactly public key and signature strings"
                            .into(),
                    );
                }
                parsed.groups.push(ContractGroup {
                    pubkey: attr.args[0].clone(),
                    signature: attr.args[1].clone(),
                });
            }
            _ => {
                parsed.extra.insert(attr.name.clone(), attr.args.join(","));
            }
        }
    }
    Ok(parsed)
}

fn single_attr_arg<'a>(attr: &'a Attribute, name: &str) -> Result<&'a str, String> {
    if attr.args.len() != 1 {
        return Err(format!(
            "attribute `{name}` requires exactly one string argument"
        ));
    }
    Ok(&attr.args[0])
}

fn permission_rule_from_args(args: &[String], label: &str) -> Result<PermissionRule, String> {
    if args.is_empty() {
        return Ok(PermissionRule::All);
    }
    if args.iter().any(|arg| arg == WILDCARD) {
        if args.len() == 1 {
            return Ok(PermissionRule::All);
        }
        return Err(format!(
            "attribute `{label}` cannot combine `*` with explicit values"
        ));
    }
    Ok(PermissionRule::Allows(args.to_vec()))
}

fn nep_standard_from_manifest_name(name: &str) -> Result<NepStandard, String> {
    match name {
        "NEP-11" => Ok(NepStandard::Nep11),
        "NEP-17" => Ok(NepStandard::Nep17),
        "NEP-24" => Ok(NepStandard::Nep24),
        "NEP-26" => Ok(NepStandard::Nep26),
        "NEP-27" => Ok(NepStandard::Nep27),
        "NEP-29" => Ok(NepStandard::Nep29),
        "NEP-30" => Ok(NepStandard::Nep30),
        "NEP-31" => Ok(NepStandard::Nep31),
        _ => Err(format!(
            "unsupported standard `{name}` in `supportedStandards`"
        )),
    }
}

fn validate_supported_standards(
    contract: &ContractDecl,
    standards: &[String],
) -> Result<(), String> {
    if standards.is_empty() {
        return Ok(());
    }

    let mut shape = ContractShape::new(contract.name.clone());
    let parsed_standards = standards
        .iter()
        .map(|name| nep_standard_from_manifest_name(name))
        .collect::<Result<Vec<_>, _>>()?;
    for standard in &parsed_standards {
        shape.supported_standards.push(*standard);
    }

    for member in &contract.members {
        match member {
            ContractMember::Function(method) => {
                shape.methods.push(function_spec_from_decl(method));
            }
            ContractMember::Event(event) => {
                shape.events.push(event_spec_from_decl(event));
            }
            _ => {}
        }
    }

    let mut messages = Vec::new();
    for standard in parsed_standards {
        if let Err(errors) = validate_standard(standard, &shape) {
            messages.extend(
                errors
                    .into_iter()
                    .map(|error| format!("{}: {error}", standard.manifest_name())),
            );
        }
    }

    if messages.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "supported standard compatibility failed: {}",
            messages.join("; ")
        ))
    }
}

fn function_spec_from_decl(method: &FunctionDecl) -> FunctionSpec {
    FunctionSpec::new(
        method.name.clone(),
        method
            .params
            .iter()
            .map(|param| ParameterSpec::new(param.name.clone(), neo_type_for_manifest(&param.ty)))
            .collect(),
        neo_type_for_manifest(&method.return_ty),
    )
}

fn event_spec_from_decl(event: &EventDecl) -> FunctionSpec {
    FunctionSpec::new(
        event.name.clone(),
        event
            .params
            .iter()
            .map(|param| ParameterSpec::new(param.name.clone(), neo_type_for_manifest(&param.ty)))
            .collect(),
        NeoType::Void,
    )
}

fn build_manifest(ast: &SourceFile, compiled: &CompiledSourceFile) -> Result<Manifest, String> {
    let contract = ast
        .contract
        .as_ref()
        .ok_or_else(|| "missing contract".to_string())?;
    let manifest_attrs = parse_manifest_attributes(contract)?;
    validate_supported_standards(contract, &manifest_attrs.supported_standards)?;

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

    if !methods.iter().any(|method| method.name == "_deploy") {
        if let Some(offset) = contract_method_offset.get("_deploy").copied() {
            methods.push(ContractMethod {
                name: "_deploy".into(),
                parameters: vec![
                    ContractParameter {
                        name: "data".into(),
                        ty: "Any".into(),
                    },
                    ContractParameter {
                        name: "update".into(),
                        ty: "Boolean".into(),
                    },
                ],
                return_type: "Void".into(),
                offset,
                safe: false,
            });
        }
    }

    Ok(Manifest {
        name: contract.name.clone(),
        groups: manifest_attrs.groups,
        supported_standards: manifest_attrs.supported_standards,
        abi: ContractAbi { methods, events },
        permissions: manifest_attrs.permissions.unwrap_or_else(|| {
            vec![ContractPermission {
                contract: WILDCARD.into(),
                methods: PermissionRule::All,
            }]
        }),
        trusts: manifest_attrs.trusts.unwrap_or(PermissionRule::All),
        extra: manifest_attrs.extra,
    })
}

fn manifest_type_name(ty: &Type) -> String {
    neo_type_for_manifest(ty).manifest_name().to_string()
}

fn neo_type_for_manifest(ty: &Type) -> NeoType {
    match ty {
        Type::Void => NeoType::Void,
        Type::Bool => NeoType::Boolean,
        Type::Int => NeoType::Integer,
        Type::String => NeoType::String,
        Type::Hash160 => NeoType::Hash160,
        Type::Hash256 => NeoType::Hash256,
        Type::Buffer => NeoType::ByteArray,
        Type::Any => NeoType::Any,
        Type::Named(name) if name == "iterator" => NeoType::Iterator,
        Type::Named(_) => NeoType::Any,
        Type::Array(_) => NeoType::Array,
        Type::Map { .. } => NeoType::Map,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parser::parse_source_file;
    use neo_devpack::templates::{render_template, TemplateKind, TemplateOptions};

    #[test]
    fn manifest_includes_synthetic_deploy_for_contract_storage_initializers() {
        let src = r#"
            contract X {
                int x = 7;

                #[safe]
                int get() {
                    return self.x;
                }
            }
        "#;
        let ast = parse_source_file(src).expect("parse");
        let compiled = Codegen::new().codegen_source_file(&ast).expect("codegen");
        let manifest = build_manifest(&ast, &compiled).expect("manifest");
        let deploy = manifest
            .abi
            .methods
            .iter()
            .find(|method| method.name == "_deploy")
            .expect("synthetic _deploy ABI method");
        assert_eq!(deploy.return_type, "Void");
        assert_eq!(deploy.parameters.len(), 2);
        assert_eq!(deploy.parameters[0].name, "data");
        assert_eq!(deploy.parameters[0].ty, "Any");
        assert_eq!(deploy.parameters[1].name, "update");
        assert_eq!(deploy.parameters[1].ty, "Boolean");
    }

    #[test]
    fn manifest_maps_contract_attributes_to_neo_n3_fields() {
        let src = r#"
            #[author("core-dev")]
            #[email("core@example.com")]
            #[description("Production token")]
            #[supportedStandards("NEP-24", "NEP-29")]
            #[permission("0x1234567890abcdef1234567890abcdef12345678", "transfer", "balanceOf")]
            #[trust("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd")]
            #[group("03b209fd4f04a9d3e8e7b4ad5c5f2d5148c10c7ad2e9eac19b7e8acb4d2f0a5f5f", "MEUCIQD")]
            contract Token {
                #[safe]
                string symbol() {
                    return "TOK";
                }
            }
        "#;
        let ast = parse_source_file(src).expect("parse");
        let compiled = Codegen::new().codegen_source_file(&ast).expect("codegen");
        let manifest = build_manifest(&ast, &compiled).expect("manifest");

        assert_eq!(manifest.supported_standards, vec!["NEP-24", "NEP-29"]);
        assert_eq!(
            manifest.extra.get("author").map(String::as_str),
            Some("core-dev")
        );
        assert_eq!(
            manifest.extra.get("email").map(String::as_str),
            Some("core@example.com")
        );
        assert_eq!(
            manifest.extra.get("description").map(String::as_str),
            Some("Production token")
        );

        assert_eq!(manifest.permissions.len(), 1);
        assert_eq!(
            manifest.permissions[0].contract,
            "0x1234567890abcdef1234567890abcdef12345678"
        );
        match &manifest.permissions[0].methods {
            PermissionRule::Allows(methods) => {
                assert_eq!(
                    methods,
                    &vec!["transfer".to_string(), "balanceOf".to_string()]
                );
            }
            PermissionRule::All => panic!("expected method-level permission rule"),
        }

        match &manifest.trusts {
            PermissionRule::Allows(trusts) => {
                assert_eq!(
                    trusts,
                    &vec!["0xabcdefabcdefabcdefabcdefabcdefabcdefabcd".to_string()]
                );
            }
            PermissionRule::All => panic!("expected explicit trusts"),
        }

        assert_eq!(manifest.groups.len(), 1);
        assert_eq!(
            manifest.groups[0].pubkey,
            "03b209fd4f04a9d3e8e7b4ad5c5f2d5148c10c7ad2e9eac19b7e8acb4d2f0a5f5f"
        );
        assert_eq!(manifest.groups[0].signature, "MEUCIQD");
    }

    #[test]
    fn manifest_rejects_incomplete_supported_standard_abi() {
        let src = r#"
            #[supportedStandards("NEP-17")]
            contract Token {
                #[safe]
                string symbol() {
                    return "TOK";
                }
            }
        "#;
        let ast = parse_source_file(src).expect("parse");
        let compiled = Codegen::new().codegen_source_file(&ast).expect("codegen");
        let err = match build_manifest(&ast, &compiled) {
            Ok(_) => panic!("expected NEP-17 validation error"),
            Err(err) => err,
        };

        assert!(err.contains("NEP-17"));
        assert!(err.contains("missing method `totalSupply`"));
        assert!(err.contains("missing event `Transfer`"));
    }

    #[test]
    fn manifest_accepts_complete_nep17_shape() {
        let src = r#"
            #[supportedStandards("NEP-17")]
            contract Token {
                event Transfer(hash160 source, hash160 dest, int amount);

                #[safe]
                int totalSupply() {
                    return 0;
                }

                #[safe]
                string symbol() {
                    return "TOK";
                }

                #[safe]
                int decimals() {
                    return 8;
                }

                #[safe]
                int balanceOf(hash160 account) {
                    return 0;
                }

                bool transfer(hash160 source, hash160 dest, int amount, any data) {
                    return true;
                }
            }
        "#;
        let ast = parse_source_file(src).expect("parse");
        let compiled = Codegen::new().codegen_source_file(&ast).expect("codegen");
        let manifest = build_manifest(&ast, &compiled).expect("manifest");

        assert_eq!(manifest.supported_standards, vec!["NEP-17"]);
    }

    #[test]
    fn devpack_templates_compile_to_manifest() {
        let kinds = [
            TemplateKind::HelloWorld,
            TemplateKind::Nep17Token,
            TemplateKind::Nep11Nft,
            TemplateKind::StorageMap,
            TemplateKind::OracleConsumer,
            TemplateKind::UpgradeableAdmin,
        ];

        for kind in kinds {
            let rendered =
                render_template(kind, &TemplateOptions::new("Sample")).expect("template");
            let source = rendered
                .files
                .iter()
                .find(|file| file.path.ends_with(".neo"))
                .expect("template source file");
            let ast = parse_source_file(&source.contents)
                .unwrap_or_else(|err| panic!("{kind:?} failed to parse: {err:?}"));
            let compiled = Codegen::new()
                .codegen_source_file(&ast)
                .unwrap_or_else(|err| panic!("{kind:?} failed codegen: {err}"));
            build_manifest(&ast, &compiled)
                .unwrap_or_else(|err| panic!("{kind:?} failed manifest generation: {err}"));
        }
    }
}
