//! NEF method tokens and deduplicated registry for [`OpCode::CALLT`].

use std::collections::HashMap;

use crate::target::nef::MethodToken;

#[derive(Debug, Default, Clone)]
pub struct MethodTokenRegistry {
    tokens: Vec<MethodToken>,
    index: HashMap<MethodToken, u16>,
}

#[derive(Debug, thiserror::Error)]
pub enum MethodTokenError {
    #[error("method-token: too many tokens (max 65535)")]
    LimitExceeded,
}

impl MethodTokenRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the token index for `token`, inserting it when not yet present.
    pub fn intern(&mut self, token: MethodToken) -> Result<u16, MethodTokenError> {
        if let Some(index) = self.index.get(&token) {
            return Ok(*index);
        }
        let index =
            u16::try_from(self.tokens.len()).map_err(|_| MethodTokenError::LimitExceeded)?;
        self.index.insert(token.clone(), index);
        self.tokens.push(token);
        Ok(index)
    }

    pub fn tokens(&self) -> &[MethodToken] {
        &self.tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::natives::contract_management::CONTRACT_MANAGEMENT;
    use crate::target::syscall::CallFlags;

    #[test]
    fn intern_deduplicates_identical_tokens() {
        let mut registry = MethodTokenRegistry::new();
        let token = MethodToken {
            hash: CONTRACT_MANAGEMENT.hash,
            method: "isContract".into(),
            parameters_count: 1,
            has_return_value: true,
            call_flags: CallFlags::ReadOnly as u8,
        };
        let a = registry.intern(token.clone()).unwrap();
        let b = registry.intern(token).unwrap();
        assert_eq!(a, 0);
        assert_eq!(b, 0);
        assert_eq!(registry.tokens().len(), 1);
    }
}
