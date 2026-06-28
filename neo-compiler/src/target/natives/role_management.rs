// RoleManagement contract: 0x49cf4e5378ffcd4dec034fd98a174c5491e395e2

use crate::target::natives::{NativeContract, NativeMethod};
use crate::target::StackItemType::{Array, Integer as Int};

const METHODS: &[NativeMethod] = &[NativeMethod::new(
    "getDesignatedByRole",
    &[Int, Int],
    Some(Array),
)];

pub const ROLE_MANAGEMENT: NativeContract = NativeContract {
    name: "RoleManagement",
    hash: [
        0x49, 0xcf, 0x4e, 0x53, 0x78, 0xff, 0xcd, 0x4d, 0xec, 0x03, 0x4f, 0xd9, 0x8a, 0x17, 0x4c,
        0x54, 0x91, 0xe3, 0x95, 0xe2,
    ],
    methods: METHODS,
};
