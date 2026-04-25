// Copyright (C) 2015-2026 The Neo Project.
//
// JumpTable.Control.cs file belongs to the neo project and is free
// software distributed under the MIT software license, see the
// accompanying file LICENSE in the main directory of the
// repository or http://www.opensource.org/licenses/mit-license.php
// for more details.
//
// Redistribution and use in source and binary forms with or without
// modifications are permitted.

use num_enum::TryFromPrimitive;

use crate::syntax::ast::{AssignOp, BinaryOp};

/// The current supported opcodes for the NeoVM.
/// In NeoVM, an opcode is 1 byte long, and some opcodes have additional operands.
/// `neo-lang` compiler will translate the AST to the NeoVM opcodes.
/// This file is from `github.com/neo-project/neo-vm`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive)]
#[allow(non_camel_case_types)]
pub enum OpCode {
    /// Pushes a 1-byte signed integer onto the stack.
    /// The next byte is the integer value.
    PUSHINT8 = 0x00,

    /// Pushes a 2-bytes signed integer onto the stack.
    /// The next 2 bytes are the little-endian integer value.
    PUSHINT16 = 0x01,

    /// Pushes a 4-bytes signed integer onto the stack.
    /// The next 4 bytes are the little-endian integer value.
    PUSHINT32 = 0x02,

    /// Pushes an 8-bytes signed integer onto the stack.
    /// The next 8 bytes are the little-endian integer value.
    PUSHINT64 = 0x03,

    /// Pushes a 16-bytes signed integer onto the stack.
    /// The next 16 bytes are the little-endian integer value.
    PUSHINT128 = 0x04,

    /// Pushes a 32-bytes signed integer onto the stack.
    /// The next 32 bytes are the little-endian integer value.
    PUSHINT256 = 0x05,

    /// Pushes the boolean value `true` onto the stack.
    PUSHT = 0x08,

    /// Pushes the boolean value `false` onto the stack.
    PUSHF = 0x09,

    /// Converts the 4-bytes offset to an pointer, and pushes it onto the stack.
    /// The execution will be faulted if the current position + offset is out of script range[0, script.Length).
    /// The next 4 bytes are the little-endian offset.
    PUSHA = 0x0A,

    /// The item `null` is pushed onto the stack.
    PUSHNULL = 0x0B,

    /// The next byte contains the number of bytes to be pushed onto the stack.
    /// The data format: `|1-byte unsigned size|data|`.
    PUSHDATA1 = 0x0C,

    /// The next two bytes contain the number of bytes to be pushed onto the stack.
    /// The data format: `|2-byte little-endian unsigned size|data|`.
    PUSHDATA2 = 0x0D,

    /// The next four bytes contain the number of bytes to be pushed onto the stack.
    /// The data format: `|4-byte little-endian unsigned size|data|`.
    /// The execution will be faulted if the datasize is out of MaxItemSize.
    PUSHDATA4 = 0x0E,

    /// The number -1 is pushed onto the stack.
    PUSHM1 = 0x0F,

    /// The number 0 is pushed onto the stack.
    PUSH0 = 0x10,

    /// The number 1 is pushed onto the stack.
    PUSH1 = 0x11,

    /// The number 2 is pushed onto the stack.
    PUSH2 = 0x12,

    /// The number 3 is pushed onto the stack.
    PUSH3 = 0x13,

    /// The number 4 is pushed onto the stack.
    PUSH4 = 0x14,

    /// The number 5 is pushed onto the stack.
    PUSH5 = 0x15,

    /// The number 6 is pushed onto the stack.
    PUSH6 = 0x16,

    /// The number 7 is pushed onto the stack.
    PUSH7 = 0x17,

    /// The number 8 is pushed onto the stack.
    PUSH8 = 0x18,

    /// The number 9 is pushed onto the stack.
    PUSH9 = 0x19,

    /// The number 10 is pushed onto the stack.
    PUSH10 = 0x1A,

    /// The number 11 is pushed onto the stack.
    PUSH11 = 0x1B,

    /// The number 12 is pushed onto the stack.
    PUSH12 = 0x1C,

    /// The number 13 is pushed onto the stack.
    PUSH13 = 0x1D,

    /// The number 14 is pushed onto the stack.
    PUSH14 = 0x1E,

    /// The number 15 is pushed onto the stack.
    PUSH15 = 0x1F,

    /// The number 16 is pushed onto the stack.
    PUSH16 = 0x20,

    /// The `NOP` operation does nothing. It is intended to fill in space if opcodes are patched.
    NOP = 0x21,

    /// Unconditionally transfers control to a target instruction.
    /// The target instruction is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// The execution will be faulted if the target offset is out of script range[0, script.Length).
    JMP = 0x22,

    /// Unconditionally transfers control to a target instruction.
    /// The target instruction is represented as a 4-bytes little-endian signed offset from the beginning of the current instruction.
    /// The execution will be faulted if the target offset is out of script range[0, script.Length).
    JMP_L = 0x23,

    /// Transfers control to a target instruction if the value is true value (true, non-null, non-zero).
    /// The target instruction is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// The execution will be faulted if the target offset is out of script range[0, script.Length).
    JMPIF = 0x24,

    /// Transfers control to a target instruction if the value is true value (true, non-null, non-zero).
    /// The target instruction is represented as a 4-bytes little-endian signed offset from the beginning of the current instruction.
    /// The execution will be faulted if the target offset is out of script range[0, script.Length).
    JMPIF_L = 0x25,

    /// Transfers control to a target instruction if the value is false value (false, null, zero).
    /// The target instruction is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// If the target offset is out of script range[0, script.Length), the execution will be faulted.
    JMPIFNOT = 0x26,

    /// Transfers control to a target instruction if the value is false value (false, null, zero).
    /// The target instruction is represented as a 4-bytes little-endian signed offset from the beginning of the current instruction.
    /// The execution will be faulted if the target offset is out of script range[0, script.Length).
    JMPIFNOT_L = 0x27,

    /// Transfers control to a target instruction if the top two items are equal.
    /// The target instruction is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot convert to an integer.
    JMPEQ = 0x28,

    /// Transfers control to a target instruction if the top two items are equal.
    /// The target instruction is represented as a 4-bytes signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot convert to an integer.
    JMPEQ_L = 0x29,

    /// Transfers control to a target instruction when the top two items are not equal.
    /// The target instruction is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot convert to an integer.
    JMPNE = 0x2A,

    /// Transfers control to a target instruction when the top two items are not equal.
    /// The target instruction is represented as a 4-bytes signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot convert to an integer.
    JMPNE_L = 0x2B,

    /// Transfers control to a target instruction if first pushed item(the second in the stack) is greater than the second pushed item.
    /// The target instruction is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot represent as an integer.
    JMPGT = 0x2C,

    /// Transfers control to a target instruction if first pushed item(the second in the stack) is greater than the second pushed item.
    /// The target instruction is represented as a 4-bytes signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot represent as an integer.
    JMPGT_L = 0x2D,

    /// Transfers control to a target instruction if first pushed item(the second in the stack) is greater than or equal to the second pushed item.
    /// The target instruction is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot represent as an integer.
    JMPGE = 0x2E,

    /// Transfers control to a target instruction if first pushed item(the second in the stack) is greater than or equal to the second pushed item.
    /// The target instruction is represented as a 4-bytes signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot represent as an integer.
    JMPGE_L = 0x2F,

    /// Transfers control to a target instruction if first pushed item(the second in the stack) is less than the second pushed item.
    /// The target instruction is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot represent as an integer.
    JMPLT = 0x30,

    /// Transfers control to a target instruction if first pushed item(the second in the stack) is less than the second pushed item.
    /// The target instruction is represented as a 4-bytes signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot represent as an integer.
    JMPLT_L = 0x31,

    /// Transfers control to a target instruction if first pushed item(the second in the stack) is less than or equal to the second pushed item.
    /// The target instruction is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot represent as an integer.
    JMPLE = 0x32,

    /// Transfers control to a target instruction if first pushed item(the second in the stack) is less than or equal to the second pushed item.
    /// The target instruction is represented as a 4-bytes signed offset from the beginning of the current instruction.
    /// If any item is not an integer, it will be converted to an integer then compared.
    /// The execution will be faulted if:
    ///  1. the target offset is out of script range[0, script.Length).
    ///  2. One or both of items cannot represent as an integer.
    JMPLE_L = 0x33,

    /// Calls the function at the target address which is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// The execution will be faulted if the target address is out of script range[0, script.Length).
    CALL = 0x34,

    /// Calls the function at the target address which is represented as a 4-bytes little-endian signed offset from the beginning of the current instruction.
    /// The execution will be faulted if the target address is out of script range[0, script.Length).
    CALL_L = 0x35,

    /// Pop the pointer of a function from the stack, and call the function.
    /// The execution will be faulted if the pointer is not from the current script or not a valid pointer.
    CALLA = 0x36,

    /// Calls the function which is described by the token.
    /// The next 2 bytes are contract method token in little-endian.
    CALLT = 0x37,

    /// It turns the vm state to FAULT immediately, and cannot be caught.
    ABORT = 0x38,

    /// Pop the top value of the stack. If it's false value (false, null, zero), exit vm execution and set vm state to FAULT.
    ASSERT = 0x39,

    /// Pop the top value of the stack, and throw it.
    THROW = 0x3A,

    /// TRY CatchOffset(sbyte) FinallyOffset(sbyte). If there's no catch body, set CatchOffset 0. If there's no finally body, set FinallyOffset 0.
    /// The next 2 bytes are signed 1-byte offset of the `catch` and `finally` blocks.
    /// The execution will be faulted if:
    ///  1. `catch` and `finally` are not provided both.
    ///  2. the `try` can not be nested more than `MaxTryNestingDepth`.
    ///  3. the `catch` or `finally` offset is out of script range[0, script.Length).
    TRY = 0x3B,

    /// TRY_L CatchOffset(int) FinallyOffset(int). If there's no catch body, set CatchOffset 0. If there's no finally body, set FinallyOffset 0.
    /// The next 4 bytes are signed 4-bytes little-endian offset of the `catch` and `finally` blocks.
    /// The execution will be faulted if:
    ///  1. `catch` and `finally` are not provided both.
    ///  2. the `try` can not be nested more than `MaxTryNestingDepth`.
    ///  3. the `catch` or `finally` offset is out of script range[0, script.Length).
    TRY_L = 0x3C,

    /// Ensures that the appropriate surrounding finally blocks are executed.
    /// And then unconditionally transfers control to the specific target instruction,
    /// which is represented as a 1-byte signed offset from the beginning of the current instruction.
    /// The execution will be faulted if:
    ///  1. the corresponding `try` is not provided.
    ///  2. the end offset is out of script range[0, script.Length).
    ENDTRY = 0x3D,

    /// Ensures that the appropriate surrounding finally blocks are executed.
    /// And then unconditionally transfers control to the specific target instruction,
    /// which is represented as a 4-byte little-endian signed offset from the beginning of the current instruction.
    /// The execution will be faulted if:
    ///  1. the corresponding `try` is not provided.
    ///  2. the end offset is out of script range[0, script.Length).
    ENDTRY_L = 0x3E,

    /// End finally, If no exception happen or be catched, vm will jump to the target instruction of ENDTRY/ENDTRY_L.
    /// Otherwise, vm will rethrow the exception to upper layer.
    /// The execution will be faulted if the corresponding `try` is not provided.
    ENDFINALLY = 0x3F,

    /// Returns from the current method. Each function must be returned by `RET`.
    RET = 0x40,

    /// Calls to an system service.
    /// The next 4 bytes are signed 4-bytes little-endian offset of the syscall identifier.
    SYSCALL = 0x41,

    /// Pushes the number of stack items onto the stack.
    DEPTH = 0x43,

    /// Removes the top stack item.
    /// The execution will be faulted if the stack is empty.
    DROP = 0x45,

    /// Removes the second-to-top stack item.
    /// The execution will be faulted if the stack has less than 2 items.
    NIP = 0x46,

    /// The item n back in the main stack is removed. The top item indicates the number of items to be removed.
    /// If the top item is not an integer, it will be converted to an integer.
    /// The execution will be faulted if:
    ///  1. The top item cannot convert to an integer.
    ///  2. The stack has less than n+1 items.
    XDROP = 0x48,

    /// Clear the stack
    CLEAR = 0x49,

    /// Duplicates the top stack item.
    /// The execution will be faulted if the stack is empty.
    DUP = 0x4A,

    /// Copies the second-to-top stack item to the top.
    /// The execution will be faulted if the stack has less than 2 items.
    OVER = 0x4B,

    /// The item n back in the stack is copied to the top. The top item indicates the index of the item to be copied.
    /// If the top item is not an integer, it will be converted to an integer.
    /// The execution will be faulted if:
    ///  1. The top item cannot convert to an integer.
    ///  2. The stack has less than n+1 items.
    PICK = 0x4D,

    /// The item at the top of the stack is copied and inserted before the second-to-top item.
    /// The execution will be faulted if the stack has less than 2 items.
    TUCK = 0x4E,

    /// The top two items on the stack are swapped.
    /// The execution will be faulted if the stack has less than 2 items.
    SWAP = 0x50,

    /// The top three items on the stack are rotated to the left.
    /// The execution will be faulted if the stack has less than 3 items.
    ROT = 0x51,

    /// The item n back in the stack is moved to the top. The top item indicates the index of the item to be moved.
    /// If the top item is not an integer, it will be converted to an integer.
    /// The execution will be faulted if:
    ///  1. The top item cannot convert to an integer.
    ///  2. The stack has less than n+1 items.
    ROLL = 0x52,

    /// Reverse the order of the top 3 items on the stack.
    /// The execution will be faulted if the stack has less than 3 items.
    REVERSE3 = 0x53,

    /// Reverse the order of the top 4 items on the stack.
    /// The execution will be faulted if the stack has less than 4 items.
    REVERSE4 = 0x54,

    /// Pop the number N on the stack, and reverse the order of the top N items on the stack.
    /// If the top item is not an integer, it will be converted to an integer.
    /// The execution will be faulted if:
    ///  1. The top item cannot convert to an integer.
    ///  2. The stack has less than n+1 items.
    REVERSEN = 0x55,

    /// Initialize the static field list for the current execution context.
    /// The execution will be faulted if:
    ///  1. The static field list for the current execution context has been initialized.
    ///  2. The operand is 0.
    INITSSLOT = 0x56,

    /// Initialize the argument slot and/or the local variable list for the current execution context.
    /// It has two uint8 operands: The first is the number of local variables, and the second is the number of arguments.
    /// Two operands cannot both be 0.
    /// The execution will be faulted if:
    ///  1. The argument slot and/or the local variable list for the current execution context has been initialized.
    ///  2. Two operands are both 0.
    INITSLOT = 0x57,

    /// Loads the static field at index 0 onto the evaluation stack.
    LDSFLD0 = 0x58,

    /// Loads the static field at index 1 onto the evaluation stack.
    LDSFLD1 = 0x59,

    /// Loads the static field at index 2 onto the evaluation stack.
    LDSFLD2 = 0x5A,

    /// Loads the static field at index 3 onto the evaluation stack.
    LDSFLD3 = 0x5B,

    /// Loads the static field at index 4 onto the evaluation stack.
    LDSFLD4 = 0x5C,

    /// Loads the static field at index 5 onto the evaluation stack.   
    LDSFLD5 = 0x5D,

    /// Loads the static field at index 6 onto the evaluation stack.
    LDSFLD6 = 0x5E,

    /// Loads the static field at a specified index onto the evaluation stack.
    /// The index is represented as a 1-byte unsigned integer.
    LDSFLD = 0x5F,

    /// Stores the value on top of the evaluation stack in the static field list at index 0.
    STSFLD0 = 0x60,

    /// Stores the value on top of the evaluation stack in the static field list at index 1.
    STSFLD1 = 0x61,

    /// Stores the value on top of the evaluation stack in the static field list at index 2.
    STSFLD2 = 0x62,

    /// Stores the value on top of the evaluation stack in the static field list at index 3.
    STSFLD3 = 0x63,

    /// Stores the value on top of the evaluation stack in the static field list at index 4.
    STSFLD4 = 0x64,

    /// Stores the value on top of the evaluation stack in the static field list at index 5.
    STSFLD5 = 0x65,

    /// Stores the value on top of the evaluation stack in the static field list at index 6.
    STSFLD6 = 0x66,

    /// Stores the value on top of the evaluation stack in the static field list at a specified index.
    /// The index is represented as a 1-byte unsigned integer.
    STSFLD = 0x67,

    /// Loads the local variable at index 0 onto the evaluation stack.
    LDLOC0 = 0x68,

    /// Loads the local variable at index 1 onto the evaluation stack.
    LDLOC1 = 0x69,

    /// Loads the local variable at index 2 onto the evaluation stack.
    LDLOC2 = 0x6A,

    /// Loads the local variable at index 3 onto the evaluation stack.
    LDLOC3 = 0x6B,

    /// Loads the local variable at index 4 onto the evaluation stack.
    LDLOC4 = 0x6C,

    /// Loads the local variable at index 5 onto the evaluation stack.
    LDLOC5 = 0x6D,

    /// Loads the local variable at index 6 onto the evaluation stack.
    LDLOC6 = 0x6E,

    /// Loads the local variable at a specified index onto the evaluation stack.
    /// The index is represented as a 1-byte unsigned integer.
    LDLOC = 0x6F,

    /// Stores the value on top of the evaluation stack in the local variable list at index 0.
    STLOC0 = 0x70,

    /// Stores the value on top of the evaluation stack in the local variable list at index 1.
    STLOC1 = 0x71,

    /// Stores the value on top of the evaluation stack in the local variable list at index 2.
    STLOC2 = 0x72,

    /// Stores the value on top of the evaluation stack in the local variable list at index 3.
    STLOC3 = 0x73,

    /// Stores the value on top of the evaluation stack in the local variable list at index 4.
    STLOC4 = 0x74,

    /// Stores the value on top of the evaluation stack in the local variable list at index 5.
    STLOC5 = 0x75,

    /// Stores the value on top of the evaluation stack in the local variable list at index 6.
    STLOC6 = 0x76,

    /// Stores the value on top of the evaluation stack in the local variable list at a specified index.
    /// The index is represented as a 1-byte unsigned integer.
    STLOC = 0x77,

    /// Loads the argument at index 0 onto the evaluation stack.
    LDARG0 = 0x78,

    /// Loads the argument at index 1 onto the evaluation stack.
    LDARG1 = 0x79,

    /// Loads the argument at index 2 onto the evaluation stack.
    LDARG2 = 0x7A,

    /// Loads the argument at index 3 onto the evaluation stack.
    LDARG3 = 0x7B,

    /// Loads the argument at index 4 onto the evaluation stack.
    LDARG4 = 0x7C,

    /// Loads the argument at index 5 onto the evaluation stack.
    LDARG5 = 0x7D,

    /// Loads the argument at index 6 onto the evaluation stack.
    LDARG6 = 0x7E,

    /// Loads the argument at a specified index onto the evaluation stack.
    /// The index is represented as a 1-byte unsigned integer.
    LDARG = 0x7F,

    /// Stores the value on top of the evaluation stack in the argument slot at index 0.
    STARG0 = 0x80,

    /// Stores the value on top of the evaluation stack in the argument slot at index 1.
    STARG1 = 0x81,

    /// Stores the value on top of the evaluation stack in the argument slot at index 2.
    STARG2 = 0x82,

    /// Stores the value on top of the evaluation stack in the argument slot at index 3.
    STARG3 = 0x83,

    /// Stores the value on top of the evaluation stack in the argument slot at index 4.
    STARG4 = 0x84,

    /// Stores the value on top of the evaluation stack in the argument slot at index 5.
    STARG5 = 0x85,

    /// Stores the value on top of the evaluation stack in the argument slot at index 6.
    STARG6 = 0x86,

    /// Stores the value on top of the evaluation stack in the argument slot at a specified index.
    /// The index is represented as a 1-byte unsigned integer.
    STARG = 0x87,

    /// Creates a new `Buffer` and pushes it onto the stack, and the top item is the length of the buffer.
    /// If the top item is not an integer, it will be converted to an integer.
    NEWBUFFER = 0x88,

    /// Copies a range of bytes from one `Buffer` to another.
    /// Using this opcode will require to dup the destination buffer.
    /// The top 5 items in the stack are(The `count` item is the top item):
    /// `| destination buffer | destination start index | source buffer | source start index | count`.
    /// The execution will be faulted if:
    ///  1. the destination start index, source start index or count cannot be converted to integer.
    ///  2. The destination start index, source start index or count is negative(or converted value is negative).
    ///  3. The destination start index + count is out of the destination buffer range.
    ///  4. The source start index + count is out of the source buffer range.
    MEMCPY = 0x89,

    /// Concatenates two items as a buffer. The result is the first pushed item concatenated with the second pushed item(the top item).
    /// If item is not a buffer, it will be converted to a buffer(bytes) and then concatenated.
    /// `| buffer1 | buffer2`.
    /// The execution will be faulted if:
    ///  1. the total length exceeds the maximum item size.
    ///  2. One or both items cannot be converted to a buffer.
    CAT = 0x8B,

    /// Pushes a sub-buffer from the source buffer onto the evaluation stack.
    /// The first pushed item is the source buffer, the second pushed item is the start index, the third pushed item is the count(the top item).
    /// If the start index or count is not an integer, it will be converted to an integer.
    /// If the source buffer is not a buffer, it will be converted to a buffer(bytes).
    /// `| source buffer | start index | count`.
    /// The execution will be faulted if:
    ///  1. The start index or count cannot be converted to integer.
    ///  2. The source buffer cannot be converted to buffer(bytes).
    ///  3. The start index or count is negative(or converted value is negative) or out of the source buffer range.
    SUBSTR = 0x8C,

    /// Keeps only characters left of the specified point in a buffer.
    /// The first pushed item is the source buffer, the second pushed item is the count(the top item).
    /// If the count is not an integer, it will be converted to an integer.
    /// If the source buffer is not a buffer, it will be converted to a buffer(bytes).
    /// `| source buffer | count`.
    /// The execution will be faulted if:
    ///  1. The count cannot be converted to integer.
    ///  2. The source buffer cannot be converted to buffer(bytes).
    ///  3. The count is negative(or converted value is negative) or out of the source buffer range.
    LEFT = 0x8D,

    /// Keeps only characters right of the specified point in a buffer.
    /// The first pushed item is the source buffer, the second pushed item is the count(the top item).
    /// If the count is not an integer, it will be converted to an integer.
    /// If the source buffer is not a buffer, it will be converted to a buffer(bytes).
    /// `| source buffer | count`.
    /// The execution will be faulted if:
    ///  1. The count cannot be converted to integer.
    ///  2. The source buffer cannot be converted to buffer(bytes).
    ///  3. The count is negative(or converted value is negative) or out of the source buffer range.
    RIGHT = 0x8E,

    /// Pops the top stack item and pushes the result of flipping all the bits in the item.
    /// If the item is not an integer, it will be converted to an integer.
    /// The execution will be faulted if the item cannot be converted to integer.
    INVERT = 0x90,

    /// Pops the top two stack items and pushes the result of the boolean and between each bit in the items.
    /// If the items are not integers, they will be converted to integers.
    /// The execution will be faulted if the items cannot be converted to integers.
    AND = 0x91,

    /// Pops the top two stack items and pushes the result of the boolean or between each bit in the items.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the items cannot be converted to integers.
    OR = 0x92,

    /// Pops the top two stack items and pushes the result of the boolean exclusive or between each bit in the items.
    /// If the items are not integers, they will be converted to integers.
    /// The execution will be faulted if the items cannot be converted to integers.
    XOR = 0x93,

    /// Pops the top two stack items and pushes true if the items are exactly equal, false otherwise.
    EQUAL = 0x97,

    /// Pops the top two stack items and pushes true if the items are not equal, false otherwise.
    NOTEQUAL = 0x98,

    /// Pops the top stack item and pushes the sign of the item.
    /// If the item is negative, push -1; if positive, push 1; if zero, push 0.
    /// If the item is not an integer, it will be converted to an integer.
    /// The execution will be faulted if the item cannot be converted to an integer.
    SIGN = 0x99,

    /// Pops the top stack item and pushes the absolute value of the item.
    /// If the item is not an integer, it will be converted to an integer.
    /// The execution will be faulted if:
    ///  1. the item cannot be converted to an integer.
    ///  2. the item is the minimum integer value.
    ABS = 0x9A,

    /// Pops the top stack item and pushes the negation of the item.
    /// If the input is not an integer, it will be converted to an integer.
    /// The execution will be faulted if the item cannot be converted to an integer.
    NEGATE = 0x9B,

    /// Pops the top stack item and pushes the result of adding 1 to the item.
    /// If the input is not an integer, it will be converted to an integer.
    /// The execution will be faulted if the item cannot be converted to an integer.
    INC = 0x9C,

    /// Pops the top stack item and pushes the result of subtracting 1 from the item.
    /// If the input is not an integer, it will be converted to an integer.
    /// The execution will be faulted if the item cannot be converted to an integer.
    DEC = 0x9D,

    /// Pops the top two stack items and pushes the result of adding the first pushed item to the second pushed item(the top item).
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    ADD = 0x9E,

    /// Pops the top two stack items and pushes the result of subtracting the second pushed item(the top item) from the first pushed item.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    SUB = 0x9F,

    /// Pops the top two stack items and pushes the result of multiplying the first pushed item by the second pushed item(the top item).
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    MUL = 0xA0,

    /// Pops the top two stack items and pushes the result of dividing the first pushed item by the second pushed item(the top item).
    /// The first pushed item is the dividend, the second pushed item is the divisor(the top item).
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if:
    ///  1. the inputs cannot be converted to integers.
    ///  2. the divisor is zero.
    DIV = 0xA1,

    /// Pops the top two stack items and pushes the remainder after dividing a by b.
    /// The first pushed item is the dividend, the second pushed item is the divisor(the top item).
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if:
    ///  1. the inputs cannot be converted to integers.
    ///  2. the divisor is zero.
    MOD = 0xA2,

    /// Pops the top two stack items and pushes the result of raising value to the exponent power.
    /// The first pushed item is the exponent, the second pushed item is the value(the top item).
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    POW = 0xA3,

    /// Pops the top stack item and pushes the square root of the item.
    /// If the input is not an integer, it will be converted to an integer.
    /// The execution will be faulted if:
    ///  1. the input cannot be converted to an integer.
    ///  2. the input is negative.
    SQRT = 0xA4,

    /// Performs modulus division on a number multiplied by another number.
    /// The third pushed item is the modulus.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if:
    ///  1. the inputs cannot be converted to integers.
    ///  2. the modulus is zero.
    MODMUL = 0xA5,

    /// Performs modulus division on a number raised to the power of another number.
    /// If the exponent is -1, it will have the calculation of the modular inverse.
    /// The third pushed item is the modulus, the second pushed item is the exponent, the first pushed item is the value(the top item).
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if:
    ///  1. the inputs cannot be converted to integers.
    ///  2. the modulus is zero.
    ///  3. the exponent is negative and not -1.
    MODPOW = 0xA6,

    /// Pops the top two stack items and shifts the first pushed item left by the second pushed item(the top item) bits, preserving sign.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if:
    ///  1. the inputs cannot be converted to integers.
    ///  2. the shift amount is negative or out of the limit.
    SHL = 0xA8,

    /// Pops the top two stack items and shifts the first pushed item right by the second pushed item(the top item) bits, preserving sign.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if:
    ///  1. the inputs cannot be converted to integers.
    ///  2. the shift amount is negative or out of the limit.
    SHR = 0xA9,

    /// Pushes true if the input is false value (false, null, zero), false otherwise.
    NOT = 0xAA,

    /// Pops the top two stack items and pushes true if both items are true value (true, not null, not zero), false otherwise.
    BOOLAND = 0xAB,

    /// Pops the top two stack items and pushes true if either item is true value (true, not null, not zero), false otherwise.
    BOOLOR = 0xAC,

    /// Pops the top stack item and pushes true if the item is not 0, false otherwise.
    /// If the input is not an integer, it will be converted to an integer.
    /// The execution will be faulted if the input cannot be converted to an integer.
    NZ = 0xB1,

    /// Pops the top two stack items and pushes true if the items are equal in number, false otherwise.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    NUMEQUAL = 0xB3,

    /// Pops the top two stack items and pushes true if the items are not equal in number, false otherwise.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    NUMNOTEQUAL = 0xB4,

    /// Pops the top two stack items and pushes true if the first pushed item is less than the second pushed item(the top item), false otherwise.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    LT = 0xB5,

    /// Pops the top two stack items and pushes true if the first pushed item is less than or equal to the second pushed item(the top item), false otherwise.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    LE = 0xB6,

    /// Pops the top two stack items and pushes true if the first pushed item is greater than the second pushed item(the top item), false otherwise.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    GT = 0xB7,

    /// Pops the top two stack items and pushes true if the first pushed item is greater than or equal to the second pushed item(the top item), false otherwise.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    GE = 0xB8,

    /// Pops the top two stack items and pushes the minimum of the two items.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    MIN = 0xB9,

    /// Pops the top two stack items and pushes the maximum of the two items.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    MAX = 0xBA,

    /// Pops the top three stack items and pushes true if the first pushed item is within the specified range [left, right), false otherwise.
    /// If the inputs are not integers, they will be converted to integers.
    /// The execution will be faulted if the inputs cannot be converted to integers.
    WITHIN = 0xBB,

    /// A value n is taken from top of main stack.
    /// The next n*2 items on main stack are removed, put inside n-sized map and this map is put on top of the main stack.
    /// If the top item is not an integer, it will be converted to an integer.
    /// The key should be primitive type and if there are the same key, the last one will be used.
    /// The execution will be faulted if:
    ///  1. the top item cannot be converted to integers.
    ///  2. Any key is not a primitive type.
    PACKMAP = 0xBE,

    /// A value n is taken from top of main stack.
    /// The next n items on main stack are removed, put inside n-sized struct and this struct is put on top of the main stack.
    /// If the top item is not an integer, it will be converted to an integer.
    /// The execution will be faulted if the top item cannot be converted to integers.
    PACKSTRUCT = 0xBF,

    /// A value n is taken from top of main stack.
    /// The next n items on main stack are removed, put inside n-sized array and this array is put on top of the main stack.
    /// If the top item is not an integer, it will be converted to an integer.
    /// The execution will be faulted if the top item cannot be converted to integers.
    PACK = 0xC0,

    /// A collection is removed from top of the main stack.
    /// Its elements are put on top of the main stack (in reverse order) and the collection size is also put on the main stack.
    UNPACK = 0xC1,

    /// An empty array (with size 0) is put on top of the main stack.
    NEWARRAY0 = 0xC2,

    /// A value n is taken from top of main stack. A null-filled array with size n is put on top of the main stack.
    NEWARRAY = 0xC3,

    /// An array of type T with size n filled with the default value of type T(false, 0, empty string or null) is put on top of the main stack.
    /// If the top item is not an integer, it will be converted to an integer.
    /// The next byte of this OpCode is the type identifier.
    /// The execution will be faulted if:
    ///  1. the top item cannot be converted to integer.
    ///  2. the type operand is not a valid stack item type.
    NEWARRAY_T = 0xC4,

    /// An empty struct (with size 0) is put on top of the main stack.
    /// NOTE: It's not used in `neo-lang` compiler.
    NEWSTRUCT0 = 0xC5,

    /// A value n is taken from top of main stack. A null-filled struct with size n is put on top of the main stack.
    /// If the top item is not an integer, it will be converted to an integer.
    /// The execution will be faulted if the top item cannot be converted to integer.
    /// NOTE: It's not used in `neo-lang` compiler.
    NEWSTRUCT = 0xC6,

    /// A empty map is created and put on top of the main stack.
    NEWMAP = 0xC8,

    /// Pop the top item and push its size. The top item should be an array, map, buffer or primitive type.
    /// If the top item is an array or map, push its count.
    /// If the top item is a buffer or primitive type, push its size(in bytes).
    SIZE = 0xCA,

    /// An input index n (or key) and an array (map, buffer or string) are removed from the top of the main stack.
    /// Pushes true to the stack if array[n](map[key], or the n-th byte of buffer/string) exist, and false otherwise.
    ///
    /// If the target is an array, buffer or string, the index will be converted to an integer.
    /// The index is the second pushed item(the top item), and the tartget is the first pushed item.
    ///
    /// The execution will be faulted if:
    ///  1. The target is an array, buffer or string and the index cannot be converted to integer or is out of range.
    ///  2. The target is a map and the key is not a primitive type.
    HASKEY = 0xCB,

    /// A map is taken from top of the main stack. The keys of this map are put on top of the main stack.
    KEYS = 0xCC,

    /// An array or map is taken from top of the main stack. The values of this array or map are put on top of the main stack.
    VALUES = 0xCD,

    /// An input index n (or key) and an array (map, buffer or primitive type) are taken from main stack.
    /// Element array[n], or map[n], or buffer[n], or the n-th byte of primitive type(converted to integer) is put on top of the main stack.
    ///
    /// If the target is an array, buffer or primitive type, the index will be converted to an integer.
    /// The index is the second pushed item(the top item), and the tartget is the first pushed item.
    ///
    /// The execution will be faulted if:
    ///  1. The target is an array, buffer or primitive type and the index cannot be converted to integer or is out of range.
    ///  2. The target is a map and the key is not a primitive type.
    PICKITEM = 0xCE,

    /// The item on top of main stack is removed and appended to the second item on top of the main stack.
    /// When we use this opcode, we should dup the second item on top of the main stack before using it.
    APPEND = 0xCF,

    /// A value, index n (or key) and an array (or buffer, map) are taken from main stack.
    /// Attribution array[n] = value (or buffer[n] = value, map[key] = value) is performed.
    ///
    /// The `value` is the third pushed item(the top item), the `n` or `key` is the second pushed item, and target is the first pushed item.
    /// If the target is an array or buffer, the index will be converted to an integer.
    /// If the tartget is a buffer, the value should within [-128, 255].
    SETITEM = 0xD0,

    /// An array or buffer is removed from the top of the main stack and its elements are reversed.
    REVERSEITEMS = 0xD1,

    /// An input index n (or key) and an array (or map)are removed from the top of the main stack.
    /// Element array[n] (or map[key]) is removed.
    ///
    /// The index or key is the second pushed item(the top item), and the array or map is the first pushed item.
    /// If the target is an array, the index will be converted to an integer.
    ///
    /// The execution will be faulted if:
    ///  1. The target is an array and the index cannot be converted to integer or is out of range.
    ///  2. The target is a map and the key is not a primitive type.
    REMOVE = 0xD2,

    /// Remove all the items from the compound-type.
    /// Using this opcode will need to dup the compound-type before using it.
    CLEARITEMS = 0xD3,

    /// Remove the last element from an array, and push it onto the stack.
    /// Using this opcode will need to dup the array before using it.
    POPITEM = 0xD4,

    /// Pop the top item and push a bool value indicating whether the item is null.
    ISNULL = 0xD8,

    /// Pop the top item and push a bool value indicating whether the item is of the specified type.
    /// The next byte of this OpCode is type identifier.
    /// The execution will be faulted if the type operand(uint8) is invalid.
    ISTYPE = 0xD9,

    /// Pop the top item and convert it to the specified type, then push the converted item.
    /// The next byte of this OpCode is type identifier.
    /// The execution will be faulted if:
    ///  1. The top item cannot convert to the specified type.
    ///  2. The type operand(uint8) is invalid.
    CONVERT = 0xDB,

    /// Pops the top stack item. Then, turns the vm state to FAULT immediately, and cannot be caught.
    /// The top stack value is used as reason. The top item should be string or can be converted to string.
    ABORTMSG = 0xE0,

    /// Pops the top two stack items.
    /// If the second-to-top stack value is false value (false, null, zero), exits the vm execution and sets the vm state to FAULT.
    /// In this case, the top stack value is used as reason for the exit. Otherwise, it is ignored.
    /// The top item should be string or can be converted to string.
    /// The execution will be faulted if the top item is not string or cannot be converted to string.
    ASSERTMSG = 0xE1,
}

pub trait ToOpCode {
    fn to_op_code(&self) -> OpCode;
}

impl ToOpCode for BinaryOp {
    fn to_op_code(&self) -> OpCode {
        match *self {
            BinaryOp::Mul => OpCode::MUL,
            BinaryOp::Div => OpCode::DIV,
            BinaryOp::Mod => OpCode::MOD,
            BinaryOp::Add => OpCode::ADD,
            BinaryOp::Sub => OpCode::SUB,
            BinaryOp::Shl => OpCode::SHL,
            BinaryOp::Shr => OpCode::SHR,
            BinaryOp::BitAnd => OpCode::AND,
            BinaryOp::BitOr => OpCode::OR,
            BinaryOp::BitXor => OpCode::XOR,
            BinaryOp::Eq => OpCode::EQUAL,
            BinaryOp::Ne => OpCode::NOTEQUAL,
            BinaryOp::Lt => OpCode::LT,
            BinaryOp::Le => OpCode::LE,
            BinaryOp::Gt => OpCode::GT,
            BinaryOp::Ge => OpCode::GE,
            BinaryOp::And => OpCode::AND,
            BinaryOp::Or => OpCode::OR,
        }
    }
}

impl ToOpCode for AssignOp {
    fn to_op_code(&self) -> OpCode {
        match *self {
            AssignOp::PlusAssign => OpCode::ADD,
            AssignOp::MinusAssign => OpCode::SUB,
            AssignOp::StarAssign => OpCode::MUL,
            AssignOp::SlashAssign => OpCode::DIV,
            AssignOp::PercentAssign => OpCode::MOD,
            AssignOp::ShrAssign => OpCode::SHR,
            AssignOp::ShlAssign => OpCode::SHL,
            AssignOp::AmpAssign => OpCode::AND,
            AssignOp::PipeAssign => OpCode::OR,
            AssignOp::CaretAssign => OpCode::XOR,
            AssignOp::Assign => OpCode::DUP, // maybe no-op?
        }
    }
}
