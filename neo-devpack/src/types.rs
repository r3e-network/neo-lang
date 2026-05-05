use serde::{Deserialize, Serialize};

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
