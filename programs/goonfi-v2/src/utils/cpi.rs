//! Cross-program-invocation helpers shared by instruction handlers.

mod route_validation;

pub use goonfi_account_validation::{
    close_program_account, create_or_allocate_account, validate_token_account,
};
pub use goonfi_account_validation::decode_quote_route;
pub use route_validation::{classify_quote_route, RouteClass};
