pub use crate::types::CallFlags;
use crate::types::{FunctionSpec, NeoType, ParameterSpec};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub functions: Vec<FunctionSpec>,
}

impl ModuleSpec {
    pub fn function(&self, name: &str) -> Option<&FunctionSpec> {
        self.functions.iter().find(|function| function.name == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeContractSpec {
    pub name: &'static str,
    pub hash: &'static str,
    pub description: &'static str,
    pub functions: Vec<FunctionSpec>,
}

impl NativeContractSpec {
    pub fn function(&self, name: &str) -> Option<&FunctionSpec> {
        self.functions.iter().find(|function| function.name == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiCatalog {
    modules: Vec<ModuleSpec>,
    native_contracts: Vec<NativeContractSpec>,
}

impl ApiCatalog {
    pub fn neo_n3() -> Self {
        Self {
            modules: vec![
                runtime_module(),
                storage_module(),
                contract_module(),
                crypto_module(),
                iterator_module(),
            ],
            native_contracts: vec![
                contract_management(),
                std_lib(),
                crypto_lib(),
                ledger(),
                neo_token(),
                gas_token(),
                policy(),
                role_management(),
                oracle(),
            ],
        }
    }

    pub fn module(&self, name: &str) -> Option<&ModuleSpec> {
        self.modules.iter().find(|module| module.name == name)
    }

    pub fn modules(&self) -> &[ModuleSpec] {
        &self.modules
    }

    pub fn native_contract(&self, name: &str) -> Option<&NativeContractSpec> {
        self.native_contracts
            .iter()
            .find(|contract| contract.name == name)
    }

    pub fn native_contracts(&self) -> &[NativeContractSpec] {
        &self.native_contracts
    }
}

fn p(name: &'static str, ty: NeoType) -> ParameterSpec {
    ParameterSpec::new(name, ty)
}

fn f(name: &'static str, params: Vec<ParameterSpec>, ret: NeoType) -> FunctionSpec {
    FunctionSpec::new(name, params, ret)
}

fn runtime_module() -> ModuleSpec {
    ModuleSpec {
        name: "runtime",
        description: "Neo N3 runtime execution context APIs.",
        functions: vec![
            f("platform", vec![], NeoType::String).safe(),
            f("getNetwork", vec![], NeoType::Integer).safe(),
            f("getAddressVersion", vec![], NeoType::Integer).safe(),
            f("getTrigger", vec![], NeoType::Integer).safe(),
            f("getTime", vec![], NeoType::Integer).safe(),
            f("getScriptContainer", vec![], NeoType::Any).safe(),
            f("getExecutingScriptHash", vec![], NeoType::Hash160).safe(),
            f("getCallingScriptHash", vec![], NeoType::Hash160).safe(),
            f("getEntryScriptHash", vec![], NeoType::Hash160).safe(),
            f(
                "loadScript",
                vec![
                    p("script", NeoType::ByteArray),
                    p("callFlags", NeoType::Integer),
                    p("args", NeoType::Array),
                ],
                NeoType::Any,
            )
            .call_flags(CallFlags::AllowCall),
            f(
                "checkWitness",
                vec![p("hashOrPubkey", NeoType::ByteArray)],
                NeoType::Boolean,
            )
            .safe(),
            f("getInvocationCounter", vec![], NeoType::Integer).safe(),
            f("getRandom", vec![], NeoType::Integer).safe(),
            f("log", vec![p("message", NeoType::String)], NeoType::Void)
                .call_flags(CallFlags::AllowNotify),
            f(
                "notify",
                vec![p("eventName", NeoType::String), p("state", NeoType::Array)],
                NeoType::Void,
            )
            .call_flags(CallFlags::AllowNotify),
            f(
                "getNotifications",
                vec![p("hash", NeoType::Hash160)],
                NeoType::Array,
            )
            .safe(),
            f("gasLeft", vec![], NeoType::Integer).safe(),
            f(
                "burnGas",
                vec![p("datoshi", NeoType::Integer)],
                NeoType::Void,
            ),
            f("currentSigners", vec![], NeoType::Array).safe(),
        ],
    }
}

fn storage_module() -> ModuleSpec {
    ModuleSpec {
        name: "storage",
        description: "Persistent storage context, read, write, delete, and find APIs.",
        functions: vec![
            f("getContext", vec![], NeoType::Array).safe(),
            f("getReadOnlyContext", vec![], NeoType::Array).safe(),
            f(
                "asReadOnly",
                vec![p("context", NeoType::Array)],
                NeoType::Array,
            )
            .safe(),
            f(
                "get",
                vec![p("key", NeoType::ByteArray)],
                NeoType::ByteArray,
            )
            .safe()
            .call_flags(CallFlags::ReadStates),
            f(
                "put",
                vec![p("key", NeoType::ByteArray), p("value", NeoType::ByteArray)],
                NeoType::Void,
            )
            .call_flags(CallFlags::WriteStates),
            f("delete", vec![p("key", NeoType::ByteArray)], NeoType::Void)
                .call_flags(CallFlags::WriteStates),
            f(
                "find",
                vec![
                    p("prefix", NeoType::ByteArray),
                    p("options", NeoType::Integer),
                ],
                NeoType::Iterator,
            )
            .safe()
            .call_flags(CallFlags::ReadStates),
            f(
                "localGet",
                vec![p("key", NeoType::ByteArray)],
                NeoType::ByteArray,
            )
            .safe()
            .call_flags(CallFlags::ReadStates),
            f(
                "localPut",
                vec![p("key", NeoType::ByteArray), p("value", NeoType::ByteArray)],
                NeoType::Void,
            )
            .call_flags(CallFlags::WriteStates),
            f(
                "localDelete",
                vec![p("key", NeoType::ByteArray)],
                NeoType::Void,
            )
            .call_flags(CallFlags::WriteStates),
            f(
                "localFind",
                vec![
                    p("prefix", NeoType::ByteArray),
                    p("options", NeoType::Integer),
                ],
                NeoType::Iterator,
            )
            .safe()
            .call_flags(CallFlags::ReadStates),
        ],
    }
}

fn contract_module() -> ModuleSpec {
    ModuleSpec {
        name: "contract",
        description: "Contract invocation and account script helpers.",
        functions: vec![
            f(
                "call",
                vec![
                    p("contractHash", NeoType::Hash160),
                    p("method", NeoType::String),
                    p("callFlags", NeoType::Integer),
                    p("args", NeoType::Array),
                ],
                NeoType::Any,
            )
            .call_flags(CallFlags::AllowCall),
            f("getCallFlags", vec![], NeoType::Integer).safe(),
            f(
                "createStandardAccount",
                vec![p("pubKey", NeoType::PublicKey)],
                NeoType::Hash160,
            )
            .safe(),
            f(
                "createMultisigAccount",
                vec![p("m", NeoType::Integer), p("pubKeys", NeoType::Array)],
                NeoType::Hash160,
            )
            .safe(),
        ],
    }
}

fn crypto_module() -> ModuleSpec {
    ModuleSpec {
        name: "crypto",
        description: "Signature verification APIs.",
        functions: vec![
            f(
                "checkSig",
                vec![
                    p("pubKey", NeoType::PublicKey),
                    p("signature", NeoType::Signature),
                ],
                NeoType::Boolean,
            )
            .safe(),
            f(
                "checkMultisig",
                vec![
                    p("pubKeys", NeoType::Array),
                    p("signatures", NeoType::Array),
                ],
                NeoType::Boolean,
            )
            .safe(),
        ],
    }
}

fn iterator_module() -> ModuleSpec {
    ModuleSpec {
        name: "iterator",
        description: "Iterator helpers for storage find and native APIs.",
        functions: vec![
            f(
                "next",
                vec![p("iterator", NeoType::Iterator)],
                NeoType::Boolean,
            )
            .safe(),
            f(
                "value",
                vec![p("iterator", NeoType::Iterator)],
                NeoType::Any,
            )
            .safe(),
        ],
    }
}

fn contract_management() -> NativeContractSpec {
    NativeContractSpec {
        name: "ContractManagement",
        hash: "0xfffdc93764dbaddd97c48f252a53ea4643faa3fd",
        description: "Native contract deployment, update, destroy, and lookup APIs.",
        functions: vec![
            f("getMinimumDeploymentFee", vec![], NeoType::Integer).safe(),
            f(
                "getContract",
                vec![p("hash", NeoType::Hash160)],
                NeoType::Any,
            )
            .safe(),
            f(
                "getContractById",
                vec![p("id", NeoType::Integer)],
                NeoType::Any,
            )
            .safe(),
            f("getContractHashes", vec![], NeoType::Iterator).safe(),
            f(
                "deploy",
                vec![
                    p("nefFile", NeoType::ByteArray),
                    p("manifest", NeoType::String),
                ],
                NeoType::Any,
            ),
            f(
                "update",
                vec![
                    p("nefFile", NeoType::ByteArray),
                    p("manifest", NeoType::String),
                ],
                NeoType::Void,
            ),
            f("destroy", vec![], NeoType::Void),
        ],
    }
}

fn std_lib() -> NativeContractSpec {
    NativeContractSpec {
        name: "StdLib",
        hash: "0xacce6fd80d44e1796aa0c2c625e9e4e0ce39efc0",
        description: "Native serialization, JSON, and base encoding helpers.",
        functions: vec![
            f(
                "serialize",
                vec![p("source", NeoType::Any)],
                NeoType::ByteArray,
            )
            .safe(),
            f(
                "deserialize",
                vec![p("source", NeoType::ByteArray)],
                NeoType::Any,
            )
            .safe(),
            f(
                "jsonSerialize",
                vec![p("source", NeoType::Any)],
                NeoType::String,
            )
            .safe(),
            f(
                "jsonDeserialize",
                vec![p("json", NeoType::String)],
                NeoType::Any,
            )
            .safe(),
            f(
                "base64Encode",
                vec![p("input", NeoType::ByteArray)],
                NeoType::String,
            )
            .safe(),
            f(
                "base64Decode",
                vec![p("input", NeoType::String)],
                NeoType::ByteArray,
            )
            .safe(),
            f(
                "base58Encode",
                vec![p("input", NeoType::ByteArray)],
                NeoType::String,
            )
            .safe(),
            f(
                "base58Decode",
                vec![p("input", NeoType::String)],
                NeoType::ByteArray,
            )
            .safe(),
        ],
    }
}

fn crypto_lib() -> NativeContractSpec {
    NativeContractSpec {
        name: "CryptoLib",
        hash: "0x726cb6e0cd8628a1350a611384688911ab75f51b",
        description: "Native cryptographic algorithms.",
        functions: vec![
            f(
                "sha256",
                vec![p("value", NeoType::ByteArray)],
                NeoType::Hash256,
            )
            .safe(),
            f(
                "ripemd160",
                vec![p("value", NeoType::ByteArray)],
                NeoType::Hash160,
            )
            .safe(),
            f(
                "verifyWithECDsa",
                vec![
                    p("message", NeoType::ByteArray),
                    p("pubKey", NeoType::PublicKey),
                    p("signature", NeoType::Signature),
                    p("curve", NeoType::Integer),
                ],
                NeoType::Boolean,
            )
            .safe(),
        ],
    }
}

fn ledger() -> NativeContractSpec {
    NativeContractSpec {
        name: "Ledger",
        hash: "0xda65b600f7124ce6c79950c1772a36403104f2be",
        description: "Native block and transaction lookup APIs.",
        functions: vec![
            f("currentHash", vec![], NeoType::Hash256).safe(),
            f("currentIndex", vec![], NeoType::Integer).safe(),
            f(
                "getBlock",
                vec![p("hashOrIndex", NeoType::Any)],
                NeoType::Any,
            )
            .safe(),
            f(
                "getTransaction",
                vec![p("hash", NeoType::Hash256)],
                NeoType::Any,
            )
            .safe(),
            f(
                "getTransactionFromBlock",
                vec![
                    p("hashOrIndex", NeoType::Any),
                    p("txIndex", NeoType::Integer),
                ],
                NeoType::Any,
            )
            .safe(),
            f(
                "getTransactionHeight",
                vec![p("hash", NeoType::Hash256)],
                NeoType::Integer,
            )
            .safe(),
        ],
    }
}

fn neo_token() -> NativeContractSpec {
    NativeContractSpec {
        name: "NEO",
        hash: "0xef4073a0f2b305a38ec4050e4d3d28bc40ea63f5",
        description: "Native NEO governance token.",
        functions: token_functions()
            .into_iter()
            .chain(vec![
                f("getGasPerBlock", vec![], NeoType::Integer).safe(),
                f(
                    "unclaimedGas",
                    vec![p("account", NeoType::Hash160), p("end", NeoType::Integer)],
                    NeoType::Integer,
                )
                .safe(),
                f(
                    "registerCandidate",
                    vec![p("pubKey", NeoType::PublicKey)],
                    NeoType::Boolean,
                ),
                f(
                    "unRegisterCandidate",
                    vec![p("pubKey", NeoType::PublicKey)],
                    NeoType::Boolean,
                ),
                f(
                    "vote",
                    vec![
                        p("account", NeoType::Hash160),
                        p("voteTo", NeoType::PublicKey),
                    ],
                    NeoType::Boolean,
                ),
                f("getCandidates", vec![], NeoType::Iterator).safe(),
                f("getCommittee", vec![], NeoType::Array).safe(),
                f("getNextBlockValidators", vec![], NeoType::Array).safe(),
            ])
            .collect(),
    }
}

fn gas_token() -> NativeContractSpec {
    NativeContractSpec {
        name: "GAS",
        hash: "0xd2a4cff31913016155e38e474a2c06d08be276cf",
        description: "Native GAS utility token.",
        functions: token_functions(),
    }
}

fn token_functions() -> Vec<FunctionSpec> {
    vec![
        f("symbol", vec![], NeoType::String).safe(),
        f("decimals", vec![], NeoType::Integer).safe(),
        f("totalSupply", vec![], NeoType::Integer).safe(),
        f(
            "balanceOf",
            vec![p("account", NeoType::Hash160)],
            NeoType::Integer,
        )
        .safe(),
        f(
            "transfer",
            vec![
                p("from", NeoType::Hash160),
                p("to", NeoType::Hash160),
                p("amount", NeoType::Integer),
                p("data", NeoType::Any),
            ],
            NeoType::Boolean,
        ),
    ]
}

fn policy() -> NativeContractSpec {
    NativeContractSpec {
        name: "Policy",
        hash: "0xcc5e4edd9f5f8dba8bb65734541df7a1c081c67b",
        description: "Native policy configuration contract.",
        functions: vec![
            f("getFeePerByte", vec![], NeoType::Integer).safe(),
            f("getExecFeeFactor", vec![], NeoType::Integer).safe(),
            f("getStoragePrice", vec![], NeoType::Integer).safe(),
            f(
                "isBlocked",
                vec![p("account", NeoType::Hash160)],
                NeoType::Boolean,
            )
            .safe(),
        ],
    }
}

fn role_management() -> NativeContractSpec {
    NativeContractSpec {
        name: "RoleManagement",
        hash: "0x49cf4e5378ffcd4dec034fd98a174c5491e395e2",
        description: "Native designated-role lookup contract.",
        functions: vec![f(
            "getDesignatedByRole",
            vec![p("role", NeoType::Integer), p("index", NeoType::Integer)],
            NeoType::Array,
        )
        .safe()],
    }
}

fn oracle() -> NativeContractSpec {
    NativeContractSpec {
        name: "Oracle",
        hash: "0xfe924b7cfe89ddd271abaf7210a80a7e11178758",
        description: "Native Oracle request contract.",
        functions: vec![
            f("getPrice", vec![], NeoType::Integer).safe(),
            f(
                "request",
                vec![
                    p("url", NeoType::String),
                    p("filter", NeoType::String),
                    p("callback", NeoType::String),
                    p("userData", NeoType::Any),
                    p("gasForResponse", NeoType::Integer),
                ],
                NeoType::Void,
            ),
        ],
    }
}
