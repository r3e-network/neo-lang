// Policy contract: 0xcc5e4edd9f5f8dba8bb65734541df7a1c081c67b

use crate::target::natives::{NativeContract, NativeMethod};
use crate::target::StackItemType::{Boolean as Bool, ByteString as String, Integer as Int};

const METHODS: &[NativeMethod] = &[
    NativeMethod::new("getFeePerByte", &[], Some(Int)),
    NativeMethod::new("getExecFeeFactor", &[], Some(Int)),
    NativeMethod::new("getExecPicoFeeFactor", &[], Some(Int)),
    NativeMethod::new("getStoragePrice", &[], Some(Int)),
    NativeMethod::new("isBlocked", &[String], Some(Bool)),
    NativeMethod::new("getAttributeFee", &[Int], Some(Int)),
];

pub const POLICY: NativeContract = NativeContract {
    name: "Policy",
    hash: [
        0xcc, 0x5e, 0x4e, 0xdd, 0x9f, 0x5f, 0x8d, 0xba, 0x8b, 0xb6, 0x57, 0x34, 0x54, 0x1d, 0xf7,
        0xa1, 0xc0, 0x81, 0xc6, 0x7b,
    ],
    methods: METHODS,
};
