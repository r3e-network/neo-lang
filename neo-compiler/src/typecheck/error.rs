use thiserror::Error;

use crate::diagnostic::{Diagnostic, Span};

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("type-error: {message}")]
    Message { message: String, span: Option<Span> },
}

#[inline]
pub(super) fn err(s: impl std::fmt::Display) -> TypeError {
    TypeError::Message {
        message: s.to_string(),
        span: None,
    }
}

#[inline]
pub(super) fn err_at(span: Span, s: impl std::fmt::Display) -> TypeError {
    TypeError::Message {
        message: s.to_string(),
        span: Some(span),
    }
}

impl TypeError {
    pub fn span(&self) -> Option<Span> {
        match self {
            TypeError::Message { span, .. } => *span,
        }
    }

    pub fn diagnostic(&self) -> Diagnostic {
        let message = match self {
            TypeError::Message { message, .. } => message,
        };
        Diagnostic::error(message, self.span())
    }
}
