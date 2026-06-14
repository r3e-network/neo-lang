//! The compiling result contains two file:
//! - The executable file (.nef)
//!   NEF is the neo executable format. The current version is NEF3.
//! - The manifest file (.json)
//!   The manifest file is a JSON file that contains the metadata of the contract.

use std::collections::HashMap;

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::ser::{SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const WILDCARD: &str = "*";

/// Method token is used to identify a method in the executable file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MethodToken {
    pub hash: [u8; 20], // hash160, the contract hash
    pub method: String, // the method name
    pub parameters_count: u16,
    pub has_return_value: bool,
    pub call_flags: u8,
}

impl MethodToken {
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.hash);
        Nef3::write_var_string(out, &self.method);
        out.extend_from_slice(&self.parameters_count.to_le_bytes());
        out.push(u8::from(self.has_return_value));
        out.push(self.call_flags);
    }
}

/// NEF3 is the neo executable format 3.
/// ┌───────────────────────────────────────────────────────────────────────┐
/// │                    NEO Executable Format 3 (NEF3)                     │
/// ├──────────┬───────────────┬────────────────────────────────────────────┤
/// │  Field   │     Type      │                  Comment                   │
/// ├──────────┼───────────────┼────────────────────────────────────────────┤
/// │ Magic    │ uint32        │ Magic header                               │
/// │ Compiler │ byte[64]      │ Compiler name and version                  │
/// ├──────────┼───────────────┼────────────────────────────────────────────┤
/// │ Source   │ byte[]        │ The url of the source files                │
/// │ Reserve  │ byte          │ Reserved for future extensions. Must be 0. │
/// │ Tokens   │ MethodToken[] │ Method tokens.                             │
/// │ Reserve  │ byte[2]       │ Reserved for future extensions. Must be 0. │
/// │ Script   │ byte[]        │ Var bytes for the payload                  │
/// ├──────────┼───────────────┼────────────────────────────────────────────┤
/// │ Checksum │ uint32        │ First four bytes of double SHA256 hash     │
/// └──────────┴───────────────┴────────────────────────────────────────────┘
pub struct Nef3 {
    /// always 0x3346454E(NEF3)
    pub magic: u32,
    pub compiler: [u8; 64],
    pub source: Vec<u8>,
    // pub _reserve1: u8,
    pub tokens: Vec<MethodToken>,
    // pub _reserve2: [u8; 2],
    pub script: Vec<u8>,
}

impl Nef3 {
    /// NEF3 magic constant (`'N' 'E' 'F' '3'` as little-endian u32).
    pub const MAGIC: u32 = 0x3346_454e;

    /// Create a minimal NEF3 with empty `source` and `tokens`.
    pub fn new(script: Vec<u8>, compiler: &str) -> Self {
        let mut compiler_fixed = [0u8; 64];
        let bytes = compiler.as_bytes();
        let n = bytes.len().min(64);
        compiler_fixed[..n].copy_from_slice(&bytes[..n]);
        Self {
            magic: Self::MAGIC,
            compiler: compiler_fixed,
            source: Vec::new(),
            tokens: Vec::new(),
            script,
        }
    }

    pub fn checksum(&self) -> u32 {
        Self::compute_checksum(&self.to_bytes())
    }

    /// Serialize NEF3 bytes (including checksum).
    ///
    /// Encoding rules follow Neo N3 `NefFile`:
    /// - `magic` (u32 LE)
    /// - `compiler` (fixed 64 bytes)
    /// - `source` (VarBytes)
    /// - reserve1 (byte = 0)
    /// - `tokens` (VarInt count, then token entries) — currently empty
    /// - reserve2 (u16 LE = 0)
    /// - `script` (VarBytes)
    /// - `checksum` (u32 LE): first 4 bytes of double SHA256 of all previous bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.magic.to_le_bytes());
        out.extend_from_slice(&self.compiler);
        Self::write_var_bytes(&mut out, &self.source);
        out.push(0); // reserve1
        Self::write_var_int(&mut out, self.tokens.len() as u64);
        for token in &self.tokens {
            token.encode_into(&mut out);
        }
        out.extend_from_slice(&0u16.to_le_bytes()); // reserve2
        Self::write_var_bytes(&mut out, &self.script);

        let checksum = Self::compute_checksum(&out);
        out.extend_from_slice(&checksum.to_le_bytes());
        out
    }

    fn compute_checksum(data: &[u8]) -> u32 {
        let h1 = Sha256::digest(data);
        let h2 = Sha256::digest(h1);
        u32::from_le_bytes([h2[0], h2[1], h2[2], h2[3]])
    }

    /// Neo `WriteVarInt` encoding:
    /// - value < 0xFD: 1 byte
    /// - value <= 0xFFFF: 0xFD + u16 LE
    /// - value <= 0xFFFF_FFFF: 0xFE + u32 LE
    /// - else: 0xFF + u64 LE
    fn write_var_int(out: &mut Vec<u8>, value: u64) {
        if value < 0xFD {
            out.push(value as u8);
        } else if value <= 0xFFFF {
            out.push(0xFD);
            out.extend_from_slice(&(value as u16).to_le_bytes());
        } else if value <= 0xFFFF_FFFF {
            out.push(0xFE);
            out.extend_from_slice(&(value as u32).to_le_bytes());
        } else {
            out.push(0xFF);
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn write_var_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
        Self::write_var_int(out, bytes.len() as u64);
        out.extend_from_slice(bytes);
    }

    fn write_var_string(out: &mut Vec<u8>, s: &str) {
        Self::write_var_bytes(out, s.as_bytes());
    }

    /// Read a Neo `VarInt` from `bytes` at `index`; returns `(value, new_index)`.
    pub fn read_var_int(bytes: &[u8], index: usize) -> Result<(u64, usize), String> {
        let fb = *bytes
            .get(index)
            .ok_or_else(|| format!("nef: truncated varint at offset {index}"))?;
        if fb < 0xFD {
            return Ok((fb as u64, index + 1));
        }
        if fb == 0xFD {
            if index + 3 > bytes.len() {
                return Err("nef: truncated u16 varint".into());
            }
            let v = u16::from_le_bytes([bytes[index + 1], bytes[index + 2]]) as u64;
            return Ok((v, index + 3));
        }
        if fb == 0xFE {
            if index + 5 > bytes.len() {
                return Err("nef: truncated u32 varint".into());
            }
            let v = u32::from_le_bytes([
                bytes[index + 1],
                bytes[index + 2],
                bytes[index + 3],
                bytes[index + 4],
            ]) as u64;
            return Ok((v, index + 5));
        }
        if fb == 0xFF {
            if index + 9 > bytes.len() {
                return Err("nef: truncated u64 varint".into());
            }
            let v = u64::from_le_bytes([
                bytes[index + 1],
                bytes[index + 2],
                bytes[index + 3],
                bytes[index + 4],
                bytes[index + 5],
                bytes[index + 6],
                bytes[index + 7],
                bytes[index + 8],
            ]);
            return Ok((v, index + 9));
        }
        Err(format!(
            "nef: invalid varint prefix 0x{fb:02x} at offset {index}"
        ))
    }

    /// Read `VarBytes` (varint length + payload) from `bytes` at `index`.
    pub fn read_var_bytes(bytes: &[u8], index: usize) -> Result<(Vec<u8>, usize), String> {
        let (len, mut index) = Self::read_var_int(bytes, index)?;
        let len =
            usize::try_from(len).map_err(|_| "nef: byte array length overflow".to_string())?;
        if index + len > bytes.len() {
            return Err(format!(
                "nef: truncated byte array (need {len} bytes at offset {index})"
            ));
        }
        let v = bytes[index..index + len].to_vec();
        index += len;
        Ok((v, index))
    }

    /// Parse a `.nef` file and return the embedded VM script (verifies magic + checksum).
    ///
    /// Supports the same NEF3 layout as [`Self::to_bytes`] (empty `tokens` list only for now).
    pub fn extract_script(bytes: &[u8]) -> Result<Vec<u8>, String> {
        if bytes.len() < 4 + 64 + 1 + 2 + 4 {
            return Err("nef: file too small".into());
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if magic != Self::MAGIC {
            return Err(format!(
                "nef: wrong magic {magic:#x} (expected NEF3 {:#x})",
                Self::MAGIC
            ));
        }
        let mut index = 4usize + 64usize;
        let (_source, next_index) = Self::read_var_bytes(bytes, index)?;
        index = next_index;
        let r1 = *bytes
            .get(index)
            .ok_or_else(|| "nef: missing reserve1".to_string())?;
        if r1 != 0 {
            return Err(format!("nef: reserve1 must be 0, got {r1}"));
        }
        index += 1;
        let (n_tokens, next_index) = Self::read_var_int(bytes, index)?;
        index = next_index;
        if n_tokens != 0 {
            return Err(format!(
                "nef: decoding with non-empty method tokens is not implemented yet (count={n_tokens})"
            ));
        }
        if index + 2 > bytes.len() {
            return Err("nef: missing reserve2".into());
        }
        let r2 = u16::from_le_bytes([bytes[index], bytes[index + 1]]);
        if r2 != 0 {
            return Err(format!("nef: reserve2 must be 0, got {r2}"));
        }
        index += 2;
        let (script, next_index) = Self::read_var_bytes(bytes, index)?;
        index = next_index;
        if index + 4 != bytes.len() {
            return Err(format!(
                "nef: expected exactly 4-byte checksum after script, offset after script={index}, file_len={}",
                bytes.len()
            ));
        }
        let checksum = u32::from_le_bytes(bytes[index..index + 4].try_into().unwrap());
        let body = &bytes[..index];
        let calc = Self::compute_checksum(body);
        if calc != checksum {
            return Err(format!(
                "nef: checksum mismatch (file {checksum:#010x}, computed {calc:#010x})"
            ));
        }
        Ok(script)
    }
}

#[derive(Serialize, Deserialize)]
pub struct ContractGroup {
    /// The hex-encoded public key
    pub pubkey: String,

    pub signature: String,
}

#[derive(Serialize, Deserialize)]
pub struct ContractMethod {
    pub name: String,

    pub parameters: Vec<ContractParameter>,

    #[serde(rename = "returntype")]
    pub return_type: String,

    // The offset in compiled code(script)
    pub offset: u32,

    pub safe: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ContractParameter {
    pub name: String,

    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Serialize, Deserialize)]
pub struct ContractEvent {
    pub name: String,
    pub parameters: Vec<ContractParameter>,
}

#[derive(Serialize, Deserialize)]
pub struct ContractAbi {
    pub methods: Vec<ContractMethod>,
    pub events: Vec<ContractEvent>,
}

/// The permission rule for the contract:
/// - '*' means all contracts are allowed.
/// - ["value1", "value2", ...] means the contracts that are allowed.
pub enum PermissionRule {
    All,
    Allows(Vec<String>),
}

impl Serialize for PermissionRule {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            PermissionRule::All => serializer.serialize_str("*"),
            PermissionRule::Allows(values) => {
                let mut seq = serializer.serialize_seq(Some(values.len()))?;
                for v in values {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for PermissionRule {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PermissionRuleVisitor;
        impl<'de> Visitor<'de> for PermissionRuleVisitor {
            type Value = PermissionRule;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(r#"a "*" string or an array of strings"#)
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                if v == "*" {
                    Ok(PermissionRule::All)
                } else {
                    Err(E::custom(r#"expected "*""#))
                }
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::<String>::new();
                while let Some(v) = seq.next_element::<String>()? {
                    values.push(v);
                }
                Ok(PermissionRule::Allows(values))
            }
        }

        deserializer.deserialize_any(PermissionRuleVisitor)
    }
}

#[derive(Serialize, Deserialize)]
pub struct ContractPermission {
    /// If it is '*', it means all contracts are allowed.
    /// If it is 42-bytes hex-encoded hash160, it means the contract hash that is allowed.
    /// If it is 66-bytes hex-encoded hash256, it means the group(public key) that is allowed.
    pub contract: String,

    pub methods: PermissionRule,
}

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub groups: Vec<ContractGroup>,

    #[serde(rename = "supportedstandards")]
    pub supported_standards: Vec<String>,
    pub abi: ContractAbi,
    pub permissions: Vec<ContractPermission>,
    pub trusts: PermissionRule,

    // The extra metadata for the contract.
    // Including the contract author, version, etc.
    pub extra: HashMap<String, String>,
}
