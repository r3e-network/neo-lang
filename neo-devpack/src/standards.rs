use std::fmt;

use crate::types::{FunctionSpec, NeoType, ParameterSpec};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NepStandard {
    Nep11,
    Nep17,
    Nep24,
    Nep26,
    Nep27,
    Nep29,
    Nep30,
    Nep31,
}

impl NepStandard {
    pub fn manifest_name(self) -> &'static str {
        match self {
            NepStandard::Nep11 => "NEP-11",
            NepStandard::Nep17 => "NEP-17",
            NepStandard::Nep24 => "NEP-24",
            NepStandard::Nep26 => "NEP-26",
            NepStandard::Nep27 => "NEP-27",
            NepStandard::Nep29 => "NEP-29",
            NepStandard::Nep30 => "NEP-30",
            NepStandard::Nep31 => "NEP-31",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandardInfo {
    pub standard: NepStandard,
    pub title: &'static str,
    pub status: &'static str,
}

pub fn standard_index() -> Vec<StandardInfo> {
    vec![
        StandardInfo {
            standard: NepStandard::Nep11,
            title: "Non-fungible Token Standard",
            status: "Final",
        },
        StandardInfo {
            standard: NepStandard::Nep17,
            title: "Token Standard",
            status: "Final",
        },
        StandardInfo {
            standard: NepStandard::Nep24,
            title: "NFT Royalty Standard",
            status: "Final",
        },
        StandardInfo {
            standard: NepStandard::Nep26,
            title: "Smart contract transfer callback for non-fungible tokens",
            status: "Final",
        },
        StandardInfo {
            standard: NepStandard::Nep27,
            title: "Smart contract transfer callback for fungible tokens",
            status: "Final",
        },
        StandardInfo {
            standard: NepStandard::Nep29,
            title: "Contract deployment/update callback function",
            status: "Accepted",
        },
        StandardInfo {
            standard: NepStandard::Nep30,
            title: "Contract witness verification callback",
            status: "Accepted",
        },
        StandardInfo {
            standard: NepStandard::Nep31,
            title: "Contract Destroy Guideline",
            status: "Accepted",
        },
    ]
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractShape {
    pub name: String,
    pub supported_standards: Vec<NepStandard>,
    pub methods: Vec<FunctionSpec>,
    pub events: Vec<FunctionSpec>,
}

impl ContractShape {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            supported_standards: Vec::new(),
            methods: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn supported_standard(mut self, standard: NepStandard) -> Self {
        self.supported_standards.push(standard);
        self
    }

    pub fn method(mut self, method: FunctionSpec) -> Self {
        self.methods.push(method);
        self
    }

    pub fn event(mut self, event: FunctionSpec) -> Self {
        self.events.push(event);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompatibilityError {
    MissingSupportedStandard {
        standard: NepStandard,
    },
    MissingMethod {
        name: String,
    },
    MissingEvent {
        name: String,
    },
    MethodSignatureMismatch {
        name: String,
        expected: FunctionSpec,
        actual: FunctionSpec,
    },
    EventSignatureMismatch {
        name: String,
        expected: FunctionSpec,
        actual: FunctionSpec,
    },
}

impl fmt::Display for CompatibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompatibilityError::MissingSupportedStandard { standard } => {
                write!(
                    f,
                    "missing supported standard `{}`",
                    standard.manifest_name()
                )
            }
            CompatibilityError::MissingMethod { name } => write!(f, "missing method `{name}`"),
            CompatibilityError::MissingEvent { name } => write!(f, "missing event `{name}`"),
            CompatibilityError::MethodSignatureMismatch { name, .. } => {
                write!(f, "method `{name}` signature does not match the standard")
            }
            CompatibilityError::EventSignatureMismatch { name, .. } => {
                write!(f, "event `{name}` signature does not match the standard")
            }
        }
    }
}

pub fn validate_standard(
    standard: NepStandard,
    shape: &ContractShape,
) -> Result<(), Vec<CompatibilityError>> {
    let mut errors = Vec::new();
    if !shape.supported_standards.contains(&standard) {
        errors.push(CompatibilityError::MissingSupportedStandard { standard });
    }

    let (required_methods, required_events) = match standard {
        NepStandard::Nep17 => (nep17_methods(), nep17_events()),
        NepStandard::Nep11 => (nep11_methods(), nep11_events()),
        _ => (Vec::new(), Vec::new()),
    };

    for expected in required_methods {
        match shape
            .methods
            .iter()
            .find(|method| method.name == expected.name)
        {
            None => errors.push(CompatibilityError::MissingMethod {
                name: expected.name,
            }),
            Some(actual) if !signature_matches(actual, &expected) => {
                errors.push(CompatibilityError::MethodSignatureMismatch {
                    name: expected.name.clone(),
                    expected,
                    actual: actual.clone(),
                });
            }
            Some(_) => {}
        }
    }

    for expected in required_events {
        match shape
            .events
            .iter()
            .find(|event| event.name == expected.name)
        {
            None => errors.push(CompatibilityError::MissingEvent {
                name: expected.name,
            }),
            Some(actual) if !signature_matches(actual, &expected) => {
                errors.push(CompatibilityError::EventSignatureMismatch {
                    name: expected.name.clone(),
                    expected,
                    actual: actual.clone(),
                });
            }
            Some(_) => {}
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn p(name: &'static str, ty: NeoType) -> ParameterSpec {
    ParameterSpec::new(name, ty)
}

fn f(name: &'static str, params: Vec<ParameterSpec>, ret: NeoType) -> FunctionSpec {
    FunctionSpec::new(name, params, ret)
}

fn signature_matches(actual: &FunctionSpec, expected: &FunctionSpec) -> bool {
    actual.return_type == expected.return_type
        && actual.parameters.len() == expected.parameters.len()
        && actual
            .parameters
            .iter()
            .zip(expected.parameters.iter())
            .all(|(actual, expected)| actual.ty == expected.ty)
}

fn nep17_methods() -> Vec<FunctionSpec> {
    vec![
        f("totalSupply", vec![], NeoType::Integer),
        f("symbol", vec![], NeoType::String),
        f("decimals", vec![], NeoType::Integer),
        f(
            "balanceOf",
            vec![p("account", NeoType::Hash160)],
            NeoType::Integer,
        ),
        f(
            "transfer",
            vec![
                p("from", NeoType::Hash160),
                p("to", NeoType::Hash160),
                p("amount", NeoType::Integer),
                p("data", NeoType::Any),
            ],
            NeoType::Boolean,
        ),
    ]
}

fn nep17_events() -> Vec<FunctionSpec> {
    vec![f(
        "Transfer",
        vec![
            p("from", NeoType::Hash160),
            p("to", NeoType::Hash160),
            p("amount", NeoType::Integer),
        ],
        NeoType::Void,
    )]
}

fn nep11_methods() -> Vec<FunctionSpec> {
    vec![
        f("symbol", vec![], NeoType::String),
        f("decimals", vec![], NeoType::Integer),
        f("totalSupply", vec![], NeoType::Integer),
        f("tokens", vec![], NeoType::Iterator),
        f(
            "balanceOf",
            vec![p("owner", NeoType::Hash160)],
            NeoType::Integer,
        ),
        f(
            "tokensOf",
            vec![p("owner", NeoType::Hash160)],
            NeoType::Iterator,
        ),
        f(
            "ownerOf",
            vec![p("tokenId", NeoType::ByteArray)],
            NeoType::Hash160,
        ),
        f(
            "properties",
            vec![p("tokenId", NeoType::ByteArray)],
            NeoType::Map,
        ),
        f(
            "transfer",
            vec![
                p("to", NeoType::Hash160),
                p("tokenId", NeoType::ByteArray),
                p("data", NeoType::Any),
            ],
            NeoType::Boolean,
        ),
    ]
}

fn nep11_events() -> Vec<FunctionSpec> {
    vec![f(
        "Transfer",
        vec![
            p("from", NeoType::Hash160),
            p("to", NeoType::Hash160),
            p("amount", NeoType::Integer),
            p("tokenId", NeoType::ByteArray),
        ],
        NeoType::Void,
    )]
}
