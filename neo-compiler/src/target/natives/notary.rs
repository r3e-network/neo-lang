// Notary contract: 0xc1e14f19c3e60d0b9244d06dd7ba9b113135ec3b

use crate::target::natives::{NativeContract, NativeMethod};
use crate::target::StackItemType::{Boolean as Bool, ByteString as String, Integer as Int};

const METHODS: &[NativeMethod] = &[
    NativeMethod::new("lockDepositUntil", &[String, Int], Some(Bool)),
    NativeMethod::new("withdraw", &[String], Some(Bool)),
    NativeMethod::new("withdraw", &[String, String], Some(Bool)),
    NativeMethod::new("balanceOf", &[String], Some(Int)),
    NativeMethod::new("expirationOf", &[String], Some(Int)),
    NativeMethod::new("verify", &[String], Some(Bool)),
    NativeMethod::new("getMaxNotValidBeforeDelta", &[], Some(Int)),
];

pub const NOTARY: NativeContract = NativeContract {
    name: "Notary",
    hash: [
        0xc1, 0xe1, 0x4f, 0x19, 0xc3, 0xe6, 0x0d, 0x0b, 0x92, 0x44, 0xd0, 0x6d, 0xd7, 0xba, 0x9b,
        0x11, 0x31, 0x35, 0xec, 0x3b,
    ],
    methods: METHODS,
};
