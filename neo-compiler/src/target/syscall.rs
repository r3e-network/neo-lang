//! Static syscall metadata for NeoVM syscall.
//! Mirrors the shape of neo-lang `Syscall`: name, typed args, return type, required call flags.
//! The Neo VM remains stack-based and weakly typed; this is for tooling / language front-ends.

use std::collections::HashMap;
use std::sync::LazyLock;

use sha2::{Digest, Sha256};

use crate::target::StackItemType;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CallFlags {
    None = 0x00,
    ReadStates = 0b0000_0001,
    WriteStates = 0b0000_0010,
    AllowCall = 0b0000_0100,
    AllowNotify = 0b0000_1000,
    States = 0b0000_0011,   // ReadStates | WriteStates
    ReadOnly = 0b0000_0101, // ReadStates | AllowCall
    All = 0b0000_1111,      // States | AllowCall | AllowNotify
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FindOptions {
    /// No option is set. The results will be an iterator of (key, value).
    NONE = 0,

    /// Indicates that only keys need to be returned. The results will be an iterator of keys.
    KeysOnly = 1 << 0,

    /// Indicates that the prefix byte of keys should be removed before return
    RemovePrefix = 1 << 1,

    /// Indicates that only values need to be returned. The results will be an iterator of values.
    ValuesOnly = 1 << 2,

    /// Indicates that values should be deserialized before return.
    DeserializeValues = 1 << 3,

    /// Indicates that only the field 0 of the deserialized values need to be returned. This flag must be set together with DeserializeValues.
    PickField0 = 1 << 4,

    /// Indicates that only the field 1 of the deserialized values need to be returned. This flag must be set together with DeserializeValues.
    PickField1 = 1 << 5,

    /// Indicates that results should be returned in backwards (descending) order.
    Backwards = 1 << 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Syscall {
    pub name: &'static str,
    pub args: &'static [(&'static str, StackItemType)],
    pub return_type: Option<StackItemType>, // None means void
    pub callflags: u8,
}

impl Syscall {
    pub fn token(self) -> u32 {
        // first 4 bytes(in little-endian) of sha256(name)
        let mut hasher = Sha256::new();
        hasher.update(self.name.as_bytes());
        let hash = hasher.finalize();
        hash[0] as u32
            | ((hash[1] as u32) << 8)
            | ((hash[2] as u32) << 16)
            | ((hash[3] as u32) << 24)
    }

    // --- System.Contract.*  ---

    pub const CONTRACT_CALL: Syscall = Syscall {
        name: "System.Contract.Call",
        args: &[
            ("contractHash", StackItemType::ByteString), // Hash160, 20 bytes string
            ("method", StackItemType::ByteString),       // method name
            ("callFlags", StackItemType::Integer),       // call flags
            ("args", StackItemType::Array),              // arguments
        ],
        return_type: Some(StackItemType::Any),
        callflags: CallFlags::ReadOnly as u8,
    };

    pub const CONTRACT_GET_CALL_FLAGS: Syscall = Syscall {
        name: "System.Contract.GetCallFlags",
        args: &[],
        return_type: Some(StackItemType::Integer),
        callflags: CallFlags::None as u8,
    };

    pub const CONTRACT_CREATE_STANDARD_ACCOUNT: Syscall = Syscall {
        name: "System.Contract.CreateStandardAccount",
        args: &[("pubKey", StackItemType::ByteString)], // Public key, 33(or 65) bytes string
        return_type: Some(StackItemType::ByteString),   // Hash160, 20 bytes string
        callflags: CallFlags::None as u8,
    };

    pub const CONTRACT_CREATE_MULTISIG_ACCOUNT: Syscall = Syscall {
        name: "System.Contract.CreateMultisigAccount",
        args: &[
            ("m", StackItemType::Integer),     // m, integer
            ("pubKeys", StackItemType::Array), // Public keys, 33(or 65) bytes string array
        ],
        return_type: Some(StackItemType::ByteString), // Hash160, 20 bytes string
        callflags: CallFlags::None as u8,
    };

    // --- System.Crypto.*  ---

    pub const CRYPTO_CHECK_SIG: Syscall = Syscall {
        name: "System.Crypto.CheckSig",
        args: &[
            ("pubkey", StackItemType::ByteString), // Public key, 33(or 65) bytes string
            ("signature", StackItemType::Buffer),  // signature, 64 bytes string
        ],
        return_type: Some(StackItemType::Boolean),
        callflags: CallFlags::None as u8,
    };

    pub const CRYPTO_CHECK_MULTISIG: Syscall = Syscall {
        name: "System.Crypto.CheckMultisig",
        args: &[
            ("pubkeys", StackItemType::Array), // Public keys, 33(or 65) bytes string array
            ("signatures", StackItemType::Array), // signatures, 64 bytes string array
        ],
        return_type: Some(StackItemType::Boolean),
        callflags: CallFlags::None as u8,
    };

    // --- System.Iterator.*  ---

    pub const ITERATOR_NEXT: Syscall = Syscall {
        name: "System.Iterator.Next",
        args: &[("iterator", StackItemType::InteropInterface)],
        return_type: Some(StackItemType::Boolean),
        callflags: CallFlags::None as u8,
    };

    pub const ITERATOR_VALUE: Syscall = Syscall {
        name: "System.Iterator.Value",
        args: &[("iterator", StackItemType::InteropInterface)],
        return_type: Some(StackItemType::Any),
        callflags: CallFlags::None as u8,
    };

    // --- System.Runtime.*  ---

    pub const RUNTIME_PLATFORM: Syscall = Syscall {
        name: "System.Runtime.Platform",
        args: &[],
        return_type: Some(StackItemType::ByteString),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GET_NETWORK: Syscall = Syscall {
        name: "System.Runtime.GetNetwork",
        args: &[],
        return_type: Some(StackItemType::Integer),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GET_ADDRESS_VERSION: Syscall = Syscall {
        name: "System.Runtime.GetAddressVersion",
        args: &[],
        return_type: Some(StackItemType::Integer),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GET_TRIGGER: Syscall = Syscall {
        name: "System.Runtime.GetTrigger",
        args: &[],
        return_type: Some(StackItemType::Integer),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GET_TIME: Syscall = Syscall {
        name: "System.Runtime.GetTime",
        args: &[],
        return_type: Some(StackItemType::Integer),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GET_SCRIPT_CONTAINER: Syscall = Syscall {
        name: "System.Runtime.GetScriptContainer",
        args: &[],
        return_type: Some(StackItemType::Any),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GET_EXECUTING_SCRIPT_HASH: Syscall = Syscall {
        name: "System.Runtime.GetExecutingScriptHash",
        args: &[],
        return_type: Some(StackItemType::ByteString), // Hash160, 20 bytes string
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GET_CALLING_SCRIPT_HASH: Syscall = Syscall {
        name: "System.Runtime.GetCallingScriptHash",
        args: &[],
        return_type: Some(StackItemType::ByteString), // Hash160, 20 bytes string
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GET_ENTRY_SCRIPT_HASH: Syscall = Syscall {
        name: "System.Runtime.GetEntryScriptHash",
        args: &[],
        return_type: Some(StackItemType::ByteString), // Hash160, 20 bytes string
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_LOAD_SCRIPT: Syscall = Syscall {
        name: "System.Runtime.LoadScript",
        args: &[
            ("script", StackItemType::Buffer),
            ("callFlags", StackItemType::Integer),
            ("args", StackItemType::Array),
        ],
        return_type: None,
        callflags: CallFlags::AllowCall as u8,
    };

    pub const RUNTIME_CHECK_WITNESS: Syscall = Syscall {
        name: "System.Runtime.CheckWitness",
        args: &[("hashOrPubkey", StackItemType::Buffer)],
        return_type: Some(StackItemType::Boolean),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GET_INVOCATION_COUNTER: Syscall = Syscall {
        name: "System.Runtime.GetInvocationCounter",
        args: &[],
        return_type: Some(StackItemType::Integer),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GET_RANDOM: Syscall = Syscall {
        name: "System.Runtime.GetRandom",
        args: &[],
        return_type: Some(StackItemType::Integer),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_LOG: Syscall = Syscall {
        name: "System.Runtime.Log",
        args: &[("state", StackItemType::Buffer)],
        return_type: None,
        callflags: CallFlags::AllowNotify as u8,
    };

    pub const RUNTIME_NOTIFY: Syscall = Syscall {
        name: "System.Runtime.Notify",
        args: &[
            ("eventName", StackItemType::ByteString),
            ("state", StackItemType::Array),
        ],
        return_type: None,
        callflags: CallFlags::AllowNotify as u8,
    };

    /// `null` means all notifications. VM stack uses a nullable hash.
    pub const RUNTIME_GET_NOTIFICATIONS: Syscall = Syscall {
        name: "System.Runtime.GetNotifications",
        args: &[("hash", StackItemType::ByteString)], // Hash160, 20 bytes string
        return_type: Some(StackItemType::Array),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_GAS_LEFT: Syscall = Syscall {
        name: "System.Runtime.GasLeft",
        args: &[],
        return_type: Some(StackItemType::Integer),
        callflags: CallFlags::None as u8,
    };

    pub const RUNTIME_BURN_GAS: Syscall = Syscall {
        name: "System.Runtime.BurnGas",
        args: &[("datoshi", StackItemType::Integer)],
        return_type: None,
        callflags: CallFlags::None as u8,
    };

    /// Returns contract signers array or VM null when there is no transaction container.
    pub const RUNTIME_CURRENT_SIGNERS: Syscall = Syscall {
        name: "System.Runtime.CurrentSigners",
        args: &[],
        return_type: Some(StackItemType::Array),
        callflags: CallFlags::None as u8,
    };

    // --- System.Storage.*  ---

    pub const STORAGE_GET_CONTEXT: Syscall = Syscall {
        name: "System.Storage.GetContext",
        args: &[],
        return_type: Some(StackItemType::Array), // a StorageContext struct. The underlying type of struct in `neo-vm` is Array.
        callflags: CallFlags::ReadStates as u8,
    };

    pub const STORAGE_GET_READ_ONLY_CONTEXT: Syscall = Syscall {
        name: "System.Storage.GetReadOnlyContext",
        args: &[],
        return_type: Some(StackItemType::Array), // a StorageContext struct. The underlying type of struct in `neo-vm` is Array.
        callflags: CallFlags::ReadStates as u8,
    };

    pub const STORAGE_AS_READ_ONLY: Syscall = Syscall {
        name: "System.Storage.AsReadOnly",
        args: &[("context", StackItemType::Array)],
        return_type: Some(StackItemType::Array), // a StorageContext struct. The underlying type of struct in `neo-vm` is Array.
        callflags: CallFlags::ReadStates as u8,
    };

    pub const STORAGE_PUT: Syscall = Syscall {
        name: "System.Storage.Put",
        args: &[
            ("key", StackItemType::ByteString),
            ("value", StackItemType::Buffer),
        ],
        return_type: None,
        callflags: CallFlags::WriteStates as u8,
    };

    pub const STORAGE_GET: Syscall = Syscall {
        name: "System.Storage.Get",
        args: &[("key", StackItemType::ByteString)],
        return_type: Some(StackItemType::Buffer),
        callflags: CallFlags::ReadStates as u8,
    };

    pub const STORAGE_DELETE: Syscall = Syscall {
        name: "System.Storage.Delete",
        args: &[("key", StackItemType::ByteString)],
        return_type: None,
        callflags: CallFlags::WriteStates as u8,
    };

    /// Scan storage entries with a prefix. Options is a bitmask of FindOptions.
    pub const STORAGE_FIND: Syscall = Syscall {
        name: "System.Storage.Find",
        args: &[
            ("prefix", StackItemType::ByteString),
            ("options", StackItemType::Integer),
        ],
        return_type: Some(StackItemType::Any), // A iterator of (key, value)
        callflags: CallFlags::ReadStates as u8,
    };

    // --- System.Storage.Local.*  ---

    pub const STORAGE_LOCAL_PUT: Syscall = Syscall {
        name: "System.Storage.Local.Put",
        args: &[
            ("key", StackItemType::ByteString),
            ("value", StackItemType::Buffer),
        ],
        return_type: None,
        callflags: CallFlags::WriteStates as u8,
    };

    pub const STORAGE_LOCAL_GET: Syscall = Syscall {
        name: "System.Storage.Local.Get",
        args: &[("key", StackItemType::ByteString)],
        return_type: Some(StackItemType::Buffer),
        callflags: CallFlags::ReadStates as u8,
    };

    pub const STORAGE_LOCAL_DELETE: Syscall = Syscall {
        name: "System.Storage.Local.Delete",
        args: &[("key", StackItemType::ByteString)],
        return_type: None,
        callflags: CallFlags::WriteStates as u8,
    };

    /// Scan local storage entries with a prefix. Options is a bitmask of FindOptions.
    pub const STORAGE_LOCAL_FIND: Syscall = Syscall {
        name: "System.Storage.Local.Find",
        args: &[
            ("prefix", StackItemType::ByteString),
            ("options", StackItemType::Integer),
        ],
        return_type: Some(StackItemType::Any), // A iterator of (key, value)
        callflags: CallFlags::ReadStates as u8,
    };
}

/// `System.Runtime.*` syscalls exposed in neo-lang as `runtime.<camelName>`,
/// where `<camelName>` is the Neo API segment after `System.Runtime.` with its first letter lowercased
/// (e.g. `GetNetwork` → `getNetwork` → [`Syscall::RUNTIME_GET_NETWORK`]).
pub const RUNTIME_SYSCALLS: &[Syscall] = &[
    Syscall::RUNTIME_PLATFORM,
    Syscall::RUNTIME_GET_NETWORK,
    Syscall::RUNTIME_GET_ADDRESS_VERSION,
    Syscall::RUNTIME_GET_TRIGGER,
    Syscall::RUNTIME_GET_TIME,
    Syscall::RUNTIME_GET_SCRIPT_CONTAINER,
    Syscall::RUNTIME_GET_EXECUTING_SCRIPT_HASH,
    Syscall::RUNTIME_GET_CALLING_SCRIPT_HASH,
    Syscall::RUNTIME_GET_ENTRY_SCRIPT_HASH,
    Syscall::RUNTIME_LOAD_SCRIPT,
    Syscall::RUNTIME_CHECK_WITNESS,
    Syscall::RUNTIME_GET_INVOCATION_COUNTER,
    Syscall::RUNTIME_GET_RANDOM,
    Syscall::RUNTIME_LOG,
    Syscall::RUNTIME_NOTIFY,
    Syscall::RUNTIME_GET_NOTIFICATIONS,
    Syscall::RUNTIME_GAS_LEFT,
    Syscall::RUNTIME_BURN_GAS,
    Syscall::RUNTIME_CURRENT_SIGNERS,
];

/// `System.Runtime.GetNetwork` → `getNetwork` for source `runtime.getNetwork(...)`.
pub fn runtime_suffix_to_camel_case(pascal_suffix: &str) -> String {
    if pascal_suffix.is_empty() {
        return String::new();
    }
    let mut c = pascal_suffix.chars();
    let first: String = c.next().unwrap().to_lowercase().collect();
    first + c.as_str()
}

/// Resolve `runtime.foo` to a syscall in [`RUNTIME_SYSCALLS`].
pub fn runtime_syscall_for_method(method: &str) -> Option<&'static Syscall> {
    const PREFIX: &str = "System.Runtime.";
    for sc in RUNTIME_SYSCALLS {
        if let Some(suffix) = sc.name.strip_prefix(PREFIX) {
            if runtime_suffix_to_camel_case(suffix) == method {
                return Some(sc);
            }
        }
    }
    None
}

const SYSCALLS: &[Syscall] = &[
    Syscall::CONTRACT_CALL,
    Syscall::CONTRACT_GET_CALL_FLAGS,
    Syscall::CONTRACT_CREATE_STANDARD_ACCOUNT,
    Syscall::CONTRACT_CREATE_MULTISIG_ACCOUNT,
    Syscall::CRYPTO_CHECK_SIG,
    Syscall::CRYPTO_CHECK_MULTISIG,
    Syscall::ITERATOR_NEXT,
    Syscall::ITERATOR_VALUE,
    Syscall::RUNTIME_PLATFORM,
    Syscall::RUNTIME_GET_NETWORK,
    Syscall::RUNTIME_GET_ADDRESS_VERSION,
    Syscall::RUNTIME_GET_TRIGGER,
    Syscall::RUNTIME_GET_TIME,
    Syscall::RUNTIME_GET_SCRIPT_CONTAINER,
    Syscall::RUNTIME_GET_EXECUTING_SCRIPT_HASH,
    Syscall::RUNTIME_GET_CALLING_SCRIPT_HASH,
    Syscall::RUNTIME_GET_ENTRY_SCRIPT_HASH,
    Syscall::RUNTIME_LOAD_SCRIPT,
    Syscall::RUNTIME_CHECK_WITNESS,
    Syscall::RUNTIME_GET_INVOCATION_COUNTER,
    Syscall::RUNTIME_GET_RANDOM,
    Syscall::RUNTIME_LOG,
    Syscall::RUNTIME_NOTIFY,
    Syscall::RUNTIME_GET_NOTIFICATIONS,
    Syscall::RUNTIME_GAS_LEFT,
    Syscall::RUNTIME_BURN_GAS,
    Syscall::RUNTIME_CURRENT_SIGNERS,
    Syscall::STORAGE_GET_CONTEXT,
    Syscall::STORAGE_GET_READ_ONLY_CONTEXT,
    Syscall::STORAGE_AS_READ_ONLY,
    Syscall::STORAGE_PUT,
    Syscall::STORAGE_GET,
    Syscall::STORAGE_DELETE,
    Syscall::STORAGE_FIND,
    Syscall::STORAGE_LOCAL_PUT,
    Syscall::STORAGE_LOCAL_GET,
    Syscall::STORAGE_LOCAL_DELETE,
    Syscall::STORAGE_LOCAL_FIND,
];

static SYSCALL_BY_TOKEN: LazyLock<HashMap<u32, &'static Syscall>> =
    LazyLock::new(|| SYSCALLS.iter().map(|s| (s.token(), s)).collect());

pub fn token_to_syscall(token: u32) -> Option<&'static Syscall> {
    SYSCALL_BY_TOKEN.get(&token).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neo_runtime_suffix_to_camel() {
        assert_eq!(runtime_suffix_to_camel_case("GetNetwork"), "getNetwork");
        assert_eq!(runtime_suffix_to_camel_case("Platform"), "platform");
        assert_eq!(
            runtime_suffix_to_camel_case("GetAddressVersion"),
            "getAddressVersion"
        );
    }

    #[test]
    fn runtime_get_network_resolves() {
        let sc = runtime_syscall_for_method("getNetwork").expect("getNetwork");
        assert_eq!(sc.name, "System.Runtime.GetNetwork");
        assert_eq!(sc, &Syscall::RUNTIME_GET_NETWORK);
        assert!(runtime_syscall_for_method("GetNetwork").is_none());
    }

    #[test]
    fn runtime_platform_resolves() {
        assert_eq!(
            runtime_syscall_for_method("platform").map(|s| s.name),
            Some("System.Runtime.Platform")
        );
    }
}
