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

pub struct StdLib;

impl StdLib {
    pub fn serialize(source: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::StdLib.call("serialize").arg(source).build()
    }

    pub fn deserialize(source: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::StdLib
            .call("deserialize")
            .arg(source)
            .build()
    }

    pub fn json_serialize(source: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::StdLib
            .call("jsonSerialize")
            .arg(source)
            .build()
    }

    pub fn json_deserialize(
        json: impl Into<String>,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::StdLib
            .call("jsonDeserialize")
            .arg(NativeValue::String(json.into()))
            .build()
    }

    pub fn base64_encode(input: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::StdLib
            .call("base64Encode")
            .arg(input)
            .build()
    }

    pub fn base64_decode(input: impl Into<String>) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::StdLib
            .call("base64Decode")
            .arg(NativeValue::String(input.into()))
            .build()
    }

    pub fn base58_encode(input: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::StdLib
            .call("base58Encode")
            .arg(input)
            .build()
    }

    pub fn base58_decode(input: impl Into<String>) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::StdLib
            .call("base58Decode")
            .arg(NativeValue::String(input.into()))
            .build()
    }
}

pub struct CryptoLib;

impl CryptoLib {
    pub fn sha256(value: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::CryptoLib.call("sha256").arg(value).build()
    }

    pub fn ripemd160(value: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::CryptoLib
            .call("ripemd160")
            .arg(value)
            .build()
    }

    pub fn verify_with_ecdsa(
        message: NativeValue,
        pub_key: NativeValue,
        signature: NativeValue,
        curve: impl Into<i128>,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::CryptoLib
            .call("verifyWithECDsa")
            .arg(message)
            .arg(pub_key)
            .arg(signature)
            .arg(NativeValue::integer(curve))
            .build()
    }
}

pub struct GasToken;

impl GasToken {
    pub fn symbol() -> Result<NativeInvocation, NativeBindingError> {
        token_symbol(NativeContract::Gas)
    }

    pub fn decimals() -> Result<NativeInvocation, NativeBindingError> {
        token_decimals(NativeContract::Gas)
    }

    pub fn total_supply() -> Result<NativeInvocation, NativeBindingError> {
        token_total_supply(NativeContract::Gas)
    }

    pub fn balance_of(account: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        token_balance_of(NativeContract::Gas, account)
    }

    pub fn transfer(
        from: NativeValue,
        to: NativeValue,
        amount: impl Into<i128>,
        data: NativeValue,
    ) -> Result<NativeInvocation, NativeBindingError> {
        token_transfer(NativeContract::Gas, from, to, amount, data)
    }
}

pub struct NeoToken;

impl NeoToken {
    pub fn symbol() -> Result<NativeInvocation, NativeBindingError> {
        token_symbol(NativeContract::Neo)
    }

    pub fn decimals() -> Result<NativeInvocation, NativeBindingError> {
        token_decimals(NativeContract::Neo)
    }

    pub fn total_supply() -> Result<NativeInvocation, NativeBindingError> {
        token_total_supply(NativeContract::Neo)
    }

    pub fn balance_of(account: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        token_balance_of(NativeContract::Neo, account)
    }

    pub fn transfer(
        from: NativeValue,
        to: NativeValue,
        amount: impl Into<i128>,
        data: NativeValue,
    ) -> Result<NativeInvocation, NativeBindingError> {
        token_transfer(NativeContract::Neo, from, to, amount, data)
    }

    pub fn get_gas_per_block() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Neo.call("getGasPerBlock").build()
    }

    pub fn unclaimed_gas(
        account: NativeValue,
        end: impl Into<i128>,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Neo
            .call("unclaimedGas")
            .arg(account)
            .arg(NativeValue::integer(end))
            .build()
    }

    pub fn register_candidate(
        pub_key: NativeValue,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Neo
            .call("registerCandidate")
            .arg(pub_key)
            .build()
    }

    pub fn unregister_candidate(
        pub_key: NativeValue,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Neo
            .call("unRegisterCandidate")
            .arg(pub_key)
            .build()
    }

    pub fn vote(
        account: NativeValue,
        vote_to: NativeValue,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Neo
            .call("vote")
            .arg(account)
            .arg(vote_to)
            .build()
    }

    pub fn get_candidates() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Neo.call("getCandidates").build()
    }

    pub fn get_committee() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Neo.call("getCommittee").build()
    }

    pub fn get_next_block_validators() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Neo.call("getNextBlockValidators").build()
    }
}

pub struct ContractManagement;

impl ContractManagement {
    pub fn get_minimum_deployment_fee() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::ContractManagement
            .call("getMinimumDeploymentFee")
            .build()
    }

    pub fn get_contract(hash: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::ContractManagement
            .call("getContract")
            .arg(hash)
            .build()
    }

    pub fn get_contract_by_id(id: impl Into<i128>) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::ContractManagement
            .call("getContractById")
            .arg(NativeValue::integer(id))
            .build()
    }

    pub fn get_contract_hashes() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::ContractManagement
            .call("getContractHashes")
            .build()
    }

    pub fn deploy(
        nef_file: NativeValue,
        manifest: impl Into<String>,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::ContractManagement
            .call("deploy")
            .arg(nef_file)
            .arg(NativeValue::String(manifest.into()))
            .build()
    }

    pub fn update(
        nef_file: NativeValue,
        manifest: impl Into<String>,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::ContractManagement
            .call("update")
            .arg(nef_file)
            .arg(NativeValue::String(manifest.into()))
            .build()
    }

    pub fn destroy() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::ContractManagement.call("destroy").build()
    }
}

pub struct Ledger;

impl Ledger {
    pub fn current_hash() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Ledger.call("currentHash").build()
    }

    pub fn current_index() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Ledger.call("currentIndex").build()
    }

    pub fn get_block(hash_or_index: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Ledger
            .call("getBlock")
            .arg(hash_or_index)
            .build()
    }

    pub fn get_transaction(hash: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Ledger
            .call("getTransaction")
            .arg(hash)
            .build()
    }

    pub fn get_transaction_from_block(
        hash_or_index: NativeValue,
        tx_index: impl Into<i128>,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Ledger
            .call("getTransactionFromBlock")
            .arg(hash_or_index)
            .arg(NativeValue::integer(tx_index))
            .build()
    }

    pub fn get_transaction_height(
        hash: NativeValue,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Ledger
            .call("getTransactionHeight")
            .arg(hash)
            .build()
    }
}

pub struct Policy;

impl Policy {
    pub fn get_fee_per_byte() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Policy.call("getFeePerByte").build()
    }

    pub fn get_exec_fee_factor() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Policy.call("getExecFeeFactor").build()
    }

    pub fn get_storage_price() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Policy.call("getStoragePrice").build()
    }

    pub fn is_blocked(account: NativeValue) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Policy
            .call("isBlocked")
            .arg(account)
            .build()
    }
}

pub struct RoleManagement;

impl RoleManagement {
    pub fn get_designated_by_role(
        role: impl Into<i128>,
        index: impl Into<i128>,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::RoleManagement
            .call("getDesignatedByRole")
            .arg(NativeValue::integer(role))
            .arg(NativeValue::integer(index))
            .build()
    }
}

pub struct Oracle;

impl Oracle {
    pub fn get_price() -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Oracle.call("getPrice").build()
    }

    pub fn request(
        url: impl Into<String>,
        filter: impl Into<String>,
        callback: impl Into<String>,
        user_data: NativeValue,
        gas_for_response: impl Into<i128>,
    ) -> Result<NativeInvocation, NativeBindingError> {
        NativeContract::Oracle
            .call("request")
            .arg(NativeValue::String(url.into()))
            .arg(NativeValue::String(filter.into()))
            .arg(NativeValue::String(callback.into()))
            .arg(user_data)
            .arg(NativeValue::integer(gas_for_response))
            .build()
    }
}

fn token_symbol(contract: NativeContract) -> Result<NativeInvocation, NativeBindingError> {
    contract.call("symbol").build()
}

fn token_decimals(contract: NativeContract) -> Result<NativeInvocation, NativeBindingError> {
    contract.call("decimals").build()
}

fn token_total_supply(contract: NativeContract) -> Result<NativeInvocation, NativeBindingError> {
    contract.call("totalSupply").build()
}

fn token_balance_of(
    contract: NativeContract,
    account: NativeValue,
) -> Result<NativeInvocation, NativeBindingError> {
    contract.call("balanceOf").arg(account).build()
}

fn token_transfer(
    contract: NativeContract,
    from: NativeValue,
    to: NativeValue,
    amount: impl Into<i128>,
    data: NativeValue,
) -> Result<NativeInvocation, NativeBindingError> {
    contract
        .call("transfer")
        .arg(from)
        .arg(to)
        .arg(NativeValue::integer(amount))
        .arg(data)
        .build()
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

    pub fn hash256(value: &str) -> Result<Self, NativeBindingError> {
        validate_hex_bytes(value, 32)?;
        Ok(Self::Hash256(normalize_hex(value)))
    }

    pub fn address(value: &str) -> Result<Self, NativeBindingError> {
        let script_hash = decode_neo_n3_address(value)?;
        Ok(Self::Hash160(hex_string(&script_hash)))
    }

    pub fn byte_array(value: &str) -> Result<Self, NativeBindingError> {
        Ok(Self::ByteArray(decode_hex(value)?))
    }

    pub fn buffer(value: &str) -> Result<Self, NativeBindingError> {
        Ok(Self::Buffer(decode_hex(value)?))
    }

    pub fn public_key(value: &str) -> Result<Self, NativeBindingError> {
        let bytes = decode_hex(value)?;
        let valid = matches!(
            (bytes.len(), bytes.first().copied()),
            (33, Some(0x02 | 0x03)) | (65, Some(0x04))
        );
        if !valid {
            return Err(NativeBindingError::InvalidPublicKey {
                actual_bytes: bytes.len(),
                first_byte: bytes.first().copied(),
            });
        }
        Ok(Self::PublicKey(bytes))
    }

    pub fn signature(value: &str) -> Result<Self, NativeBindingError> {
        Ok(Self::Signature(decode_fixed_hex(value, 64)?))
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
    InvalidHexString {
        actual_nibbles: usize,
    },
    InvalidPublicKey {
        actual_bytes: usize,
        first_byte: Option<u8>,
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
            Self::InvalidHexString { actual_nibbles } => write!(
                f,
                "invalid hex string with {actual_nibbles} hex nibbles"
            ),
            Self::InvalidPublicKey {
                actual_bytes,
                first_byte,
            } => {
                let first_byte = first_byte
                    .map(|byte| format!("0x{byte:02x}"))
                    .unwrap_or_else(|| "none".to_string());
                write!(
                    f,
                    "public key must be 33-byte compressed key with prefix 0x02/0x03 or 65-byte uncompressed key with prefix 0x04, got {actual_bytes} byte(s) with first byte {first_byte}"
                )
            }
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

fn decode_fixed_hex(value: &str, expected_bytes: usize) -> Result<Vec<u8>, NativeBindingError> {
    validate_hex_bytes(value, expected_bytes)?;
    decode_hex(value)
}

fn decode_hex(value: &str) -> Result<Vec<u8>, NativeBindingError> {
    let raw = value.strip_prefix("0x").unwrap_or(value);
    if !raw.len().is_multiple_of(2) || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(NativeBindingError::InvalidHexString {
            actual_nibbles: raw.len(),
        });
    }

    let mut bytes = Vec::with_capacity(raw.len() / 2);
    for pair in raw.as_bytes().chunks_exact(2) {
        let high = hex_digit(pair[0]).expect("validated hex digit");
        let low = hex_digit(pair[1]).expect("validated hex digit");
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
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
