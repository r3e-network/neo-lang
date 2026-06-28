// Oracle contract: 0xfe924b7cfe89ddd271abaf7210a80a7e11178758

use crate::target::natives::{NativeContract, NativeMethod};
use crate::target::StackItemType::{Any, ByteString as String, Integer as Int};

const METHODS: &[NativeMethod] = &[
    NativeMethod::new("getPrice", &[], Some(Int)),
    NativeMethod::new("request", &[String, String, String, Any, Int], None),
];

pub const ORACLE: NativeContract = NativeContract {
    name: "Oracle",
    hash: [
        0xfe, 0x92, 0x4b, 0x7c, 0xfe, 0x89, 0xdd, 0xd2, 0x71, 0xab, 0xaf, 0x72, 0x10, 0xa8, 0x0a,
        0x7e, 0x11, 0x17, 0x87, 0x58,
    ],
    methods: METHODS,
};
