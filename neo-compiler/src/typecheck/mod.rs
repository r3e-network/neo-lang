//! Type checking runs inside [`super::Codegen::codegen_source_file`] before lowering.
//!
//! Expression types must match declarations and operators (strong typing).

mod builtin;
mod call;
mod context;
mod error;
mod expr;
mod map_key;
mod source;
mod stmt;

#[cfg(test)]
mod tests;

pub use error::TypeError;
