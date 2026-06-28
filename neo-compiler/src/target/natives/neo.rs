// NEO contract: 0xef4073a0f2b305a38ec4050e4d3d28bc40ea63f5

use crate::target::natives::{NativeContract, NativeMethod};
use crate::target::StackItemType::{
    Any, Array, Boolean as Bool, ByteString as String, Integer as Int,
};

const METHODS: &[NativeMethod] = &[
    NativeMethod::new("symbol", &[], Some(String)),
    NativeMethod::new("decimals", &[], Some(Int)),
    NativeMethod::new("totalSupply", &[], Some(Int)),
    NativeMethod::new("balanceOf", &[String], Some(Int)),
    NativeMethod::new("transfer", &[String, String, Int], Some(Bool)),
    NativeMethod::new("transfer", &[String, String, Int, Any], Some(Bool)),
    NativeMethod::new("getGasPerBlock", &[], Some(Int)),
    NativeMethod::new("getRegisterPrice", &[], Some(Int)),
    NativeMethod::new("unclaimedGas", &[String, Int], Some(Int)),
    NativeMethod::new("registerCandidate", &[String], Some(Bool)),
    NativeMethod::new("unRegisterCandidate", &[String], Some(Bool)),
    NativeMethod::new("vote", &[String], Some(Bool)), // unvote
    NativeMethod::new("vote", &[String, String], Some(Bool)),
    NativeMethod::new("getCandidateVote", &[String], Some(Int)),
    NativeMethod::new("getCommittee", &[], Some(Array)),
    NativeMethod::new("getCommitteeAddress", &[], Some(String)),
    NativeMethod::new("getNextBlockValidators", &[], Some(Array)),
];

pub const NEO: NativeContract = NativeContract {
    name: "NEO",
    hash: [
        0xef, 0x40, 0x73, 0xa0, 0xf2, 0xb3, 0x05, 0xa3, 0x8e, 0xc4, 0x05, 0x0e, 0x4d, 0x3d, 0x28,
        0xbc, 0x40, 0xea, 0x63, 0xf5,
    ],
    methods: METHODS,
};
