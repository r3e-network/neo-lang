use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::types::{FunctionSpec, ParameterSpec};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractManifest {
    pub name: String,
    pub groups: Vec<ContractGroup>,
    pub supportedstandards: Vec<String>,
    pub abi: ContractAbi,
    pub permissions: Vec<ContractPermission>,
    pub trusts: PermissionRule,
    pub extra: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractGroup {
    pub pubkey: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractAbi {
    pub methods: Vec<ManifestMethod>,
    pub events: Vec<ManifestEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestMethod {
    pub name: String,
    pub parameters: Vec<ManifestParameter>,
    #[serde(rename = "returntype")]
    pub return_type: String,
    pub offset: u32,
    pub safe: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEvent {
    pub name: String,
    pub parameters: Vec<ManifestParameter>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractPermission {
    pub contract: String,
    pub methods: PermissionRule,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionRule {
    Any(String),
    List(Vec<String>),
}

impl PermissionRule {
    pub fn any() -> Self {
        Self::Any("*".into())
    }
}

#[derive(Clone, Debug)]
pub struct ManifestBuilder {
    name: String,
    supported_standards: Vec<String>,
    methods: Vec<ManifestMethod>,
    events: Vec<ManifestEvent>,
    permissions: Vec<ContractPermission>,
    trusts: PermissionRule,
    extra: BTreeMap<String, String>,
}

impl ManifestBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            supported_standards: Vec::new(),
            methods: Vec::new(),
            events: Vec::new(),
            permissions: vec![ContractPermission {
                contract: "*".into(),
                methods: PermissionRule::any(),
            }],
            trusts: PermissionRule::any(),
            extra: BTreeMap::new(),
        }
    }

    pub fn supported_standard(mut self, name: impl Into<String>) -> Self {
        self.supported_standards.push(name.into());
        self
    }

    pub fn method(mut self, spec: FunctionSpec) -> Self {
        self.methods.push(ManifestMethod {
            name: spec.name,
            parameters: manifest_params(spec.parameters),
            return_type: spec.return_type.manifest_name().into(),
            offset: 0,
            safe: spec.safe,
        });
        self
    }

    pub fn event(mut self, spec: FunctionSpec) -> Self {
        self.events.push(ManifestEvent {
            name: spec.name,
            parameters: manifest_params(spec.parameters),
        });
        self
    }

    pub fn extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> ContractManifest {
        ContractManifest {
            name: self.name,
            groups: Vec::new(),
            supportedstandards: self.supported_standards,
            abi: ContractAbi {
                methods: self.methods,
                events: self.events,
            },
            permissions: self.permissions,
            trusts: self.trusts,
            extra: self.extra,
        }
    }
}

fn manifest_params(params: Vec<ParameterSpec>) -> Vec<ManifestParameter> {
    params
        .into_iter()
        .map(|param| ManifestParameter {
            name: param.name,
            ty: param.ty.manifest_name().into(),
        })
        .collect()
}
