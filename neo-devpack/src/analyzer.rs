use crate::standards::{validate_standard, CompatibilityError, ContractShape, NepStandard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingSeverity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub code: String,
    pub severity: FindingSeverity,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct Analyzer {
    required_standards: Vec<NepStandard>,
}

impl Analyzer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn require_standard(mut self, standard: NepStandard) -> Self {
        self.required_standards.push(standard);
        self
    }

    pub fn analyze(&self, shape: &ContractShape) -> Vec<Finding> {
        let standards = if self.required_standards.is_empty() {
            shape.supported_standards.clone()
        } else {
            self.required_standards.clone()
        };

        let mut findings = Vec::new();
        for standard in standards {
            if let Err(errors) = validate_standard(standard, shape) {
                findings.extend(
                    errors
                        .into_iter()
                        .map(|error| compatibility_finding(standard, error)),
                );
            }
        }
        findings
    }
}

fn compatibility_finding(standard: NepStandard, error: CompatibilityError) -> Finding {
    let standard_prefix = standard.manifest_name().replace('-', "");
    let suffix = match &error {
        CompatibilityError::MissingSupportedStandard { .. } => "MISSING_SUPPORTED_STANDARD",
        CompatibilityError::MissingMethod { .. } => "MISSING_METHOD",
        CompatibilityError::MissingEvent { .. } => "MISSING_EVENT",
        CompatibilityError::MethodSignatureMismatch { .. } => "METHOD_SIGNATURE_MISMATCH",
        CompatibilityError::EventSignatureMismatch { .. } => "EVENT_SIGNATURE_MISMATCH",
    };
    Finding {
        code: format!("{standard_prefix}_{suffix}"),
        severity: FindingSeverity::Error,
        message: error.to_string(),
    }
}
