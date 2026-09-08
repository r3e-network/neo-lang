use crate::diagnostic::Span;
use crate::syntax::ast::Type;

use super::error::{err_at, TypeError};

/// Every `map[K, V]` in a type must use an allowed key type; recurse into `V` and array elements.
pub(super) fn check_map_key_rules_in_type(ty: &Type, span: Span) -> Result<(), TypeError> {
    match ty {
        Type::Map { key, value } => {
            if !key.is_valid_map_key_type() {
                return Err(err_at(
                    span,
                    format!(
                        "map key type must be bool, int, string, hash160, or hash256, got `{key:?}`"
                    ),
                ));
            }
            check_map_key_rules_in_type(value, span)?;
        }
        Type::Array(el) => check_map_key_rules_in_type(el, span)?,
        _ => {}
    }
    Ok(())
}
