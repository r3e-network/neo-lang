//! Static syscall metadata for NeoVM syscall.
//! Mirrors the shape of neo-lang `Syscall`: name, typed args, return type, required call flags.
//! The Neo VM remains stack-based and weakly typed; this is for tooling / language front-ends.

use std::collections::HashMap;
use std::sync::LazyLock;

use sha2::{Digest, Sha256};

use crate::syntax::ast::Type;
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
        return_type: None, // return type
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
        args: &[],
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

/// `GetNetwork` → `getNetwork` (used for `System.Runtime.*` methods).
pub fn runtime_suffix_to_camel_case(pascal_suffix: &str) -> String {
    if pascal_suffix.is_empty() {
        return String::new();
    }
    let mut c = pascal_suffix.chars();
    let first: String = c.next().unwrap().to_lowercase().collect();
    first + c.as_str()
}

/// `System.*.*.Call` → `call`; `System.Runtime.GetNetwork` → `getNetwork`.
pub fn default_runtime_method_name(syscall_name: &str) -> String {
    let rest = syscall_name.strip_prefix("System.").unwrap_or(syscall_name);
    let method = rest.rsplit('.').next().unwrap_or(rest);
    runtime_suffix_to_camel_case(method)
}

/// One stack item to push before `SYSCALL` for a [`RuntimeBinding`] (bottom → top order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeEmitStep {
    SourceArg(usize),
    InjectedInt(i64),
    Syscall(Syscall),
}

const CONTRACT_CALL_EMIT: &[RuntimeEmitStep] = &[
    RuntimeEmitStep::SourceArg(2),
    RuntimeEmitStep::InjectedInt(CallFlags::ReadOnly as i64),
    RuntimeEmitStep::SourceArg(1),
    RuntimeEmitStep::SourceArg(0),
    RuntimeEmitStep::Syscall(Syscall::CONTRACT_CALL),
];

const CONTRACT_CALL_SOURCE_ARGS: &[StackItemType] = &[
    StackItemType::ByteString,
    StackItemType::ByteString,
    StackItemType::Array,
];

/// Metadata for exposing a NeoVM [`Syscall`] as `runtime.<method>(...)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeBinding {
    pub method: &'static str,
    pub syscall: Syscall,
    /// When `None`, source arity/types match [`Syscall::args`].
    pub source_args: Option<&'static [StackItemType]>,
    /// When `None`, push user args in reverse [`Syscall::args`] order, then `SYSCALL`.
    pub emit_plan: Option<&'static [RuntimeEmitStep]>,
}

impl RuntimeBinding {
    pub const fn direct(method: &'static str, syscall: Syscall) -> Self {
        Self {
            method,
            syscall,
            source_args: None,
            emit_plan: None,
        }
    }

    pub const fn wrapped(
        method: &'static str,
        syscall: Syscall,
        source_args: &'static [StackItemType],
        emit_plan: &'static [RuntimeEmitStep],
    ) -> Self {
        Self {
            method,
            syscall,
            source_args: Some(source_args),
            emit_plan: Some(emit_plan),
        }
    }

    pub fn source_arg_count(self) -> usize {
        self.source_args
            .map(|args| args.len())
            .unwrap_or(self.syscall.args.len())
    }

    pub fn source_arg_type(self, index: usize) -> StackItemType {
        if let Some(source_args) = self.source_args {
            return source_args[index];
        }
        self.syscall.args[index].1
    }

    pub fn emit_steps(self) -> Vec<RuntimeEmitStep> {
        if let Some(plan) = self.emit_plan {
            return plan.to_vec();
        }
        let mut steps = Vec::with_capacity(self.syscall.args.len() + 1);
        for index in (0..self.syscall.args.len()).rev() {
            steps.push(RuntimeEmitStep::SourceArg(index));
        }
        steps.push(RuntimeEmitStep::Syscall(self.syscall));
        steps
    }

    pub fn return_neo_type(self) -> Type {
        syscall_return_neo_type(&self.syscall)
    }

    pub fn leaves_stack_value(self) -> bool {
        !matches!(self.return_neo_type(), Type::Void)
    }
}

/// All syscalls in [`SYSCALLS`] exposed under the neo-lang `runtime` package.
pub const RUNTIME_BINDINGS: &[RuntimeBinding] = &[
    RuntimeBinding::wrapped(
        "call",
        Syscall::CONTRACT_CALL,
        CONTRACT_CALL_SOURCE_ARGS,
        CONTRACT_CALL_EMIT,
    ),
    RuntimeBinding::direct("getCallFlags", Syscall::CONTRACT_GET_CALL_FLAGS),
    RuntimeBinding::direct(
        "createStandardAccount",
        Syscall::CONTRACT_CREATE_STANDARD_ACCOUNT,
    ),
    RuntimeBinding::direct(
        "createMultisigAccount",
        Syscall::CONTRACT_CREATE_MULTISIG_ACCOUNT,
    ),
    RuntimeBinding::direct("checkSig", Syscall::CRYPTO_CHECK_SIG),
    RuntimeBinding::direct("checkMultisig", Syscall::CRYPTO_CHECK_MULTISIG),
    RuntimeBinding::direct("platform", Syscall::RUNTIME_PLATFORM),
    RuntimeBinding::direct("getNetwork", Syscall::RUNTIME_GET_NETWORK),
    RuntimeBinding::direct("getAddressVersion", Syscall::RUNTIME_GET_ADDRESS_VERSION),
    RuntimeBinding::direct("getTrigger", Syscall::RUNTIME_GET_TRIGGER),
    RuntimeBinding::direct("getTime", Syscall::RUNTIME_GET_TIME),
    RuntimeBinding::direct("getScriptContainer", Syscall::RUNTIME_GET_SCRIPT_CONTAINER),
    RuntimeBinding::direct(
        "getExecutingScriptHash",
        Syscall::RUNTIME_GET_EXECUTING_SCRIPT_HASH,
    ),
    RuntimeBinding::direct(
        "getCallingScriptHash",
        Syscall::RUNTIME_GET_CALLING_SCRIPT_HASH,
    ),
    RuntimeBinding::direct("getEntryScriptHash", Syscall::RUNTIME_GET_ENTRY_SCRIPT_HASH),
    RuntimeBinding::direct("loadScript", Syscall::RUNTIME_LOAD_SCRIPT),
    RuntimeBinding::direct("checkWitness", Syscall::RUNTIME_CHECK_WITNESS),
    RuntimeBinding::direct(
        "getInvocationCounter",
        Syscall::RUNTIME_GET_INVOCATION_COUNTER,
    ),
    RuntimeBinding::direct("getRandom", Syscall::RUNTIME_GET_RANDOM),
    RuntimeBinding::direct("log", Syscall::RUNTIME_LOG),
    RuntimeBinding::direct("notify", Syscall::RUNTIME_NOTIFY),
    RuntimeBinding::direct("getNotifications", Syscall::RUNTIME_GET_NOTIFICATIONS),
    RuntimeBinding::direct("gasLeft", Syscall::RUNTIME_GAS_LEFT),
    RuntimeBinding::direct("burnGas", Syscall::RUNTIME_BURN_GAS),
    RuntimeBinding::direct("currentSigners", Syscall::RUNTIME_CURRENT_SIGNERS),
    RuntimeBinding::direct("getContext", Syscall::STORAGE_GET_CONTEXT),
    RuntimeBinding::direct("getReadOnlyContext", Syscall::STORAGE_GET_READ_ONLY_CONTEXT),
    RuntimeBinding::direct("asReadOnly", Syscall::STORAGE_AS_READ_ONLY),
    RuntimeBinding::direct("put", Syscall::STORAGE_PUT),
    RuntimeBinding::direct("get", Syscall::STORAGE_GET),
    RuntimeBinding::direct("delete", Syscall::STORAGE_DELETE),
    RuntimeBinding::direct("find", Syscall::STORAGE_FIND),
    // `System.Storage.Local.*` shares method names with `System.Storage.*`; contract
    // fields use those syscalls directly in codegen, not via `runtime.*`.
];

/// Handle to a resolved `runtime.<method>` binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeMethod(pub &'static RuntimeBinding);

impl RuntimeMethod {
    pub fn resolve(method: &str) -> Option<Self> {
        runtime_binding_for_method(method).map(Self)
    }

    pub fn binding(self) -> &'static RuntimeBinding {
        self.0
    }

    pub fn source_arg_count(self) -> usize {
        self.0.source_arg_count()
    }

    pub fn emit_steps(self) -> Vec<RuntimeEmitStep> {
        self.0.emit_steps()
    }

    pub fn return_neo_type(self) -> Type {
        self.0.return_neo_type()
    }

    pub fn leaves_stack_value(self) -> bool {
        self.0.leaves_stack_value()
    }
}

static RUNTIME_BINDING_BY_METHOD: LazyLock<HashMap<&'static str, &'static RuntimeBinding>> =
    LazyLock::new(|| RUNTIME_BINDINGS.iter().map(|b| (b.method, b)).collect());

pub fn runtime_binding_for_method(method: &str) -> Option<&'static RuntimeBinding> {
    RUNTIME_BINDING_BY_METHOD.get(method).copied()
}

/// Deprecated alias; prefer [`runtime_binding_for_method`].
pub fn runtime_syscall_for_method(method: &str) -> Option<&'static Syscall> {
    runtime_binding_for_method(method).map(|binding| &binding.syscall)
}

/// Whether a neo-lang type satisfies a syscall stack-item parameter type.
pub fn neo_type_satisfies_stack_item(ty: &Type, sit: StackItemType) -> bool {
    match sit {
        StackItemType::Boolean => matches!(ty, Type::Bool),
        StackItemType::Integer => matches!(ty, Type::Int),
        StackItemType::ByteString => matches!(ty, Type::String | Type::Hash160 | Type::Hash256),
        // Source often passes string literals where syscall metadata says `Buffer` (e.g. `runtime.log`).
        StackItemType::Buffer => matches!(ty, Type::Buffer | Type::String | Type::Hash160 | Type::Hash256),
        StackItemType::Array => matches!(ty, Type::Array(_) | Type::Any),
        StackItemType::Map => matches!(ty, Type::Map { .. } | Type::Any),
        StackItemType::Any => true,
        StackItemType::Pointer | StackItemType::InteropInterface => false,
    }
}

pub fn syscall_return_neo_type(syscall: &Syscall) -> Type {
    let Some(return_ty) = syscall.return_type else {
        return Type::Void;
    };
    stack_item_to_neo_type(return_ty)
}

pub fn stack_item_to_neo_type(sit: StackItemType) -> Type {
    match sit {
        StackItemType::Boolean => Type::Bool,
        StackItemType::Integer => Type::Int,
        StackItemType::ByteString => Type::String,
        StackItemType::Buffer => Type::Buffer,
        StackItemType::Array => Type::Array(Box::new(Type::Any)),
        StackItemType::Map => Type::Map {
            key: Box::new(Type::Any),
            value: Box::new(Type::Any),
        },
        StackItemType::Any => Type::Any,
        StackItemType::Pointer | StackItemType::InteropInterface => Type::Any,
    }
}

const SYSCALLS: &[Syscall] = &[
    Syscall::CONTRACT_CALL,
    Syscall::CONTRACT_GET_CALL_FLAGS,
    Syscall::CONTRACT_CREATE_STANDARD_ACCOUNT,
    Syscall::CONTRACT_CREATE_MULTISIG_ACCOUNT,
    Syscall::CRYPTO_CHECK_SIG,
    Syscall::CRYPTO_CHECK_MULTISIG,
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
    fn default_runtime_method_names_follow_convention() {
        for binding in RUNTIME_BINDINGS {
            if binding.source_args.is_some() || binding.emit_plan.is_some() {
                continue;
            }
            assert_eq!(
                binding.method,
                default_runtime_method_name(binding.syscall.name),
                "unexpected method name for {}",
                binding.syscall.name
            );
        }
    }

    #[test]
    fn runtime_get_network_resolves() {
        let binding = runtime_binding_for_method("getNetwork").expect("getNetwork");
        assert_eq!(binding.syscall.name, "System.Runtime.GetNetwork");
        assert_eq!(binding.syscall, Syscall::RUNTIME_GET_NETWORK);
        assert!(runtime_binding_for_method("GetNetwork").is_none());
    }

    #[test]
    fn runtime_call_resolves_with_wrapper() {
        let binding = runtime_binding_for_method("call").expect("call");
        assert_eq!(binding.syscall.name, "System.Contract.Call");
        assert_eq!(binding.source_arg_count(), 3);
        assert!(binding.emit_plan.is_some());
    }

    #[test]
    fn runtime_storage_get_resolves() {
        let binding = runtime_binding_for_method("get").expect("get");
        assert_eq!(binding.syscall.name, "System.Storage.Get");
    }

    #[test]
    fn runtime_binding_method_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for binding in RUNTIME_BINDINGS {
            assert!(
                seen.insert(binding.method),
                "duplicate runtime method `{}`",
                binding.method
            );
        }
    }

    #[test]
    fn runtime_platform_resolves() {
        assert_eq!(
            runtime_binding_for_method("platform").map(|b| b.syscall.name),
            Some("System.Runtime.Platform")
        );
    }
}
