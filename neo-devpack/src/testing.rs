use std::collections::BTreeMap;

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
}

impl DevPackTestContext {
    pub fn new(contract_hash: impl Into<String>) -> Self {
        Self {
            storage: StorageFixture::new(contract_hash),
            notifications: NotificationRecorder::new(),
            gas: GasMeter::new(100_000_000),
        }
    }
}
