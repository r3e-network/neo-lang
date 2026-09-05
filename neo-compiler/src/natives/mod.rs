//! External contract declarations loaded from `.d.neo` files.
//!
//! Built-in Neo native contracts are embedded from `std/native/*.d.neo` and
//! implicitly available in every compilation unit (like Java's `java.lang`).

mod load;

pub use load::{
    lang_type_assignable_to, load_decl_file, native_contract_by_name, parse_hash160_hex,
    ExternContract, ExternContractError, ExternMethod,
};

#[cfg(test)]
mod tests;
