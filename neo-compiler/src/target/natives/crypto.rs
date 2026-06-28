// CryptoLib contract: 0x726cb6e0cd8628a1350a611384688911ab75f51b

use crate::target::natives::{NativeContract, NativeMethod};
use crate::target::StackItemType::{Boolean as Bool, ByteString as String, Integer as Int};

const METHODS: &[NativeMethod] = &[
    NativeMethod::new("recoverSecp256K1", &[String, String], Some(String)),
    NativeMethod::new("sha256", &[String], Some(String)),
    NativeMethod::new("ripemd160", &[String], Some(String)),
    NativeMethod::new("keccak256", &[String], Some(String)),
    NativeMethod::new("murmur32", &[String, Int], Some(String)),
    NativeMethod::new("verifyWithECDsa", &[String, String, String, Int], Some(Bool)),
    NativeMethod::new("verifyWithEd25519", &[String, String, String], Some(Bool)),
];

pub const CRYPTO_LIB: NativeContract = NativeContract {
    name: "CryptoLib",
    hash: [
        0x72, 0x6c, 0xb6, 0xe0, 0xcd, 0x86, 0x28, 0xa1, 0x35, 0x0a, 0x61, 0x13, 0x84, 0x68, 0x89,
        0x11, 0xab, 0x75, 0xf5, 0x1b,
    ],
    methods: METHODS,
};
