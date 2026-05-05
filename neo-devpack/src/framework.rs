use std::fmt;

use crate::api::{ApiCatalog, ModuleSpec};
use crate::native::NativeValue;
use crate::types::{CallFlags, FunctionSpec, NeoType};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameworkModule {
    Runtime,
    Storage,
    Contract,
    Crypto,
    Iterator,
}

impl FrameworkModule {
    pub fn name(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Storage => "storage",
            Self::Contract => "contract",
            Self::Crypto => "crypto",
            Self::Iterator => "iterator",
        }
    }

    pub fn call(self, function: impl Into<String>) -> FrameworkCallBuilder {
        FrameworkCallBuilder {
            module: self,
            function: function.into(),
            args: Vec::new(),
        }
    }
}

pub struct Runtime;

impl Runtime {
    pub fn platform() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime.call("platform").build()
    }

    pub fn get_network() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime.call("getNetwork").build()
    }

    pub fn get_address_version() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime.call("getAddressVersion").build()
    }

    pub fn get_trigger() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime.call("getTrigger").build()
    }

    pub fn get_time() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime.call("getTime").build()
    }

    pub fn get_script_container() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime.call("getScriptContainer").build()
    }

    pub fn get_executing_script_hash() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime
            .call("getExecutingScriptHash")
            .build()
    }

    pub fn get_calling_script_hash() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime
            .call("getCallingScriptHash")
            .build()
    }

    pub fn get_entry_script_hash() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime.call("getEntryScriptHash").build()
    }

    pub fn load_script(
        script: FrameworkValue,
        call_flags: CallFlags,
        args: Vec<FrameworkValue>,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime
            .call("loadScript")
            .arg(script)
            .arg(FrameworkValue::Integer(i128::from(call_flags.neo_bits())))
            .arg(FrameworkValue::Array(args))
            .build()
    }

    pub fn check_witness(
        hash_or_pubkey: FrameworkValue,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime
            .call("checkWitness")
            .arg(hash_or_pubkey)
            .build()
    }

    pub fn get_invocation_counter() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime
            .call("getInvocationCounter")
            .build()
    }

    pub fn get_random() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime.call("getRandom").build()
    }

    pub fn log(message: impl Into<String>) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime
            .call("log")
            .arg(FrameworkValue::String(message.into()))
            .build()
    }

    pub fn notify(
        event_name: impl Into<String>,
        state: Vec<FrameworkValue>,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime
            .call("notify")
            .arg(FrameworkValue::String(event_name.into()))
            .arg(FrameworkValue::Array(state))
            .build()
    }

    pub fn get_notifications(
        hash: FrameworkValue,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime
            .call("getNotifications")
            .arg(hash)
            .build()
    }

    pub fn gas_left() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime.call("gasLeft").build()
    }

    pub fn burn_gas(
        datoshi: impl Into<i128>,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime
            .call("burnGas")
            .arg(FrameworkValue::Integer(datoshi.into()))
            .build()
    }

    pub fn current_signers() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Runtime.call("currentSigners").build()
    }
}

pub struct Storage;

impl Storage {
    pub fn get_context() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage.call("getContext").build()
    }

    pub fn get_read_only_context() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage.call("getReadOnlyContext").build()
    }

    pub fn as_read_only(
        context: FrameworkValue,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage
            .call("asReadOnly")
            .arg(context)
            .build()
    }

    pub fn get(key: FrameworkValue) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage.call("get").arg(key).build()
    }

    pub fn put(
        key: FrameworkValue,
        value: FrameworkValue,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage
            .call("put")
            .arg(key)
            .arg(value)
            .build()
    }

    pub fn delete(key: FrameworkValue) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage.call("delete").arg(key).build()
    }

    pub fn find(
        prefix: FrameworkValue,
        options: impl Into<i128>,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage
            .call("find")
            .arg(prefix)
            .arg(FrameworkValue::Integer(options.into()))
            .build()
    }

    pub fn local_get(key: FrameworkValue) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage.call("localGet").arg(key).build()
    }

    pub fn local_put(
        key: FrameworkValue,
        value: FrameworkValue,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage
            .call("localPut")
            .arg(key)
            .arg(value)
            .build()
    }

    pub fn local_delete(key: FrameworkValue) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage
            .call("localDelete")
            .arg(key)
            .build()
    }

    pub fn local_find(
        prefix: FrameworkValue,
        options: impl Into<i128>,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Storage
            .call("localFind")
            .arg(prefix)
            .arg(FrameworkValue::Integer(options.into()))
            .build()
    }
}

pub struct Contract;

impl Contract {
    pub fn call(
        contract_hash: FrameworkValue,
        method: impl Into<String>,
        call_flags: CallFlags,
        args: Vec<FrameworkValue>,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Contract
            .call("call")
            .arg(contract_hash)
            .arg(FrameworkValue::String(method.into()))
            .arg(FrameworkValue::Integer(i128::from(call_flags.neo_bits())))
            .arg(FrameworkValue::Array(args))
            .build()
    }

    pub fn get_call_flags() -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Contract.call("getCallFlags").build()
    }

    pub fn create_standard_account(
        pub_key: FrameworkValue,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Contract
            .call("createStandardAccount")
            .arg(pub_key)
            .build()
    }

    pub fn create_multisig_account(
        m: impl Into<i128>,
        pub_keys: Vec<FrameworkValue>,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Contract
            .call("createMultisigAccount")
            .arg(FrameworkValue::Integer(m.into()))
            .arg(FrameworkValue::Array(pub_keys))
            .build()
    }
}

pub struct Crypto;

impl Crypto {
    pub fn check_sig(
        pub_key: FrameworkValue,
        signature: FrameworkValue,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Crypto
            .call("checkSig")
            .arg(pub_key)
            .arg(signature)
            .build()
    }

    pub fn check_multisig(
        pub_keys: Vec<FrameworkValue>,
        signatures: Vec<FrameworkValue>,
    ) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Crypto
            .call("checkMultisig")
            .arg(FrameworkValue::Array(pub_keys))
            .arg(FrameworkValue::Array(signatures))
            .build()
    }
}

pub struct IteratorApi;

impl IteratorApi {
    pub fn next(iterator: FrameworkValue) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Iterator.call("next").arg(iterator).build()
    }

    pub fn value(iterator: FrameworkValue) -> Result<FrameworkInvocation, FrameworkBindingError> {
        FrameworkModule::Iterator
            .call("value")
            .arg(iterator)
            .build()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameworkValue {
    Null,
    Boolean(bool),
    Integer(i128),
    String(String),
    Hash160(String),
    Hash256(String),
    ByteArray(Vec<u8>),
    Buffer(Vec<u8>),
    Array(Vec<FrameworkValue>),
    Map,
    PublicKey(Vec<u8>),
    Signature(Vec<u8>),
    InteropInterface(String),
    Iterator(String),
}

impl FrameworkValue {
    pub fn ty(&self) -> NeoType {
        match self {
            Self::Null => NeoType::Any,
            Self::Boolean(_) => NeoType::Boolean,
            Self::Integer(_) => NeoType::Integer,
            Self::String(_) => NeoType::String,
            Self::Hash160(_) => NeoType::Hash160,
            Self::Hash256(_) => NeoType::Hash256,
            Self::ByteArray(_) => NeoType::ByteArray,
            Self::Buffer(_) => NeoType::Buffer,
            Self::Array(_) => NeoType::Array,
            Self::Map => NeoType::Map,
            Self::PublicKey(_) => NeoType::PublicKey,
            Self::Signature(_) => NeoType::Signature,
            Self::InteropInterface(_) => NeoType::InteropInterface,
            Self::Iterator(_) => NeoType::Iterator,
        }
    }
}

impl From<NativeValue> for FrameworkValue {
    fn from(value: NativeValue) -> Self {
        match value {
            NativeValue::Null => Self::Null,
            NativeValue::Boolean(value) => Self::Boolean(value),
            NativeValue::Integer(value) => Self::Integer(value),
            NativeValue::String(value) => Self::String(value),
            NativeValue::Hash160(value) => Self::Hash160(value),
            NativeValue::Hash256(value) => Self::Hash256(value),
            NativeValue::ByteArray(value) => Self::ByteArray(value),
            NativeValue::Buffer(value) => Self::Buffer(value),
            NativeValue::Array(values) => Self::Array(values.into_iter().map(Self::from).collect()),
            NativeValue::PublicKey(value) => Self::PublicKey(value),
            NativeValue::Signature(value) => Self::Signature(value),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameworkCallBuilder {
    module: FrameworkModule,
    function: String,
    args: Vec<FrameworkValue>,
}

impl FrameworkCallBuilder {
    pub fn arg(mut self, value: FrameworkValue) -> Self {
        self.args.push(value);
        self
    }

    pub fn build(self) -> Result<FrameworkInvocation, FrameworkBindingError> {
        let catalog = ApiCatalog::neo_n3();
        let module = catalog
            .module(self.module.name())
            .cloned()
            .ok_or(FrameworkBindingError::UnknownModule(self.module.name()))?;
        let function = module.function(&self.function).cloned().ok_or_else(|| {
            FrameworkBindingError::UnknownFunction {
                module: module.name,
                function: self.function.clone(),
            }
        })?;
        if self.args.len() != function.parameters.len() {
            return Err(FrameworkBindingError::ArityMismatch {
                module: module.name,
                function: function.name.clone(),
                expected: function.parameters.len(),
                actual: self.args.len(),
            });
        }
        for (index, (arg, param)) in self.args.iter().zip(function.parameters.iter()).enumerate() {
            let actual = arg.ty();
            if !framework_type_matches(actual, param.ty) {
                return Err(FrameworkBindingError::TypeMismatch {
                    module: module.name,
                    function: function.name.clone(),
                    index,
                    expected: param.ty,
                    actual,
                });
            }
        }
        Ok(FrameworkInvocation {
            module,
            function,
            args: self.args,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameworkInvocation {
    pub module: ModuleSpec,
    pub function: FunctionSpec,
    pub args: Vec<FrameworkValue>,
}

impl FrameworkInvocation {
    pub fn argument_types(&self) -> Vec<NeoType> {
        self.args.iter().map(FrameworkValue::ty).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameworkBindingError {
    UnknownModule(&'static str),
    UnknownFunction {
        module: &'static str,
        function: String,
    },
    ArityMismatch {
        module: &'static str,
        function: String,
        expected: usize,
        actual: usize,
    },
    TypeMismatch {
        module: &'static str,
        function: String,
        index: usize,
        expected: NeoType,
        actual: NeoType,
    },
}

impl fmt::Display for FrameworkBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownModule(module) => write!(f, "unknown framework module `{module}`"),
            Self::UnknownFunction { module, function } => {
                write!(f, "framework module `{module}` has no function `{function}`")
            }
            Self::ArityMismatch {
                module,
                function,
                expected,
                actual,
            } => write!(
                f,
                "{module}.{function} expects {expected} argument(s), got {actual}"
            ),
            Self::TypeMismatch {
                module,
                function,
                index,
                expected,
                actual,
            } => write!(
                f,
                "{module}.{function} argument {index} type mismatch: expected `{expected:?}`, got `{actual:?}`"
            ),
        }
    }
}

impl std::error::Error for FrameworkBindingError {}

fn framework_type_matches(actual: NeoType, expected: NeoType) -> bool {
    expected == NeoType::Any
        || actual == expected
        || matches!(
            (actual, expected),
            (
                NeoType::Hash160
                    | NeoType::Hash256
                    | NeoType::Buffer
                    | NeoType::PublicKey
                    | NeoType::Signature,
                NeoType::ByteArray
            ) | (NeoType::Iterator, NeoType::InteropInterface)
        )
}
