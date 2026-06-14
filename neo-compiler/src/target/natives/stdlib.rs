// StdLib contract: 0xacce6fd80d44e1796aa0c2c625e9e4e0ce39efc0

use crate::target::natives::{NativeContract, NativeMethod};
use crate::target::StackItemType::{
    Any, Array, Boolean as Bool, ByteString as String, Integer as Int,
};

const METHODS: &[NativeMethod] = &[
    NativeMethod::new("serialize", &[Any], Some(String)),
    NativeMethod::new("deserialize", &[String], Some(Any)),
    NativeMethod::new("jsonSerialize", &[Any], Some(String)),
    NativeMethod::new("jsonDeserialize", &[String], Some(Any)),
    NativeMethod::new("base64Decode", &[String], Some(String)),
    NativeMethod::new("base64Encode", &[String], Some(String)),
    NativeMethod::new("base64UrlDecode", &[String], Some(String)),
    NativeMethod::new("base64UrlEncode", &[String], Some(String)),
    NativeMethod::new("base58Decode", &[String], Some(String)),
    NativeMethod::new("base58Encode", &[String], Some(String)),
    NativeMethod::new("base58CheckEncode", &[String], Some(String)),
    NativeMethod::new("base58CheckDecode", &[String], Some(String)),
    NativeMethod::new("hexEncode", &[String], Some(String)),
    NativeMethod::new("hexDecode", &[String], Some(String)),
    NativeMethod::new("itoa", &[Int, Int], Some(String)),
    NativeMethod::new("atoi", &[String, Int], Some(Int)),
    NativeMethod::new("memoryCompare", &[String, String], Some(Int)),
    NativeMethod::new("memorySearch", &[String, String], Some(Int)),
    NativeMethod::new("memorySearch", &[String, String, Int], Some(Int)),
    NativeMethod::new("memorySearch", &[String, String, Int, Bool], Some(Int)),
    NativeMethod::new("stringSplit", &[String, String], Some(Array)),
    NativeMethod::new("stringSplit", &[String, String, Bool], Some(Array)),
    NativeMethod::new("strLen", &[String], Some(Int)),
];

pub const STD_LIB: NativeContract = NativeContract {
    name: "StdLib",
    hash: [
        0xac, 0xce, 0x6f, 0xd8, 0x0d, 0x44, 0xe1, 0x79, 0x6a, 0xa0, 0xc2, 0xc6, 0x25, 0xe9, 0xe4,
        0xe0, 0xce, 0x39, 0xef, 0xc0,
    ],
    methods: METHODS,
};
