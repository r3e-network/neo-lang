// ContractManagement contract: 0xfffdc93764dbaddd97c48f252a53ea4643faa3fd

use crate::target::natives::{NativeContract, NativeMethod};
use crate::target::StackItemType::{Any, Boolean as Bool, ByteString as String, Integer};

const METHODS: &[NativeMethod] = &[
    NativeMethod::new("getMinimumDeploymentFee", &[], Some(Integer)),
    NativeMethod::new("isContract", &[String], Some(Bool)),
    NativeMethod::new("hasContract", &[String, String, Integer], Some(Bool)),
    NativeMethod::new("update", &[String, String], None),
    NativeMethod::new("update", &[String, String, Any], None),
    NativeMethod::new("destroy", &[], None),
];

pub const CONTRACT_MANAGEMENT: NativeContract = NativeContract {
    name: "ContractManagement",
    hash: [
        0xff, 0xfd, 0xc9, 0x37, 0x64, 0xdb, 0xad, 0xdd, 0x97, 0xc4, 0x8f, 0x25, 0x2a, 0x53, 0xea,
        0x46, 0x43, 0xfa, 0xa3, 0xfd,
    ],
    methods: METHODS,
};
