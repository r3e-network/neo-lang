use std::collections::BTreeMap;
use std::fmt;

use crate::native::{NativeInvocation, NativeValue};
use crate::types::NeoType;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageFixture {
    contract_hash: String,
    entries: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl StorageFixture {
    pub fn new(contract_hash: impl Into<String>) -> Self {
        Self {
            contract_hash: contract_hash.into(),
            entries: BTreeMap::new(),
        }
    }

    pub fn contract_hash(&self) -> &str {
        &self.contract_hash
    }

    pub fn put<K, V>(&mut self, key: K, value: V)
    where
        K: AsRef<[u8]>,
        V: Into<Vec<u8>>,
    {
        self.entries.insert(key.as_ref().to_vec(), value.into());
    }

    pub fn get<K>(&self, key: K) -> Option<Vec<u8>>
    where
        K: AsRef<[u8]>,
    {
        self.entries.get(key.as_ref()).cloned()
    }

    pub fn delete<K>(&mut self, key: K)
    where
        K: AsRef<[u8]>,
    {
        self.entries.remove(key.as_ref());
    }

    pub fn find_prefix<K>(&self, prefix: K) -> Vec<(Vec<u8>, Vec<u8>)>
    where
        K: AsRef<[u8]>,
    {
        let prefix = prefix.as_ref();
        self.entries
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub event_name: String,
    pub state: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotificationRecorder {
    notifications: Vec<Notification>,
}

impl NotificationRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn notify(&mut self, event_name: impl Into<String>, state: Vec<String>) {
        self.notifications.push(Notification {
            event_name: event_name.into(),
            state,
        });
    }

    pub fn all(&self) -> &[Notification] {
        &self.notifications
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GasError {
    BudgetExceeded,
    Overflow,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeMockRegistry {
    responses: BTreeMap<(String, String), NativeValue>,
}

impl NativeMockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn when(
        &mut self,
        contract: impl Into<String>,
        method: impl Into<String>,
        response: NativeValue,
    ) -> &mut Self {
        self.responses
            .insert((contract.into(), method.into()), response);
        self
    }

    pub fn invoke(&self, invocation: &NativeInvocation) -> Result<NativeValue, NativeMockError> {
        let key = (
            invocation.contract.name.to_string(),
            invocation.method.name.clone(),
        );
        let response =
            self.responses
                .get(&key)
                .cloned()
                .ok_or_else(|| NativeMockError::MissingMock {
                    contract: invocation.contract.name.to_string(),
                    method: invocation.method.name.clone(),
                })?;
        let actual = response.ty();
        let expected = invocation.method.return_type;
        if !native_mock_type_matches(actual, expected) {
            return Err(NativeMockError::ReturnTypeMismatch {
                contract: invocation.contract.name.to_string(),
                method: invocation.method.name.clone(),
                expected,
                actual,
            });
        }
        Ok(response)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMockError {
    MissingMock {
        contract: String,
        method: String,
    },
    ReturnTypeMismatch {
        contract: String,
        method: String,
        expected: NeoType,
        actual: NeoType,
    },
}

impl fmt::Display for NativeMockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingMock { contract, method } => {
                write!(f, "no native mock registered for {contract}.{method}")
            }
            Self::ReturnTypeMismatch {
                contract,
                method,
                expected,
                actual,
            } => write!(
                f,
                "native mock {contract}.{method} return type mismatch: expected `{expected:?}`, got `{actual:?}`"
            ),
        }
    }
}

impl std::error::Error for NativeMockError {}

fn native_mock_type_matches(actual: NeoType, expected: NeoType) -> bool {
    expected == NeoType::Any
        || actual == expected
        || matches!(
            (actual, expected),
            (NeoType::Any, NeoType::Void)
                | (
                    NeoType::Hash160
                        | NeoType::Hash256
                        | NeoType::Buffer
                        | NeoType::PublicKey
                        | NeoType::Signature,
                    NeoType::ByteArray
                )
        )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GasMeter {
    budget: u64,
    consumed: u64,
}

impl GasMeter {
    pub fn new(budget: u64) -> Self {
        Self {
            budget,
            consumed: 0,
        }
    }

    pub fn charge(&mut self, amount: u64) -> Result<(), GasError> {
        let next = self
            .consumed
            .checked_add(amount)
            .ok_or(GasError::Overflow)?;
        if next > self.budget {
            return Err(GasError::BudgetExceeded);
        }
        self.consumed = next;
        Ok(())
    }

    pub fn consumed(&self) -> u64 {
        self.consumed
    }

    pub fn remaining(&self) -> u64 {
        self.budget - self.consumed
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevPackTestContext {
    pub storage: StorageFixture,
    pub notifications: NotificationRecorder,
    pub gas: GasMeter,
    pub native: NativeMockRegistry,
}

impl DevPackTestContext {
    pub fn new(contract_hash: impl Into<String>) -> Self {
        Self {
            storage: StorageFixture::new(contract_hash),
            notifications: NotificationRecorder::new(),
            gas: GasMeter::new(100_000_000),
            native: NativeMockRegistry::new(),
        }
    }
}
