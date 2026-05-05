use std::fmt::{self, Write};

use crate::api::{ApiCatalog, NativeContractSpec};
use crate::types::{FunctionSpec, NeoType};
use sha2::{Digest, Sha256};

pub const NEO_N3_ADDRESS_VERSION: u8 = 0x35;

const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const NEO_SCRIPT_HASH_BYTES: usize = 20;
const NEO_ADDRESS_BYTES: usize = 1 + NEO_SCRIPT_HASH_BYTES + 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NativeContract {
    ContractManagement,
    StdLib,
    CryptoLib,
    Ledger,
    Neo,
    Gas,
    Policy,
    RoleManagement,
    Oracle,
}

impl NativeContract {
    pub fn name(self) -> &'static str {
        match self {
            Self::ContractManagement => "ContractManagement",
            Self::StdLib => "StdLib",
            Self::CryptoLib => "CryptoLib",
            Self::Ledger => "Ledger",
            Self::Neo => "NEO",
            Self::Gas => "GAS",
            Self::Policy => "Policy",
            Self::RoleManagement => "RoleManagement",
            Self::Oracle => "Oracle",
        }
    }

    pub fn call(self, method: impl Into<String>) -> NativeCallBuilder {
        NativeCallBuilder {
            contract: self,
            method: method.into(),
            args: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeValue {
    Null,
    Boolean(bool),
    Integer(i128),
    String(String),
    Hash160(String),
    Hash256(String),
    ByteArray(Vec<u8>),
    Buffer(Vec<u8>),
    Array(Vec<NativeValue>),
    PublicKey(Vec<u8>),
    Signature(Vec<u8>),
}

impl NativeValue {
    pub fn null() -> Self {
        Self::Null
    }

    pub fn integer(value: impl Into<i128>) -> Self {
        Self::Integer(value.into())
    }

    pub fn hash160(value: &str) -> Result<Self, NativeBindingError> {
        validate_hex_bytes(value, 20)?;
        Ok(Self::Hash160(normalize_hex(value)))
    }

    pub fn address(value: &str) -> Result<Self, NativeBindingError> {
        let script_hash = decode_neo_n3_address(value)?;
        Ok(Self::Hash160(hex_string(&script_hash)))
    }

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
            Self::PublicKey(_) => NeoType::PublicKey,
            Self::Signature(_) => NeoType::Signature,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCallBuilder {
    contract: NativeContract,
    method: String,
    args: Vec<NativeValue>,
}

impl NativeCallBuilder {
    pub fn arg(mut self, value: NativeValue) -> Self {
        self.args.push(value);
        self
    }

    pub fn build(self) -> Result<NativeInvocation, NativeBindingError> {
        let catalog = ApiCatalog::neo_n3();
        let contract = catalog
            .native_contract(self.contract.name())
            .cloned()
            .ok_or(NativeBindingError::UnknownContract(self.contract.name()))?;
        let method = contract.function(&self.method).cloned().ok_or_else(|| {
            NativeBindingError::UnknownMethod {
                contract: contract.name,
                method: self.method.clone(),
            }
        })?;
        if self.args.len() != method.parameters.len() {
            return Err(NativeBindingError::ArityMismatch {
                contract: contract.name,
                method: method.name.clone(),
                expected: method.parameters.len(),
                actual: self.args.len(),
            });
        }
        for (index, (arg, param)) in self.args.iter().zip(method.parameters.iter()).enumerate() {
            let actual = arg.ty();
            if !native_type_matches(actual, param.ty) {
                return Err(NativeBindingError::TypeMismatch {
                    contract: contract.name,
                    method: method.name.clone(),
                    index,
                    expected: param.ty,
                    actual,
                });
            }
        }
        Ok(NativeInvocation {
            contract_hash: contract.hash,
            contract,
            method,
            args: self.args,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeInvocation {
    pub contract_hash: &'static str,
    pub contract: NativeContractSpec,
    pub method: FunctionSpec,
    pub args: Vec<NativeValue>,
}

impl NativeInvocation {
    pub fn argument_types(&self) -> Vec<NeoType> {
        self.args.iter().map(NativeValue::ty).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeBindingError {
    UnknownContract(&'static str),
    UnknownMethod {
        contract: &'static str,
        method: String,
    },
    ArityMismatch {
        contract: &'static str,
        method: String,
        expected: usize,
        actual: usize,
    },
    TypeMismatch {
        contract: &'static str,
        method: String,
        index: usize,
        expected: NeoType,
        actual: NeoType,
    },
    InvalidHex {
        expected_bytes: usize,
        actual_nibbles: usize,
    },
    InvalidBase58Character {
        character: char,
        index: usize,
    },
    InvalidAddressLength {
        expected_bytes: usize,
        actual_bytes: usize,
    },
    InvalidAddressVersion {
        expected: u8,
        actual: u8,
    },
    InvalidAddressChecksum,
}

impl fmt::Display for NativeBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownContract(contract) => write!(f, "unknown native contract `{contract}`"),
            Self::UnknownMethod { contract, method } => {
                write!(f, "native contract `{contract}` has no method `{method}`")
            }
            Self::ArityMismatch {
                contract,
                method,
                expected,
                actual,
            } => write!(
                f,
                "{contract}.{method} expects {expected} argument(s), got {actual}"
            ),
            Self::TypeMismatch {
                contract,
                method,
                index,
                expected,
                actual,
            } => write!(
                f,
                "{contract}.{method} argument {index} type mismatch: expected `{expected:?}`, got `{actual:?}`"
            ),
            Self::InvalidHex {
                expected_bytes,
                actual_nibbles,
            } => write!(
                f,
                "expected {expected_bytes} byte hex value, got {actual_nibbles} hex nibbles"
            ),
            Self::InvalidBase58Character { character, index } => {
                write!(f, "invalid Base58 character `{character}` at index {index}")
            }
            Self::InvalidAddressLength {
                expected_bytes,
                actual_bytes,
            } => write!(
                f,
                "expected {expected_bytes} byte Neo address payload, got {actual_bytes} byte(s)"
            ),
            Self::InvalidAddressVersion { expected, actual } => write!(
                f,
                "address version mismatch: expected 0x{expected:02x}, got 0x{actual:02x}"
            ),
            Self::InvalidAddressChecksum => write!(f, "address checksum mismatch"),
        }
    }
}

impl std::error::Error for NativeBindingError {}

fn native_type_matches(actual: NeoType, expected: NeoType) -> bool {
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
            )
        )
}

fn normalize_hex(value: &str) -> String {
    let raw = value.strip_prefix("0x").unwrap_or(value);
    format!("0x{}", raw.to_ascii_lowercase())
}

fn hex_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(2 + bytes.len() * 2);
    out.push_str("0x");
    for byte in bytes {
        write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}

fn validate_hex_bytes(value: &str, expected_bytes: usize) -> Result<(), NativeBindingError> {
    let raw = value.strip_prefix("0x").unwrap_or(value);
    if raw.len() != expected_bytes * 2 || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(NativeBindingError::InvalidHex {
            expected_bytes,
            actual_nibbles: raw.len(),
        });
    }
    Ok(())
}

fn decode_neo_n3_address(value: &str) -> Result<[u8; NEO_SCRIPT_HASH_BYTES], NativeBindingError> {
    let bytes = decode_base58(value)?;
    if bytes.len() != NEO_ADDRESS_BYTES {
        return Err(NativeBindingError::InvalidAddressLength {
            expected_bytes: NEO_ADDRESS_BYTES,
            actual_bytes: bytes.len(),
        });
    }

    let version = bytes[0];
    if version != NEO_N3_ADDRESS_VERSION {
        return Err(NativeBindingError::InvalidAddressVersion {
            expected: NEO_N3_ADDRESS_VERSION,
            actual: version,
        });
    }

    let checksum_at = 1 + NEO_SCRIPT_HASH_BYTES;
    let checksum = Sha256::digest(Sha256::digest(&bytes[..checksum_at]));
    if bytes[checksum_at..] != checksum[..4] {
        return Err(NativeBindingError::InvalidAddressChecksum);
    }

    let mut script_hash = [0_u8; NEO_SCRIPT_HASH_BYTES];
    script_hash.copy_from_slice(&bytes[1..checksum_at]);
    Ok(script_hash)
}

fn decode_base58(value: &str) -> Result<Vec<u8>, NativeBindingError> {
    let mut decoded_le = Vec::<u8>::new();
    for (index, character) in value.chars().enumerate() {
        let Some(digit) = base58_digit(character) else {
            return Err(NativeBindingError::InvalidBase58Character { character, index });
        };
        let mut carry = digit as u32;
        for byte in &mut decoded_le {
            let next = u32::from(*byte) * 58 + carry;
            *byte = (next & 0xff) as u8;
            carry = next >> 8;
        }
        while carry > 0 {
            decoded_le.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }

    let leading_zeroes = value
        .chars()
        .take_while(|character| *character == '1')
        .count();
    let mut decoded = vec![0_u8; leading_zeroes];
    decoded.extend(decoded_le.iter().rev());
    Ok(decoded)
}

fn base58_digit(character: char) -> Option<u8> {
    if !character.is_ascii() {
        return None;
    }
    BASE58_ALPHABET
        .iter()
        .position(|candidate| *candidate == character as u8)
        .map(|index| index as u8)
}
