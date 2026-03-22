use std::fmt::Display;

use crate::cli_args::{EjectArgs, Session};

pub mod search;
pub mod set_resource;

pub use search::{SearchResult, search_config};
pub use set_resource::set_user_xresource;

pub fn eject_config(_args: &EjectArgs, _session: Session) -> Box<dyn Display> {
    todo!()
}
