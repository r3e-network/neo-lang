use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NeoType {
    Void,
    Any,
    Boolean,
    Integer,
    String,
    Hash160,
    Hash256,
    ByteArray,
    Buffer,
    Array,
    Map,
    PublicKey,
    Signature,
    InteropInterface,
    Iterator,
}

impl NeoType {
    pub fn manifest_name(self) -> &'static str {
        match self {
            NeoType::Void => "Void",
            NeoType::Any => "Any",
            NeoType::Boolean => "Boolean",
            NeoType::Integer => "Integer",
            NeoType::String => "String",
            NeoType::Hash160 => "Hash160",
            NeoType::Hash256 => "Hash256",
            NeoType::ByteArray | NeoType::Buffer | NeoType::Signature => "ByteArray",
            NeoType::Array => "Array",
            NeoType::Map => "Map",
            NeoType::PublicKey => "PublicKey",
            NeoType::InteropInterface | NeoType::Iterator => "InteropInterface",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterSpec {
    pub name: String,
    pub ty: NeoType,
}

impl ParameterSpec {
    pub fn new(name: impl Into<String>, ty: NeoType) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CallFlags {
    None,
    ReadStates,
    WriteStates,
    AllowCall,
    AllowNotify,
    States,
    ReadOnly,
    All,
}

impl CallFlags {
    pub fn neo_bits(self) -> u8 {
        match self {
            CallFlags::None => 0x00,
            CallFlags::ReadStates => 0x01,
            CallFlags::WriteStates => 0x02,
            CallFlags::AllowCall => 0x04,
            CallFlags::AllowNotify => 0x08,
            CallFlags::States => 0x03,
            CallFlags::ReadOnly => 0x05,
            CallFlags::All => 0x0f,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FindOptions {
    bits: u8,
}

impl FindOptions {
    pub const NONE: Self = Self { bits: 0 };
    pub const KEYS_ONLY: Self = Self { bits: 1 << 0 };
    pub const REMOVE_PREFIX: Self = Self { bits: 1 << 1 };
    pub const VALUES_ONLY: Self = Self { bits: 1 << 2 };
    pub const DESERIALIZE_VALUES: Self = Self { bits: 1 << 3 };
    pub const PICK_FIELD_0: Self = Self { bits: 1 << 4 };
    pub const PICK_FIELD_1: Self = Self { bits: 1 << 5 };

    pub fn neo_bits(self) -> u8 {
        self.bits
    }

    pub fn contains(self, other: Self) -> bool {
        self.bits & other.bits == other.bits
    }

    pub fn with(self, other: Self) -> Result<Self, FindOptionsError> {
        Self {
            bits: self.bits | other.bits,
        }
        .validate()
    }

    pub fn validate(self) -> Result<Self, FindOptionsError> {
        let keys_only = self.contains(Self::KEYS_ONLY);
        let remove_prefix = self.contains(Self::REMOVE_PREFIX);
        let values_only = self.contains(Self::VALUES_ONLY);
        let deserialize_values = self.contains(Self::DESERIALIZE_VALUES);
        let pick_field_0 = self.contains(Self::PICK_FIELD_0);
        let pick_field_1 = self.contains(Self::PICK_FIELD_1);

        if keys_only && (values_only || deserialize_values || pick_field_0 || pick_field_1) {
            return Err(FindOptionsError::Incompatible {
                left: "KeysOnly",
                right: "ValuesOnly, DeserializeValues, PickField0, or PickField1",
            });
        }
        if remove_prefix && values_only {
            return Err(FindOptionsError::Incompatible {
                left: "RemovePrefix",
                right: "ValuesOnly",
            });
        }
        if (pick_field_0 || pick_field_1) && !deserialize_values {
            return Err(FindOptionsError::Requires {
                option: "PickField0/PickField1",
                required: "DeserializeValues",
            });
        }
        if pick_field_0 && pick_field_1 {
            return Err(FindOptionsError::Incompatible {
                left: "PickField0",
                right: "PickField1",
            });
        }
        Ok(self)
    }
}

impl From<FindOptions> for i128 {
    fn from(value: FindOptions) -> Self {
        i128::from(value.neo_bits())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FindOptionsError {
    Incompatible {
        left: &'static str,
        right: &'static str,
    },
    Requires {
        option: &'static str,
        required: &'static str,
    },
}

impl fmt::Display for FindOptionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incompatible { left, right } => {
                write!(f, "FindOptions `{left}` cannot be combined with `{right}`")
            }
            Self::Requires { option, required } => {
                write!(f, "FindOptions `{option}` requires `{required}`")
            }
        }
    }
}

impl std::error::Error for FindOptionsError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionSpec {
    pub name: String,
    pub parameters: Vec<ParameterSpec>,
    pub return_type: NeoType,
    pub safe: bool,
    pub required_call_flags: CallFlags,
}

impl FunctionSpec {
    pub fn new(
        name: impl Into<String>,
        parameters: Vec<ParameterSpec>,
        return_type: NeoType,
    ) -> Self {
        Self {
            name: name.into(),
            parameters,
            return_type,
            safe: false,
            required_call_flags: CallFlags::None,
        }
    }

    pub fn safe(mut self) -> Self {
        self.safe = true;
        self
    }

    pub fn call_flags(mut self, flags: CallFlags) -> Self {
        self.required_call_flags = flags;
        self
    }
}
