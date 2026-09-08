use std::collections::HashMap;

use crate::stdlib::StructScope;
use crate::syntax::ast::{ContractField, ContractMember, FunctionDecl, SourceFile};

use super::context::{FnType, TypeCheckContext};
use super::error::{err, err_at, TypeError};
use super::map_key::check_map_key_rules_in_type;

impl SourceFile {
    pub(crate) fn type_check(&self) -> Result<(), TypeError> {
        let mut seen_user_structs = HashMap::new();
        for (index, struct_decl) in self.structs.iter().enumerate() {
            if seen_user_structs
                .insert(struct_decl.name.clone(), ())
                .is_some()
            {
                let span = self.struct_spans.get(index).copied().unwrap_or_default();
                return Err(err_at(
                    span,
                    format!("duplicate struct `{}`", struct_decl.name),
                ));
            }
        }

        let struct_scope =
            StructScope::from_source_structs(&self.structs).map_err(|e| err(e.to_string()))?;
        let structs = struct_scope.index();
        let mut package_fns: HashMap<String, &FunctionDecl> = HashMap::new();
        for (index, func) in self.functions.iter().enumerate() {
            if package_fns.insert(func.name.clone(), func).is_some() {
                let span = self.function_spans.get(index).copied().unwrap_or_default();
                return Err(err_at(
                    span,
                    format!(
                        "duplicate top-level function `{}` in the same file",
                        func.name
                    ),
                ));
            }
        }

        let mut events: HashMap<String, &crate::syntax::ast::EventDecl> = HashMap::new();
        let contract_field_storage: Vec<ContractField> = self
            .contract
            .as_ref()
            .map(|contract_decl| {
                for member in &contract_decl.members {
                    if let ContractMember::Event(event) = member {
                        events.insert(event.name.clone(), event);
                    }
                }
                contract_decl
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        ContractMember::Field(field) => Some(field.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let contract_member_spans = &self.contract_member_spans;
        for (field_index, field) in contract_field_storage.iter().enumerate() {
            if field.ty.is_array() {
                let span = contract_member_spans
                    .get(field_index)
                    .copied()
                    .unwrap_or_default();
                return Err(err_at(span, "contract cannot have array fields"));
            }
        }

        let contract_fields = contract_field_storage.as_slice();
        let contract_fns: HashMap<String, &FunctionDecl> = self
            .contract
            .as_ref()
            .map(|contract_decl| {
                contract_decl
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        ContractMember::Function(func) => Some((func.name.clone(), func)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ctx = TypeCheckContext {
            structs: &structs,
            package_fns: &package_fns,
            events: &events,
            contract_fields,
            contract_fns: &contract_fns,
        };

        self.check_source_file_map_types()?;

        for (index, func) in self.functions.iter().enumerate() {
            let span = self.function_spans.get(index).copied().unwrap_or_default();
            ctx.check_function(func, FnType::Package, span)?;
        }

        for struct_decl in &self.structs {
            for (method_index, method) in struct_decl.methods.iter().enumerate() {
                let span = struct_decl
                    .method_spans
                    .get(method_index)
                    .copied()
                    .unwrap_or_default();
                ctx.check_function(
                    method,
                    FnType::StructMethod {
                        struct_name: struct_decl.name.clone(),
                    },
                    span,
                )?;
            }
        }

        if let Some(contract_decl) = &self.contract {
            let contract_name = contract_decl.name.clone();
            for (member_index, member) in contract_decl.members.iter().enumerate() {
                if let ContractMember::Function(func) = member {
                    let span = self
                        .contract_member_spans
                        .get(member_index)
                        .copied()
                        .unwrap_or_default();
                    ctx.check_function(
                        func,
                        FnType::ContractMethod {
                            contract_name: contract_name.clone(),
                        },
                        span,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn check_source_file_map_types(&self) -> Result<(), TypeError> {
        for (index, func) in self.functions.iter().enumerate() {
            let span = self.function_spans.get(index).copied().unwrap_or_default();
            check_map_key_rules_in_type(&func.return_ty, span)?;
            for param in &func.params {
                check_map_key_rules_in_type(&param.ty, span)?;
            }
        }
        for struct_decl in &self.structs {
            for (field_index, field) in struct_decl.fields.iter().enumerate() {
                let span = struct_decl
                    .field_spans
                    .get(field_index)
                    .copied()
                    .unwrap_or_default();
                check_map_key_rules_in_type(&field.ty, span)?;
            }
            for (method_index, method) in struct_decl.methods.iter().enumerate() {
                let span = struct_decl
                    .method_spans
                    .get(method_index)
                    .copied()
                    .unwrap_or_default();
                check_map_key_rules_in_type(&method.return_ty, span)?;
                for param in &method.params {
                    check_map_key_rules_in_type(&param.ty, span)?;
                }
            }
        }
        if let Some(contract_decl) = &self.contract {
            for (member_index, member) in contract_decl.members.iter().enumerate() {
                let span = self
                    .contract_member_spans
                    .get(member_index)
                    .copied()
                    .unwrap_or_default();
                match member {
                    ContractMember::ConstProp(prop) => check_map_key_rules_in_type(&prop.ty, span)?,
                    ContractMember::Field(field) => check_map_key_rules_in_type(&field.ty, span)?,
                    ContractMember::Event(event) => {
                        for param in &event.params {
                            check_map_key_rules_in_type(&param.ty, span)?;
                        }
                    }
                    ContractMember::Function(func) => {
                        check_map_key_rules_in_type(&func.return_ty, span)?;
                        for param in &func.params {
                            check_map_key_rules_in_type(&param.ty, span)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
